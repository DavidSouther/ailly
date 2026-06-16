# Feature 3 Design: e2e + testing

**Project:** [../design.md](../design.md) | **Plan:** [../plan.md](../plan.md) | **Research:** [../research.md](../research.md)
**Type:** Feature (the e2e + testing layer; Feature 3 of 3) | **Status:** Review
**Approach:** A from project design §5, building on Feature 1 (harness) and Feature 2 (web tools), both DONE and green.
**Depends on:** Feature 1 (`Conversation::run` tool loop, `meta.tools`, `NoopToolExecutor`), Feature 2 (`web_search` / `web_fetch` and their `e2e/research/context/tools/*.json` fixtures, already on disk).

## Problem Statement

Features 1 and 2 wired the tool-call machinery and the two web tools. The eval read-path (`extract_tool_uses`, `check_must_call_tool`, `check_tool_call_order` — `assertions.rs:993/1070/1302`) scores `tool_use` blocks, the run loop (`conversation.rs:486`) appends `tool(tool_result) → assistant(text)`, and the Feature 1 feature test `tests/tool_loop.rs` proves the `user → assistant(tool_use) → tool(tool_result) → assistant(text)` shape end to end. What is missing is the **project-level demonstration** that the whole `assemble → run → eval → report` loop scores real tool calls, exercised the way an Ailly user runs it from `ci.sh` — and the `e2e/research/` project the README already advertises but does not ship.

This feature delivers three things:

1. **`e2e/research/`** — a new in-repo e2e project: an assembly declaring `web_search` + `web_fetch`, a research prompt that needs both, an eval suite asserting `must_call_tool` + `tool_call_order`, a `ci.sh`, and the docs (`README.md`, `AGENTS.md`). It mirrors `e2e/insurance-claim/` and the DDD `research/e2e` layout.
2. **The insurance-claim structural multi-turn gate** — a CI gate over `e2e/insurance-claim/` proving the now-live tool-call assertions (`must_call_tool: lookup_policy`, `tool_call_order: [lookup_policy, lookup_claim_history]`, `must_not_call_tool: auto_approve`) fire on multi-turn tool conversations, and updating `README.md`'s now-stale "Current limitations" section.
3. **A Rust e2e test** mirroring `tests/e2e_delegate_52.rs` / `tests/e2e_patterns_eval.rs`, scoring the research suite over synthetic tool-call conversations.

### The load-bearing gap: noop tool-result scripting

The project plan's surfaced assumption (plan.md §Self-Review, §Controlling-agent review) is the real problem this feature solves: **the CLI `run` path cannot produce tool calls under a noop model.** Grounded in the code:

- `open_engine_for_model("noop")` returns `NoopEngine::auto()` (`engine.rs:122-125`). `auto()` has `auto_fill: true` and an empty script queue; on every `complete()` it emits `ScriptEntry::AutoStamp(Content::Text(format!("{NOOP_MODEL}-{index}")))` (`engine.rs:255-257`). It can **only** produce `Content::Text("noop-N")` — never a `Content::Blocks([ToolUse{..}])`.
- `cli/run.rs:97` constructs `NoopToolExecutor::default()` — the **empty** executor (`tools/mod.rs:58-67`). Any `tool_use` it is handed returns `ToolError::NoopExhausted` (`mod.rs:104-113`), which bubbles as `RunCmdError::Tool`.
- The run loop (`conversation.rs:486-540`) only walks into the tool-turn branch when the just-filled slot carries `ToolUse` blocks (`:504-511`). Under `NoopEngine::auto()` no slot ever does, so the loop is a plain text-fill loop and the empty executor is never called — `tests/run.rs` and `tests/single_file...` prove this no-tool-call path is green.

The consequence: a `ci.sh` cannot get a tool call out of `ailly run` against a `model: noop` conversation. The only two ways to script a tool call are exactly what `tests/tool_loop.rs` uses — `NoopEngine::from_scripts` (engine emits the `tool_use`) plus `NoopToolExecutor::from_scripts` (executor emits the `tool_result`) — and neither is reachable from the CLI, which hard-codes `NoopEngine::auto()` and `NoopToolExecutor::default()`. Feature 1 deliberately scoped a real CLI executor registry to a follow-on (F1 design §"CLI `run` wiring"; project design §6). This feature must demonstrate the tool-call shape **without** building that registry and **without** a live API. The resolution (below) is the pre-filled-fixture mechanism.

## Prior Art (in-repo patterns this feature mirrors)

- **`tests/e2e_delegate_52.rs` / `tests/e2e_patterns_eval.rs`** — the e2e Rust-test pattern: inline synthetic conversation YAML strings (each a complete `meta` + filled turns, `model: noop`), written to a tmp run dir, scored by calling `eval_run(EvalCmdArgs { project, suite, over })` against the real in-repo project's eval suite, then asserting `outcome.conversations_matched` / `assertions_passed` / `_failed` / `_deferred` and the report-JSON totals. The research e2e test mirrors this exactly, with `tool_use` blocks in the synthetic bodies so `must_call_tool` / `tool_call_order` fire.
- **`tests/tool_loop.rs`** (Feature 1 feature test) — the only working recipe for producing a tool call under noop: `NoopEngine::from_scripts(vec![CompletionResponse { content: Content::Blocks([ToolUse{..}]), .. }, CompletionResponse { content: Content::Text(..), .. }])` + `NoopToolExecutor::from_scripts([(tool_name, vec![result_string])])`. This feature's understanding of "what can emit a tool call" is grounded here.
- **`e2e/insurance-claim/`** — the structural model for a full in-repo e2e project: `AGENTS.md`, `assemblies/<name>.yaml`, `prompts/<case>.md`, `context/{system,tools,examples,knowledge}/`, `evals/<suite>.yaml` + `evals/reports/`, `ci.sh`, `.gitignore`. The research project copies this shape. Its `ci.sh` is the gate model: CUJ 1 `assemble` always runs; CUJ 2 `run` is gated on `ANTHROPIC_API_KEY` (or a project `.env`); CUJ 3 `eval`; CUJ 4 `report`. Both insurance-claim and research `ci.sh` invoke `cargo run --quiet -- -p "${project_dir}" <cmd>` from `repo_root`.
- **`~/devel/davidsouther/domain-driven-design/research/e2e/`** — the upstream research-eval layout (`assemblies/{discovery,baseline,invocation}.yaml`, `prompts/<suite>/<case>.md`, `evals/<suite>.yaml`, `evals/scripts/*.py`, `context/`, `ci.sh`, `AGENTS.md`, `.gitignore`). Ailly's `e2e/research/` borrows its directory shape but is scoped to one suite and the two web tools (research.md "Research e2e scope? Two tools only").
- **The internally-tagged `Assertion` schema** (`content/evaluation.rs`: `MustCallTool{tool}` :63, `MustNotCallTool{tool}` :67, `ToolCallOrder{sequence}` :75, all under `#[serde(tag = "type")]` :37). The research evals use `{ type: must_call_tool, tool: web_search }`, `{ type: must_call_tool, tool: web_fetch }`, `{ type: tool_call_order, sequence: [web_search, web_fetch] }` — the live `e2e/insurance-claim/evals/regression.yaml` is the working example.

## Metrics — what "green" means

- **`e2e/research/ci.sh` exits 0.** The assemble half always runs and asserts the expected conversation count; the run/eval/report half follows the insurance-claim gating (live model gated on `ANTHROPIC_API_KEY`, structural-noop half always runs). This is Feature 3's executable feature test.
- **The insurance-claim structural gate exits 0.** It proves `must_call_tool` / `tool_call_order` now *pass* on multi-turn tool conversations (the capability the README's "Current limitations" said was impossible), with no live API.
- **The Rust e2e test passes** *for the reason the suite is wired to test*: the research suite scores `must_call_tool: web_search`, `must_call_tool: web_fetch`, and `tool_call_order: [web_search, web_fetch]` as passing over synthetic tool-call conversations — modulo the pre-existing `over`-path harness limitation characterized below, which it shares with the other two `eval_*` tests and which this feature does not resolve.
- **No live API, no live web call.** Every gate and test runs through `NoopEngine` + `NoopToolExecutor` or over pre-filled fixtures. `mise run check` / `test` / `lint` green; `mise run format` clean; `cargo nextest run --all-features --all-targets` introduces no new failures.

## Specification

### Open #3 (resolved): the noop tool fixture is a pre-filled standalone conversation, committed under the project, NOT generated by `ci.sh`

**Decision:** for both `e2e/insurance-claim` and `e2e/research`, the structural tool-call gate runs over **pre-filled, committed conversation fixtures** that already contain the full `user → assistant(tool_use) → tool(tool_result) → assistant(text)` shape with **no blank assistant slots**. `ailly run` is *not* used to produce the tool calls; it is either skipped in the structural gate or run as a verified no-op pass-through (zero blanks → byte-identical, proven by `tests/run.rs::no_blank_assistant_is_a_byte_identical_no_op`). `ailly eval` then scores the tool calls already present in the fixtures.

This resolves the open-#3 standalone-vs-generated question and is forced by, and consistent with, the noop-scripting analysis in the Problem Statement:

- **`ci.sh`-generated-with-injected-scripts is not reachable.** Generating the fixture from the assembly would require `ailly run` to emit a `tool_use`, which under `NoopEngine::auto()` is impossible (it only emits `Content::Text`). The CLI exposes no flag to inject a `NoopEngine::from_scripts` / `NoopToolExecutor::from_scripts`; building one is the deferred executor-registry / scripted-engine-source work (project design §6). A standalone pre-filled fixture sidesteps that entirely.
- **It honors the conversation-as-artifact guarantee.** A conversation file is the complete run artifact (README "Conversation-as-run-artifact replay"; project design §4). A committed conversation that already carries `tool_use → tool_result → text` *is* a valid run artifact — it is exactly what a live `ailly run` would have produced and saved. Scoring it with `eval` is the same code path that scores a freshly-run conversation; nothing about the fixture being hand-authored changes how `extract_tool_uses` reads it.
- **It mirrors the established e2e Rust-test convention.** `tests/e2e_delegate_52.rs` and `tests/e2e_patterns_eval.rs` already score **synthetic pre-filled conversation YAML** (with `tool_use` blocks, in delegate-52's case) over the real eval suites. The pre-filled-fixture mechanism is not a new idea; it is the convention these two tests already embody, extended to a shippable on-disk fixture and a `ci.sh` gate.

#### The noop tool-result scripting mechanism, precisely

The "scripted executor" the project plan asks about resolves to: **there is no executable blank for the executor to fill.** The mechanism is a pre-filled fixture conversation whose `tool_use` and `tool_result` blocks are authored literally, so:

- the `tool_use` blocks are read by `eval`'s `extract_tool_uses` exactly as if a model had emitted them — `must_call_tool` and `tool_call_order` fire on them directly;
- the `tool_result` blocks sit in a `role: tool` message as data, satisfying the agentic shape for the structural assertion without any executor call;
- `NoopToolExecutor` (and thus the empty `default()` at `cli/run.rs:97`) is never invoked, because there is no blank assistant slot after a `tool_use` for the loop to fill — the fixture is complete.

Contrast with how `NoopEngine::auto()` sources replies today: the engine *fills blank assistant slots* with `"noop-N"` text in call order (`engine.rs:255`); the run loop walks blank slots (`conversation.rs:491 while let Some(index) = self.next_blank_assistant()`). The pre-filled fixture has **no blank slots**, so the engine is never asked for a reply and the loop body never runs. The fixture *is* the script. This is the smallest mechanism consistent with the project's conventions: it reuses the synthetic-conversation pattern the e2e tests already use, adds no CLI surface, and builds no executor registry.

(The alternative — a scripted CLI executor source, e.g. an `AILLY_NOOP_TOOL_SCRIPT` env var threaded into `cli/run.rs` to build a `NoopToolExecutor::from_scripts` plus a scripted `NoopEngine` — is rejected below as scope the project deferred. The pre-filled fixture obtains the same demonstration with zero new production code.)

### `e2e/research/` layout

Mirrors `e2e/insurance-claim/` and the DDD `research/e2e`. The `context/tools/*.json` are Feature 2's deliverable, already on disk and unit-tested (`web.rs::web_search_json_round_trips_into_tool_definition`, `..web_fetch..`); Feature 3 references them, never re-creates them.

```
e2e/research/
├── AGENTS.md                          # Project constitution; named at prefix position 0.
├── .gitignore                         # ignores runs/ and evals/reports/ (mirror insurance-claim/.gitignore).
├── context/
│   ├── system/
│   │   └── 00-research-policy.md       # "You are a research assistant. Search before you fetch; cite what you fetch."
│   └── tools/
│       ├── web_search.json             # Feature 2 (already on disk) — referenced, not created.
│       └── web_fetch.json              # Feature 2 (already on disk) — referenced, not created.
├── prompts/
│   └── web-research.md                 # One research question that needs both tools: search for a source, then fetch it.
├── assemblies/
│   └── research.yaml                   # prefix declares { kind: tools, path: context/tools/*.json }; one-case matrix.
├── fixtures/
│   └── web-research.yaml               # Pre-filled noop conversation (open #3): user → assistant(tool_use web_search)
│                                       #   → tool(tool_result) → assistant(tool_use web_fetch) → tool(tool_result)
│                                       #   → assistant(text). The structural gate's run artifact.
├── runs/                               # gitignored; ailly assemble writes skeletons here.
└── evals/
    ├── research.yaml                   # must_call_tool web_search + web_fetch; tool_call_order [web_search, web_fetch].
    └── reports/                        # gitignored.
```

**`assemblies/research.yaml`** (mirrors `claim-handler.yaml`'s prefix shape; one matrix case so `assemble` produces one skeleton):

```yaml
name: research
model: claude-sonnet-4-6

matrix:
  case: [web-research]

prefix:
  - { kind: file,   path: ./AGENTS.md,                 cache: true }
  - { kind: system, path: context/system/*.md,         cache: true }
  - { kind: tools,  path: context/tools/*.json,        cache: true }

conversation:
  - { role: user, path: "prompts/{{ case }}.md" }
  - { role: assistant }
```

**`evals/research.yaml`** (internally-tagged `Assertion`; copied shape from `regression.yaml`). The case `name` matches the fixture/skeleton filename stem `web-research`:

```yaml
name: research
cases:
  - name: web-research
    assertions:
      - { type: must_call_tool, tool: web_search }
      - { type: must_call_tool, tool: web_fetch }
      - { type: tool_call_order, sequence: [web_search, web_fetch] }
```

**`fixtures/web-research.yaml`** — the pre-filled noop conversation (open #3). `model: noop`, no blank assistant slot. Shape (one `tool_use` per assistant turn so `tool_call_order` reads `[web_search, web_fetch]` in message-then-block order, matching how `extract_tool_uses` walks):

```yaml
---
model: noop
assembly: research
binding:
  case: web-research
---
role: user
content: "Find the official Rust homepage and fetch its tagline."
---
role: assistant
content:
  - type: tool_use
    id: tu-1
    name: web_search
    input: { query: "rust language official site" }
---
role: tool
content:
  - type: tool_result
    tool_use_id: tu-1
    content: "Rust — https://www.rust-lang.org\nA language empowering everyone."
---
role: assistant
content:
  - type: tool_use
    id: tu-2
    name: web_fetch
    input: { url: "https://www.rust-lang.org" }
---
role: tool
content:
  - type: tool_result
    tool_use_id: tu-2
    content: "HTTP 200 (text/html)\n\nA language empowering everyone to build reliable and efficient software."
---
role: assistant
content: "The Rust homepage tagline is: A language empowering everyone to build reliable and efficient software."
trace:
  span_id: span-research
  model: noop
  tokens: { input: 1200, output: 60 }
  latency_ms: 400
```

### `e2e/research/ci.sh`

Modeled on `e2e/insurance-claim/ci.sh` (the in-repo `cargo run --quiet -- -p "${project_dir}"` form, NOT the DDD reference's released-binary form). Four CUJs:

1. **assemble** (always): `ailly -p . assemble research`; assert exactly one conversation skeleton lands under `runs/<id>/`. (Pure file read; no API.)
2. **structural tool-call gate** (always, noop): copy `fixtures/web-research.yaml` into a fresh run dir `runs/<id>-structural/web-research.yaml`; run `ailly -p . run runs/<id>-structural/` as a verified no-op (the fixture has no blank assistant, so `run` is byte-identical — the gate asserts the file is unchanged); then `ailly -p . eval research --over runs/<id>-structural/` and assert via the report JSON that the three tool-call assertions passed (`assertions.passed >= 3`, `failed == 0`), proving `must_call_tool` + `tool_call_order` fire on the multi-turn shape with no live API.
3. **live run** (gated on `ANTHROPIC_API_KEY` or `.env`): `ailly -p . run runs/<id>/` over the assembled skeleton, then `eval` + `report`. Skipped with a clear notice otherwise, exactly like insurance-claim CUJ 2.
4. **report** (always, over the structural run): `ailly -p . report <structural-id>`; assert the markdown report wrote.

The gate stays POSIX/bash-compatible and self-locating like the insurance-claim script. It reads pass/fail from the eval report JSON with `python3` (matching the DDD `ci.sh`'s `python3 -` heredoc pattern), since `ailly eval` exits non-zero only on hard failures and the structural assertions are expected to pass.

### `AGENTS.md`, `prompts/web-research.md`, `context/system/00-research-policy.md`

- `AGENTS.md` — short project constitution naming the research task and the two tools, mirroring `e2e/insurance-claim/AGENTS.md`.
- `prompts/web-research.md` — one research question requiring a search then a fetch (e.g. "Find the official Rust homepage and fetch its tagline").
- `context/system/00-research-policy.md` — a one-fragment system prompt ("search before you fetch; cite what you fetch"), so the `kind: system` block is non-empty and the assembled prefix is representative.

### The Rust e2e test: `tests/e2e_research.rs`

Mirrors `tests/e2e_patterns_eval.rs`. Inline synthetic conversation YAML (the same shape as `fixtures/web-research.yaml`, with the two `tool_use` blocks and a final text turn), written to a tmp run dir, scored by `eval_run(EvalCmdArgs { project: e2e/research, suite: "research", over: <tmp run dir> })`. Asserts the research suite's three tool-call assertions are read and scored, and the report JSON is written at `evals/reports/<run-id>.json`, with cleanup so the project tree stays pristine.

**Known harness characteristic (shared, not introduced here):** because `over` is a tmp dir outside the project's `host_root`, `cli/mod.rs::project_relative` returns an empty `RunId` (`strip_prefix` fails → `unwrap_or_default()`), so `conversations_repository.list("")` does not find the tmp conversations and `conversations_matched == 0`. This is the identical limitation that makes `eval_insurance_claim.rs`, `e2e_delegate_52.rs`, and `e2e_patterns_eval.rs` all fail at baseline today (all three panic with `left: 0`). The research test is written to the same convention as those three. Resolving the `over`-path limitation is out of scope for Feature 3 (it touches neither tool calls nor `cli/mod.rs::project_relative`); the `ci.sh` structural gate — which runs over a run dir *inside* the project tree — is the authoritative tool-call demonstration and is unaffected by the limitation. The Rust test documents this with a comment pointing at the shared root cause, exactly as the existing two e2e tests' user-story headers describe their own harness assumptions.

### Insurance-claim structural multi-turn gate

Extend `e2e/insurance-claim/` with a committed pre-filled fixture and a structural `ci.sh` gate, parallel to the research project:

- **Fixtures** (open #3, per-case): commit `e2e/insurance-claim/fixtures/{missing-fields,over-limit}.yaml` carrying the exact tool-call shapes the suite asserts — `missing-fields` emits `lookup_policy`; `over-limit` emits `lookup_policy` then `lookup_claim_history` in order. These reuse the synthetic bodies already proven in `tests/eval_insurance_claim.rs` (`CONV_MISSING_FIELDS`, `CONV_OVER_LIMIT`), promoted from inline test constants to on-disk fixtures. The `ambiguous` case calls no tool and is already covered by the existing eval; no tool fixture is needed for it. **No fixture emits `auto_approve`** — emitting it would fail the two `must_not_call_tool: auto_approve` assertions and invert the gate's intent (verified against `regression.yaml:12/18`).
- **`ci.sh` structural gate**: after the existing CUJ 1 assemble, add a noop structural step that copies the per-case fixtures into a run dir, runs `ailly run` as a verified no-op, runs `ailly eval regression --over <fixture-run-dir>`, and asserts via the report JSON that `must_call_tool: lookup_policy`, `tool_call_order: [lookup_policy, lookup_claim_history]`, and `must_not_call_tool: auto_approve` all pass (the suite's `judge` assertion on `over-limit` defers, as it does today — no engine wired). The existing live half (CUJ 2, gated on `ANTHROPIC_API_KEY`) is preserved unchanged.
- **`README.md` doc-sync**: the "Current limitations" section (`README.md:181-200`) is now stale. Feature 1 removed the `tools: Vec::new()` limitation; the first bullet ("Tool definitions are rendered into a system message... the rig adapter sends `tools: Vec::new()` unconditionally... `must_call_tool` and `tool_call_order` assertions fail") is rewritten to describe the now-live behavior: `kind: tools` resolves to `meta.tools`, the rig adapter forwards them, and the structural gate proves `must_call_tool` / `tool_call_order` pass on multi-turn tool conversations. The `judge`-deferred bullet stays (still true). Per the documentation-sync rule this edit ships with the gate, not as a follow-up.

### Build steps (≤7; each leaves `check` + tests + scripts green)

1. **`e2e/research/` skeleton** — `AGENTS.md`, `.gitignore`, `context/system/00-research-policy.md`, `prompts/web-research.md`; reference the on-disk `context/tools/*.json`. `assemble` produces one skeleton.
2. **research assembly + eval suite** — `assemblies/research.yaml` (declares both tools), `evals/research.yaml` (the three tool-call assertions).
3. **research noop fixture** — `fixtures/web-research.yaml` (the pre-filled multi-turn tool conversation; open #3).
4. **research `ci.sh`** — four-CUJ gate; structural-noop half always runs and exits green; live half gated on `ANTHROPIC_API_KEY`.
5. **`tests/e2e_research.rs`** — the Rust e2e test mirroring `tests/e2e_patterns_eval.rs`.
6. **insurance-claim structural gate** — promote the two per-case fixtures, extend `ci.sh`, doc-sync `README.md`.

(Six steps; within the 7-step ceiling. No production `src/` code changes — this feature is e2e projects, fixtures, a `ci.sh` pair, one Rust test, and a doc edit.)

## tests/eval_insurance_claim.rs:147 — verdict

**Verdict: `:147` is a pre-existing, unrelated harness limitation. Feature 3 does NOT make it pass, and that is correct — it is a tolerated baseline failure shared by all three `eval_*` tests.**

`:147` is `assert_eq!(outcome.conversations_matched, 3)`; it panics with `left: 0, right: 3`. Traced to source:

- The test writes three synthetic conversations to a tmp dir (`tmp.path().join(run_id)`) that is **outside** the project root `e2e/insurance-claim`.
- `eval_run` resolves the listing key with `super::project_relative(&project, &args.over)` (`cli/eval.rs:102`). `project_relative` (`cli/mod.rs:15-30`) canonicalizes `over` and `strip_prefix`es the project host root; for a tmp dir outside the root, `strip_prefix` fails and it returns an empty `RunId` via `unwrap_or_default()`.
- `conversations_repository.list("")` then lists relative to the project root, not the tmp dir, finds none of the synthetic files, and `evaluate` reports `conversations_matched = 0`.

This has **nothing to do with tool-call wiring.** The synthetic `CONV_*` strings parse fine (they use the same `tool_use`/`tool_result` block schema the live code reads), the `regression.yaml` suite loads fine, and `extract_tool_uses` / `check_must_call_tool` are already live (Feature 1). The failure is purely the `over`-outside-project path resolution. Proof it is shared and not tool-specific: `e2e_delegate_52.rs` (`:262`, `left: 0`) and `e2e_patterns_eval.rs` (`:284`, `left: 0`) fail identically at baseline, and **neither involves the insurance-claim project nor tool calls** — `delegate-52` and `patterns-eval` are unrelated e2e projects with their own (already-present) eval suites.

Feature 3 does not touch `tests/eval_insurance_claim.rs`, `cli/eval.rs`, or `cli/mod.rs::project_relative`. The plan does not assign the `over`-path fix to this project (project design §6 lists no such item; research.md does not name it). The authoritative tool-call demonstration this feature ships — the insurance-claim and research `ci.sh` structural gates — runs `eval` over a run dir **inside** the project tree, where `project_relative` resolves correctly, so the gates prove the tool-call assertions pass without depending on the broken `over`-path. Fixing `:147` (and its two siblings) is a separate, pre-existing bug whose root cause and one-line locus (`project_relative` returning empty on an out-of-root `over`) are documented here for whoever picks it up; it stays a tolerated baseline failure for this project.

## Pre-existing e2e characterization: `e2e_delegate_52` and `e2e_patterns_eval`

Both fail at baseline for the **same** root cause as `:147` and are confirmed **out of tool-calls scope**:

- `e2e_delegate_52.rs::delegate_52_slice_scores_corruption_suite_end_to_end` panics at `:262` (`assert_eq!(outcome.conversations_matched, 12)`, `left: 0`). It scores the `e2e/delegate-52/evals/corruption.yaml` suite (which exists on disk) over six synthetic conversations in a tmp dir. Same `over`-outside-root resolution → 0 matches. Delegate-52 is a document-corruption protocol fixture; it declares no tools and exercises program/judge scorers, not the tool loop.
- `e2e_patterns_eval.rs::patterns_eval_slice_evaluates_both_suites_end_to_end` panics at `:284` (`assert_eq!(discovery_outcome.conversations_matched, 6)`, `left: 0`). It scores `e2e/patterns-eval/evals/{discovery,invocation}.yaml` (both present) over synthetic conversations in tmp dirs. Same resolution → 0 matches. Patterns-eval is a skill-discovery/invocation fixture; no tools, no tool loop.

Feature 3 touches neither project: it adds `e2e/research/`, two committed fixtures + a structural gate under `e2e/insurance-claim/`, and one new Rust test. It does not modify `e2e/delegate-52/`, `e2e/patterns-eval/`, `tests/e2e_delegate_52.rs`, or `tests/e2e_patterns_eval.rs`. These two remain tolerated baseline failures with the documented shared root cause; this feature neither fixes nor regresses them.

## Alternatives

| Approach | Tool calls in CI come from | New production code | Honors conv-as-artifact | Verdict |
|---|---|---|---|---|
| **A: pre-filled committed fixture + `eval`** | hand-authored `tool_use`/`tool_result` in a committed conversation; `run` is a no-op | none | yes (a complete run artifact) | **chosen** |
| B: scripted CLI executor source (`AILLY_NOOP_TOOL_SCRIPT` env → `NoopToolExecutor::from_scripts` + scripted `NoopEngine` threaded through `cli/run.rs`) | the CLI building a scripted engine + executor from an env var | a CLI flag/env, a scripted-engine source, executor threading | yes, but via new CLI surface | rejected — builds the deferred executor-registry/scripted-source machinery (project §6) for a demo; larger than the problem |
| C: live API only (`ANTHROPIC_API_KEY`) | a real model | none | yes | rejected as the *sole* path — research.md mandates a noop CI half ("noop run — no live API"); live stays an optional gated half |

**Why A over B.** B is the literal "scripted executor sourced from X" the project plan flagged as a possible mechanism, but it requires a new CLI surface (an env var or flag), a way to source a scripted `NoopEngine` *and* a scripted `NoopToolExecutor` into `cli/run.rs` (which today hard-codes `NoopEngine::auto()` + `NoopToolExecutor::default()`), and the executor registry the project explicitly deferred (project design §6: "a real executor registry is a follow-on concern"). A obtains the identical demonstration — the `tool_use → tool_result → text` shape scored by `eval` — with zero production code, reusing the synthetic-conversation convention `tests/e2e_delegate_52.rs` already ships. YAGNI: the CI gate needs to *prove the assertions fire on the shape*, not to *re-derive the shape from a live loop*; the loop's correctness is already pinned by `tests/tool_loop.rs`.

**Why A over C.** The research e2e must run in CI without credentials (research.md, project design Closing-Bell task 4 "Fully automatable — exits 0, no fix-ups"). C alone cannot. A runs unconditionally; C rides as the optional `ANTHROPIC_API_KEY`-gated half of the same `ci.sh`, identical to insurance-claim CUJ 2.

**Build vs off-the-shelf.** No off-the-shelf harness produces a deterministic tool-call conversation that Ailly's `eval` can score; the fixture *is* the artifact, and authoring it is the smallest possible mechanism.

## Summary

- **Open #3 — resolved: pre-filled, committed standalone conversation fixtures, NOT `ci.sh`-generated.** Forced by the noop-scripting analysis: `NoopEngine::auto()` can only emit `Content::Text`, and `cli/run.rs` hard-codes the empty `NoopToolExecutor::default()`, so the CLI cannot produce a tool call. A complete pre-filled fixture (no blank assistant slot → `run` is a verified no-op; the fixture *is* the script) lets `eval` score `tool_use`/`tool_result` blocks directly. Both `e2e/research/` and `e2e/insurance-claim/` use this shape.
- **Noop tool-result scripting mechanism: a pre-filled fixture conversation with no executable blanks.** The executor is never called; `extract_tool_uses` reads the authored `tool_use` blocks exactly as it would model-emitted ones. This reuses the synthetic-conversation convention `tests/e2e_delegate_52.rs` / `tests/e2e_patterns_eval.rs` already embody, adds no CLI surface, and builds no executor registry — consistent with how `NoopEngine::auto()` fills blank slots (the fixture simply has none) and how `run` walks blanks (none to walk).
- **`:147` verdict: tolerated pre-existing failure, NOT fixed by Feature 3.** `conversations_matched == 0` because `over` is a tmp dir outside the project root and `project_relative` returns an empty `RunId`; the same root cause fails `e2e_delegate_52` (`:262`) and `e2e_patterns_eval` (`:284`). None involve tool-call wiring. The `ci.sh` structural gates run `eval` over an in-tree run dir where the path resolves, so they prove the tool-call assertions without touching the bug.
- **`e2e/research/` + the Rust e2e test + the insurance-claim structural gate** ship per the layout above, mirroring `e2e/insurance-claim/` and the DDD `research/e2e`. README doc-sync removes the now-stale "tools rendered as system text / `tools: Vec::new()`" limitation. No `src/` production changes; six build steps, within the 7-step ceiling.
- **Deferred (unchanged from project §6):** a real CLI executor registry and a scripted-engine CLI source (alternative B); the `over`-outside-project path fix in `project_relative` (the `:147` family bug); live-API CI as a mandatory rather than gated half.
