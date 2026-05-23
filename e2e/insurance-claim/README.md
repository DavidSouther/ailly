# Insurance claim handler

A worked example of a single-prompt application built with Ailly: a claim-handler context window assembled from declarative parts (system fragments, JSON Schema tool definitions, few-shot exemplars, a retrieved knowledge corpus), run against a user prompt, and graded by a regression suite. The same project doubles as a fixture that demonstrates the four claims Ailly makes for itself.

| Ailly claim | How this project demonstrates it |
|---|---|
| **Context windows built exactly as desired** | The assembly recipe names every fragment in order. The assembled window is written to `runs/<ts>/window.txt` and can be read against the recipe. No proprietary agent loop chooses what gets included. |
| **Plain files, no SDK** | Every input is markdown, JSON Schema, or YAML in version control. Editing the persona or a constraint is a `git diff`, not a code change. |
| **Replayable runs** | Each invocation captures `window.txt`, `response.json`, `trace.json`, and `meta.yaml` (recipe SHA, git SHA, model ID, prompt SHA). Re-running over the same recipe and prompt produces a byte-identical window. |
| **A/B testing with one command** | Two assemblies sweep the same prompt set; `ailly diff` reports behavioural delta (tool calls), textual delta (response), and budget delta (tokens) without a bespoke harness. |

## What the handler does

A claim handler reads an insurance claim and classifies it as `auto-approve`, `human-review`, or `reject`, calling `lookup_policy` and `lookup_claim_history` along the way. The interesting cases are the ones where the policy is ambiguous, fields are missing, or the claim wants the model to short-circuit. The regression suite encodes those cases.

The scenario is intentionally representative of the common end-user-derived skill shape: a small system prompt, a handful of tools, a few exemplars, and a knowledge corpus that grows over time. The team owning this folder is *not* writing inference code; they are writing the context.

## Test surface

```
e2e/insurance-claim/
├── AGENT.md                                          # Project constitution. Pinned, cached.
├── context/
│   ├── system/                                       # System prompt fragments
│   │   ├── 00-persona.md                             # "You are a claims classifier..."
│   │   ├── 10-constraints.md                         # Hard rules (escalate on ambiguity; never auto-approve > $10k).
│   │   └── 20-tools-policy.md                        # When each tool is appropriate.
│   ├── tools/                                        # JSON Schema tool definitions
│   │   ├── lookup_policy.json
│   │   ├── lookup_claim_history.json
│   │   └── auto_approve.json
│   ├── examples/                                     # Few-shot exemplars
│   │   └── classification/
│   │       ├── 01-clean-approve.md
│   │       ├── 02-ambiguous-escalate.md
│   │       └── 03-fraud-reject.md
│   └── knowledge/                                    # RAG corpus (state regs, policy templates, prior memos)
│       └── docs/
├── prompts/
│   ├── default.md                                    # Baseline classification prompt
│   └── edge-cases/
│       ├── missing-fields.md
│       ├── ambiguous.md
│       └── over-limit.md
├── assemblies/
│   └── claim-handler.yaml                            # Recipe that composes the window
├── runs/                                             # Timestamped, replayable outputs
│   └── 2026-05-20T14-32-claim-handler/
│       ├── window.txt                                # The assembled context window, in order
│       ├── response.json                             # Model output (text + tool calls)
│       ├── trace.json                                # Token counts, latencies, cache hits per breakpoint
│       └── meta.yaml                                 # Recipe SHA, git SHA, model ID, prompt SHA
└── evals/
    ├── suites/regression.yaml                        # Assertions over runs
    └── reports/                                      # Pass/fail history per CI step
```

## Assembly (declarative context window)

`assemblies/claim-handler.yaml`:

```yaml
agent_md: ./AGENT.md
system:
  - context/system/00-persona.md
  - context/system/10-constraints.md
  - context/system/20-tools-policy.md
tools:
  - context/tools/lookup_policy.json
  - context/tools/lookup_claim_history.json
  - context/tools/auto_approve.json
examples:
  - context/examples/classification/*.md
retrieval:
  source: context/knowledge/docs/
  query: "{{ user_prompt }}"
  top_k: 5
user_prompt: prompts/default.md
model: claude-opus-4-7
cache_breakpoints: [after_system, after_tools]
```

What this proves about context-window management:

- **Every fragment is named.** The window contains exactly these files, in this order. No agent loop adds or drops anything; the window written to `runs/<ts>/window.txt` matches the recipe verbatim.
- **The cache plan is explicit.** `cache_breakpoints` declares where prompt-cache hits should land; `trace.json` records whether they did.
- **Retrieval is auditable.** `top_k: 5` plus the source folder means the retrieved chunks land in `window.txt` and can be inspected after the fact, not inferred from a vector store.
- **No assembly code.** A new contributor adds a few-shot by dropping a file under `context/examples/classification/`; no Python, no SDK call, no redeploy.

## Evaluation (regression suite asserts on the three things that matter)

A single-prompt application breaks in three ways: it calls the wrong tool, it produces the wrong text, or it bloats the context. The regression suite covers one assertion type per failure mode, plus a single judge case for the rubric work a regex cannot reach.

`evals/suites/regression.yaml`:

```yaml
cases:
  - name: missing-policy-number
    input: prompts/edge-cases/missing-fields.md
    assertions:
      - { type: must_call_tool, tool: lookup_policy }
      - { type: text_contains, value: "policy number required" }
      - { type: tool_call_count, op: "<=", value: 2 }
      - { type: tokens, metric: total, op: "<", value: 8000 }

  - name: ambiguous-claim-escalates
    input: prompts/edge-cases/ambiguous.md
    assertions:
      - { type: must_not_call_tool, tool: auto_approve }
      - { type: text_matches, pattern: "clarif|specif", flags: "i" }
      - { type: tokens, metric: output, op: "<", value: 500 }

  - name: over-limit-rejects-auto-approval
    input: prompts/edge-cases/over-limit.md
    assertions:
      - { type: must_not_call_tool, tool: auto_approve }
      - { type: tool_call_order, sequence: [lookup_policy, lookup_claim_history] }
      - type: judge
        prompt: |
          The response routes the claim to human-review and cites the
          $10,000 auto-approve ceiling from the constraints fragment.
```

What this proves about LLM evaluation:

- **Behavioural assertions** (`must_call_tool`, `must_not_call_tool`, `tool_call_order`) check the agent's actions, not just its words.
- **Textual assertions** (`text_contains`, `text_matches`) pin user-facing language without over-fitting on phrasing.
- **Efficiency assertions** (`tokens`, `latency_ms`) make context bloat a build break, not a quarterly review.
- **Judge assertions** cover rubric cases where a regex would be brittle, but stay the minority because they cost a model call per assertion.

## A/B testing (the advertised win)

The point of putting the assembly under version control is that a one-line edit to a context fragment becomes measurable, not a guess. The flow for "did raising the auto-approve ceiling regress anything?":

```sh
# 1. Snapshot the baseline (current main).
ailly -p e2e/insurance-claim assemble claim-handler
ailly -p e2e/insurance-claim run --suite regression --tag baseline

# 2. Edit context/system/10-constraints.md, then sweep again.
ailly -p e2e/insurance-claim assemble claim-handler
ailly -p e2e/insurance-claim run --suite regression --tag candidate

# 3. Diff the two runs: behaviour, text, and budget side by side.
ailly diff runs/baseline-* runs/candidate-*

# 4. Eval both and compare pass rates.
ailly -p e2e/insurance-claim eval --suite regression --tag baseline,candidate
```

The diff reports tool-call deltas (did the model start calling a different tool?), response-text deltas (semantic match, not byte match), and token deltas per cache breakpoint. A change that fixes three cases at the cost of breaking one shows up as a number, not a feeling. The same flow drives plugin-version sweeps, model swaps, and persona rewrites: substitute the variable, re-run, diff.

## Workflow at a glance

```
1. Edit context/system/10-constraints.md.
2. `ailly -p e2e/insurance-claim assemble claim-handler` — window rebuilds.
3. `ailly -p e2e/insurance-claim eval --suite regression`.
4. Read evals/reports/<ts>.json. Pass rate is reported per assertion class.
5. Commit. The next change is measured against this baseline.
```

## CI integration

```sh
# One CI step covers both this and the patterns-eval suite; report formats are shared.
ailly -p e2e/insurance-claim eval --suite regression
ailly -p e2e/patterns-eval     eval --suite all
```

The CI threshold is "no regressions against the previous green run". The run that establishes a new baseline is the one with the deliberate change, called out in the PR.
