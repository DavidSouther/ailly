# Insurance claim handler

A worked example of a single-prompt application built with Ailly: a claim-handler context window assembled from declarative parts (system fragments, JSON Schema tool definitions, few-shot exemplars, a retrieved knowledge corpus), run against a user prompt, and graded by a regression suite. The same project doubles as a fixture that demonstrates the four claims Ailly makes for itself.

| Ailly claim | How this project demonstrates it |
|---|---|
| **Context windows built exactly as desired** | The assembly's `prefix:` names every block in order, including the `AGENTS.md` block at position zero. The assembled conversation is written to `runs/<id>/<case>.yaml` with the prefix inlined and can be read against the recipe. No proprietary agent loop chooses what gets included. |
| **Plain files, no SDK** | Every input is markdown, JSON Schema, or YAML in version control. Editing the persona or a constraint is a `git diff`, not a code change. |
| **Replayable runs** | Each binding produces one conversation file: prefix, user turns, filled assistant turns, and per-message inline trace. Re-running over the same assembly and binding produces a byte-identical prefix and the same conversation skeleton; only the model's assistant text varies. |
| **A/B testing with one command** | Two assemblies sweep the same matrix; `ailly diff` reports behavioural delta (tool calls), textual delta (response), and budget delta (tokens) without a bespoke harness. |

## What the handler does

A claim handler reads an insurance claim and classifies it as `auto-approve`, `human-review`, or `reject`, calling `lookup_policy` and `lookup_claim_history` along the way. The interesting cases are the ones where the policy is ambiguous, fields are missing, or the claim wants the model to short-circuit. The regression suite encodes those cases.

The scenario is intentionally representative of a small classification skill: a short system prompt, a handful of tools, a few exemplars, and a knowledge corpus that grows over time. The team owning this folder is *not* writing inference code; they are writing the context.

## Test surface

```
e2e/insurance-claim/
├── AGENTS.md                                         # Project constitution. Named explicitly in the assembly prefix.
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
├── prompts/                                          # One prompt per matrix `case` binding
│   ├── default.md
│   ├── missing-fields.md
│   ├── ambiguous.md
│   └── over-limit.md
├── assemblies/
│   └── claim-handler.yaml                            # prefix + conversation skeleton + matrix
├── runs/                                             # One conversation .yaml per matrix binding
│   └── 2026-05-23T14-32-claim-handler/
│       ├── default.yaml                              # Full transcript: prefix inlined, user turn, filled assistant turn, inline trace
│       ├── missing-fields.yaml
│       ├── ambiguous.yaml
│       └── over-limit.yaml
└── evals/
    ├── regression.yaml                               # Assertions; case `name` matches conversation filename
    └── reports/                                      # Pass/fail history per CI step
```

## Assembly (declarative context window)

`assemblies/claim-handler.yaml`:

```yaml
name: claim-handler
model: claude-opus-4-7

matrix:
  case: [default, missing-fields, ambiguous, over-limit]

prefix:
  - { kind: file,     path: ./AGENTS.md,                                                  cache: true }
  - { kind: system,   path: context/system/*.md,                                          cache: true }
  - { kind: tools,    path: context/tools/*.json,                                         cache: true }
  - { kind: examples, path: context/examples/classification/*.md }
  - { kind: context,  source: "context/knowledge/docs/{{ case }}", glob: "*.md", count: 5 }

conversation:
  - { role: user, path: "prompts/{{ case }}.md" }
  - { role: assistant }
```

What this proves about context-window management:

- **Every prefix block is named.** The conversation contains exactly these blocks, in this order. No agent loop adds or drops anything; `AGENTS.md` is at position zero only because the assembly puts it there. The conversation file in `runs/<id>/<case>.yaml` is the verbatim materialisation.
- **The cache plan rides on the content.** `cache: true` on a prefix block marks the end of that block as a prompt-cache breakpoint; the inline trace in the conversation file records whether the breakpoint hit.
- **Knowledge selection is auditable.** `count: 5` plus the per-case source folder means the knowledge chunks land in the prefix portion of the conversation file and can be inspected after the fact, not inferred from a vector store.
- **No assembly code.** A new contributor adds a few-shot by dropping a file under `context/examples/classification/` (the glob picks it up); no Python, no SDK call, no redeploy.
- **The matrix is the sweep.** `case:` enumerates the bindings; `ailly assemble` writes one conversation skeleton per case. Adding a case is one line plus one file under `prompts/`.

## Evaluation (regression suite asserts on the three things that matter)

A single-prompt application breaks in three ways: it calls the wrong tool, it produces the wrong text, or it bloats the context. The regression suite covers one assertion type per failure mode, plus a single judge case for the rubric work a regex cannot reach.

`evals/regression.yaml`. The case `name` matches the conversation filename produced by the matrix; no `input:` field is needed.

```yaml
cases:
  - name: missing-fields
    assertions:
      - { type: must_call_tool, tool: lookup_policy }
      - { type: text_contains, value: "policy number required" }
      - { type: tool_call_count, op: "<=", value: 2 }
      - { type: tokens, metric: total, op: "<", value: 8000 }

  - name: ambiguous
    assertions:
      - { type: must_not_call_tool, tool: auto_approve }
      - { type: text_matches, pattern: "clarif|specif", flags: "i" }
      - { type: tokens, metric: output, op: "<", value: 500 }

  - name: over-limit
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

## A/B testing

The point of putting the assembly under version control is that a one-line edit to a context fragment becomes measurable, not a guess. The flow for "did raising the auto-approve ceiling regress anything?":

```sh
# 1. Snapshot the baseline (current main). Assemble writes one conversation file per matrix case.
ailly -p e2e/insurance-claim assemble claim-handler                  # → runs/<ts>/{default,missing-fields,ambiguous,over-limit}.yaml
ailly -p e2e/insurance-claim run runs/<ts>/                          # fill assistant turns in each
mv runs/<ts> runs/baseline                                           # tag by directory rename

# 2. Edit context/system/10-constraints.md, then sweep again.
ailly -p e2e/insurance-claim assemble claim-handler
ailly -p e2e/insurance-claim run runs/<ts2>/
mv runs/<ts2> runs/candidate

# 3. Diff the two runs: behaviour, text, and budget side by side.
ailly diff runs/baseline runs/candidate

# 4. Eval both and compare pass rates.
ailly -p e2e/insurance-claim eval regression --over runs/baseline
ailly -p e2e/insurance-claim eval regression --over runs/candidate
```

The diff reports tool-call deltas (did the model start calling a different tool?), response-text deltas (semantic match, not byte match), and token deltas per cache breakpoint. A change that fixes three cases at the cost of breaking one shows up as a number, not a feeling. The same flow drives plugin-version sweeps, model swaps, and persona rewrites: substitute the value, re-run, diff.

## Workflow at a glance

The single-edit workflow is the first two steps of the A/B recipe above, without the second sweep.

```
1. Edit context/system/10-constraints.md.
2. `ailly -p e2e/insurance-claim assemble claim-handler`. N skeleton conversations land in runs/<ts>/.
3. `ailly -p e2e/insurance-claim run runs/<ts>/`. Assistant turns are filled in place.
4. `ailly -p e2e/insurance-claim eval regression --over runs/<ts>/`.
5. Read evals/reports/<ts>.json. Pass rate is reported per assertion class.
6. Commit. The next change is measured against this baseline.
```

## CI integration

```sh
# One CI step covers both this and the patterns-eval suite; report formats are shared.
ailly -p e2e/insurance-claim assemble claim-handler && ailly -p e2e/insurance-claim run runs/<ts>/
ailly -p e2e/insurance-claim eval regression --over runs/<ts>/

ailly -p e2e/patterns-eval assemble discovery   && ailly -p e2e/patterns-eval run runs/<ts>-discovery/
ailly -p e2e/patterns-eval assemble invocation  && ailly -p e2e/patterns-eval run runs/<ts>-invocation/
ailly -p e2e/patterns-eval eval discovery  --over runs/<ts>-discovery/
ailly -p e2e/patterns-eval eval invocation --over runs/<ts>-invocation/
```

The CI threshold is "no regressions against the previous green run". The run that establishes a new baseline is the one with the deliberate change, called out in the PR.

## Current limitations

The project demonstrates the assemble -> run -> eval pipeline end to
end, with two intentionally deferred capabilities that affect what
the regression suite can prove today. Each item is tracked separately
in [docs/developer/TASKS.md](../../docs/developer/TASKS.md):

- **Tool definitions are rendered into a system message rather than
  registered as tools on the engine request.** The `kind: tools`
  prefix block produces a system turn whose body is the concatenated
  JSON of `context/tools/*.json`; the rig adapter sends `tools:
  Vec::new()` unconditionally. Consequence: in a live run, the model
  cannot emit `ToolUse` blocks, so `must_call_tool` and
  `tool_call_order` assertions fail and `must_not_call_tool` passes
  vacuously. Tracked under "engine deferred decisions" (tool-definition
  wiring on requests) in TASKS.md.
- **The `judge` assertion is deferred.** `Assertion::Judge` returns
  `AssertionOutcome::Deferred` in [src/knowledge/assertions.rs](../../src/knowledge/assertions.rs);
  the eval report records the case as `deferred`, which does not fail
  the CLI exit code. Tracked under the `eval-judge` entry in TASKS.md.
