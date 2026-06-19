# Implementation Plan: Feature 3 — e2e + testing

**Design:** [design.md](design.md) | **Project plan:** [../plan.md](../plan.md) | **Project design:** [../design.md](../design.md) | **Research:** [../research.md](../research.md)

**Feature test:** `e2e/research/ci.sh` exits 0 (the Feature 3 executable feature test, design §Metrics).

**User story:** an Ailly user clones the repo and runs `bash e2e/research/ci.sh`; the script drives `assemble -> run (noop, byte-identical no-op over a pre-filled tool fixture) -> eval -> report` and exits 0 without credentials or a live web call, with the eval report proving `must_call_tool: web_search`, `must_call_tool: web_fetch`, and `tool_call_order: [web_search, web_fetch]` passed on the multi-turn tool conversation.

**Steps:**
- [ ] Step 0: No domain model / no production code
- [x] Step 1: `e2e/research/` project skeleton (assemble produces one skeleton)
- [ ] Step 2: research assembly + eval suite (declares both tools; three tool-call assertions)
- [x] Step 3: research noop fixture `fixtures/web-research.yaml` (pre-filled multi-turn tool conversation)
- [ ] Step 4: `tests/e2e_research.rs` (Rust e2e test mirroring `e2e_patterns_eval.rs`)
- [ ] Step 5: `e2e/research/ci.sh` (four-CUJ gate; structural-noop half always green; live half gated) — **passes the feature test**
- [x] Step 6: insurance-claim structural gate (per-case fixtures, extend `ci.sh`, doc-sync `README.md`)

## Step 0: Domain model

None. This feature adds no `src/` production code (design §"Build steps": "No production `src/` code changes — this feature is e2e projects, fixtures, a `ci.sh` pair, one Rust test, and a doc edit"). The Step-0 contract (`ToolDefinition`, `Meta.tools`, `ToolExecutor`, `NoopToolExecutor`, the `Conversation::run` tool loop) is already delivered and green from Feature 1, and `web_search` / `web_fetch` plus their `e2e/research/context/tools/*.json` are delivered by Feature 2 (both verified on disk).

**Why no run/executor wiring is needed (the load-bearing decision, design §"noop tool-result scripting mechanism"):** `cli/run.rs:97` hard-codes the empty `NoopToolExecutor::default()`, and `open_engine_for_model("noop")` yields `NoopEngine::auto()`, which emits only `Content::Text("noop-N")` (`engine.rs:255`) and so never produces a `ToolUse` block. `Conversation::run` (`conversation.rs:491-538`) walks blank assistant slots and only enters the tool branch when a just-filled slot carries `ToolUse` blocks (`:504-511`). A pre-filled fixture with **zero blank assistant slots** makes `run` a verified byte-identical no-op (`tests/run.rs::no_blank_assistant_is_a_byte_identical_no_op`); the loop body never runs and the empty executor is never called. `eval` then scores the authored `tool_use` blocks via `extract_tool_uses` (`assertions.rs:993`) exactly as if a model had emitted them. The fixture *is* the script. No new CLI surface, no executor registry — alternative B (scripted CLI executor source) is rejected (design §Alternatives) as the deferred work in project design §6.

## Step 1: `e2e/research/` project skeleton

**Enables:** the assemble half of the feature test — `ailly -p . assemble research` produces exactly one conversation skeleton under `runs/<id>/` (CUJ 1).

Create the non-tool, non-eval files of the `e2e/research/` project, mirroring `e2e/insurance-claim/`:

- `e2e/research/AGENTS.md` — short constitution naming the research task and the two tools (mirror `e2e/insurance-claim/AGENTS.md`'s shape and tone).
- `e2e/research/.gitignore` — `runs/` and `evals/reports/` (verbatim from `e2e/insurance-claim/.gitignore`).
- `e2e/research/context/system/00-research-policy.md` — one system fragment: "You are a research assistant. Search before you fetch; cite what you fetch."
- `e2e/research/prompts/web-research.md` — one research question needing a search then a fetch: "Find the official Rust homepage and fetch its tagline."

`e2e/research/context/tools/web_search.json` + `web_fetch.json` are already on disk (Feature 2); this step references them, never re-creates them (project plan §"Single owner of the web tool JSONs").

**Runnable state:** no Rust code changes, so `mise run check`/`test` stay green. The skeleton is incomplete (no assembly yet) so `assemble research` is not yet invokable; the count assertion lands in Step 2 once the assembly exists. Step 1 only stages the prefix inputs the assembly will reference.

## Step 2: research assembly + eval suite

**Enables:** CUJ 1's count assertion (`assemble research` produces one skeleton) and the three tool-call assertions the eval report must show passing (CUJ 2 / the feature test's eval half).

- Create `e2e/research/assemblies/research.yaml` (mirror `claim-handler.yaml`'s prefix shape; one matrix case `web-research` so `assemble` produces one skeleton):
  ```yaml
  name: research
  model: claude-sonnet-4-6
  matrix:
    case: [web-research]
  prefix:
    - { kind: file,   path: ./AGENTS.md,             cache: true }
    - { kind: system, path: context/system/*.md,     cache: true }
    - { kind: tools,  path: context/tools/*.json,     cache: true }
  conversation:
    - { role: user, path: "prompts/{{ case }}.md" }
    - { role: assistant }
  ```
- Create `e2e/research/evals/research.yaml` (internally-tagged `Assertion` schema; copy the shape from `e2e/insurance-claim/evals/regression.yaml`; case `name` matches the skeleton/fixture stem `web-research`):
  ```yaml
  name: research
  cases:
    - name: web-research
      assertions:
        - { type: must_call_tool, tool: web_search }
        - { type: must_call_tool, tool: web_fetch }
        - { type: tool_call_order, sequence: [web_search, web_fetch] }
  ```

**Runnable state:** still no Rust code changes; `check`/`test` green. After this step, `ailly -p e2e/research assemble research` produces exactly one skeleton (manually verifiable; the script's assertion is wired in Step 5). The eval suite parses and loads but has no fixture to score yet — Step 3 supplies it.

## Step 3: research noop fixture `fixtures/web-research.yaml`

**Enables:** the eval half of the feature test — a pre-filled multi-turn tool conversation that `eval` scores, making `must_call_tool: web_search`, `must_call_tool: web_fetch`, and `tool_call_order: [web_search, web_fetch]` all pass with no live API.

Create `e2e/research/fixtures/web-research.yaml`: `model: noop`, no blank assistant slot, carrying the full shape `user -> assistant(tool_use web_search) -> tool(tool_result) -> assistant(tool_use web_fetch) -> tool(tool_result) -> assistant(text)` (one `tool_use` per assistant turn so `tool_call_order` reads `[web_search, web_fetch]` in message-then-block order, matching how `extract_tool_uses` walks). Use the exact block schema the live code reads — `type: tool_use { id, name, input }` and `type: tool_result { tool_use_id, content }` — verified against `CONV_OVER_LIMIT` in `tests/eval_insurance_claim.rs`. The fixture body is the one given in design §`fixtures/web-research.yaml`.

**Red-first verification for this step:** add a small assertion that fails before the fixture exists and passes after. The cleanest red-first locus is the Step-4 Rust test's fixture-shape assertion (the synthetic body mirrors this fixture); if the fixture is authored before that test, instead prove it red-green by running `cargo run --quiet -- -p e2e/research eval research --over <fixture-copied-into-a-run-dir>` and confirming the produced `evals/reports/<id>.json` shows `assertions.passed == 3, failed == 0` (it would show 0 matched / unscorable before the fixture exists). This is the same scoring path Step 5's `ci.sh` gate automates.

**Runnable state:** no Rust code changes; `check`/`test` green. The fixture is inert until copied into a run dir and scored (Step 5).

## Step 4: `tests/e2e_research.rs`

**Enables:** the Rust e2e test that scores the research suite over a synthetic tool-call conversation, asserting the three tool-call assertions are read and the report JSON is written — mirroring `tests/e2e_patterns_eval.rs`.

Create `tests/e2e_research.rs`: an inline synthetic conversation YAML constant (same shape as `fixtures/web-research.yaml`, with the two `tool_use` blocks and a final text turn), written to a tmp run dir, scored by `eval_run(EvalCmdArgs { project: <CARGO_MANIFEST_DIR>/e2e/research, suite: "research", over: <tmp run dir> })`. Assert the report JSON is written at `evals/reports/<run-id>.json` and matches the documented totals contract (`report["suite"]`, `report["run_id"]`, `report["totals"]["assertions"][...]`), with cleanup so the project tree stays pristine.

**Known harness characteristic (shared, NOT introduced here):** because `over` is a tmp dir outside the project's `host_root`, `cli/mod.rs::project_relative` returns an empty `RunId` (`strip_prefix` fails -> `unwrap_or_default()`), so `conversations_repository.list("")` does not find the tmp conversations and `conversations_matched == 0`. This is the identical limitation that makes `eval_insurance_claim.rs` (`:147`), `e2e_delegate_52.rs` (`:262`), and `e2e_patterns_eval.rs` (`:284`) all assert `left: 0` at baseline. Write `tests/e2e_research.rs` to the **same convention** as those three, with a module-doc comment pointing at the shared root cause (design §"Known harness characteristic"). The authoritative tool-call demonstration is the Step-5 `ci.sh` gate, which runs `eval` over a run dir **inside** the project tree where `project_relative` resolves correctly; the Rust test mirrors the existing convention and is not the feature gate. **Resolving the `over`-path limitation is out of scope for Feature 3** (verdict below).

**Red-first:** the test fails before `e2e/research/evals/research.yaml` exists (suite load fails) and before the synthetic body's tool blocks are scorable; it reaches its asserted state once Steps 2–3 are in place. Run `cargo nextest run --test e2e_research` to confirm red then green.

**Runnable state:** new test file; `mise run test` runs it. `check`/`lint`/`format` green.

## Step 5: `e2e/research/ci.sh` — the feature test

**Enables:** the Feature 3 feature test — `e2e/research/ci.sh` exits 0.

Create `e2e/research/ci.sh`, modeled on `e2e/insurance-claim/ci.sh` (the in-repo `cargo run --quiet -- -p "${project_dir}" <cmd>` form invoked from `repo_root`, self-locating via `BASH_SOURCE`, `set -euo pipefail`, bash-3.2 compatible). Four CUJs:

1. **assemble** (always): `ailly -p . assemble research`; assert exactly one conversation skeleton lands under `runs/<id>/` (pure file read, no API), mirroring insurance-claim CUJ 1's nullglob count check.
2. **structural tool-call gate** (always, noop): copy `fixtures/web-research.yaml` into a fresh run dir `runs/<id>-structural/web-research.yaml`; run `ailly -p . run runs/<id>-structural/` as a **verified no-op** (fixture has no blank assistant, so `run` is byte-identical — assert the file is unchanged, e.g. compare a checksum before/after); then `ailly -p . eval research --over runs/<id>-structural/` and read `evals/reports/<structural-id>.json` with `python3` (the DDD `ci.sh` heredoc pattern) asserting `totals.assertions.passed >= 3` and `totals.assertions.failed == 0`. This proves `must_call_tool` + `tool_call_order` fire on the multi-turn shape with no live API.
3. **live run** (gated on `ANTHROPIC_API_KEY` or a project `.env`): `ailly -p . run runs/<id>/` over the assembled skeleton, then `eval` + `report`; skipped with a clear notice otherwise, exactly like insurance-claim CUJ 2.
4. **report** (always, over the structural run): `ailly -p . report <structural-id>`; assert `evals/reports/<structural-id>-report.md` wrote (single-mode `report` writes `<run-id>-report.md`, verified `report.rs:78`).

**Verification (execution required, design §Metrics):** run `bash e2e/research/ci.sh` with `ANTHROPIC_API_KEY` unset and confirm it exits 0 with the assemble + structural + report halves all printing OK and the live half printing SKIP. This is the feature test and must be executed, not assumed.

**Runnable state:** new script + the fixture/assembly/eval it drives; `check`/`test` green; the script exits 0.

## Step 6: insurance-claim structural gate + doc-sync

**Enables:** the insurance-claim structural gate exits 0, proving `must_call_tool: lookup_policy`, `tool_call_order: [lookup_policy, lookup_claim_history]`, and `must_not_call_tool: auto_approve` now pass on multi-turn tool conversations (the capability the README's "Current limitations" said was impossible), with no live API.

- **Promote per-case fixtures:** create `e2e/insurance-claim/fixtures/missing-fields.yaml` and `e2e/insurance-claim/fixtures/over-limit.yaml`, lifted from the inline `CONV_MISSING_FIELDS` / `CONV_OVER_LIMIT` constants already proven in `tests/eval_insurance_claim.rs` (both already carry the exact `tool_use` shapes the suite asserts: `missing-fields` emits `lookup_policy`; `over-limit` emits `lookup_policy` then `lookup_claim_history` in order). The `ambiguous` case calls no tool and is covered by the existing eval; no fixture needed. **No fixture emits `auto_approve`** — emitting it would fail the two `must_not_call_tool: auto_approve` assertions and invert the gate (verified `regression.yaml:12/18`).
- **Extend `e2e/insurance-claim/ci.sh`:** after the existing CUJ 1 assemble, insert a noop structural step that copies the two per-case fixtures into a fresh run dir, runs `ailly run` as a verified no-op (byte-identical), runs `ailly eval regression --over <fixture-run-dir>`, and asserts via the report JSON (`python3`) that the three tool-call assertions pass (the `over-limit` `judge` assertion defers as today — no engine wired). The existing live half (CUJ 2, gated on `ANTHROPIC_API_KEY`) is preserved unchanged.
- **Doc-sync `e2e/insurance-claim/README.md` "Current limitations":** rewrite the first bullet (`README.md:188-196`, currently "Tool definitions are rendered into a system message ... the rig adapter sends `tools: Vec::new()` unconditionally ... `must_call_tool` and `tool_call_order` assertions fail") to describe the now-live behavior: `kind: tools` resolves to `meta.tools`, the rig adapter forwards them, and the structural gate proves `must_call_tool` / `tool_call_order` pass on multi-turn tool conversations. The `judge`-deferred bullet stays (still true). Ships with the gate per the documentation-sync rule, not as a follow-up.

**Red-first:** before the gate exists, `e2e/insurance-claim/ci.sh` has no structural step; add the python3-asserted structural block and confirm it would fail if a fixture (incorrectly) emitted `auto_approve` or omitted a required tool, then confirm green with the correct fixtures. Run `bash e2e/insurance-claim/ci.sh` (no key) and confirm exit 0 with the assemble + structural halves OK and the live half SKIP.

**Runnable state:** new fixtures + script edit + doc edit; no Rust changes; `check`/`test` green; both `ci.sh` exit 0.

---

## `eval_insurance_claim.rs:147` — out of scope (verdict)

**Feature 3 does NOT fix `:147`, and that is correct** (design §"`:147` verdict"). `:147` is `assert_eq!(outcome.conversations_matched, 3)`, panicking `left: 0, right: 3`. Root cause: the test writes synthetic conversations to a tmp dir **outside** `e2e/insurance-claim`; `eval_run` resolves the listing key via `project_relative(&project, &args.over)` (`cli/eval.rs` `list_key`); `project_relative` (`cli/mod.rs:15-30`) canonicalizes `over`, `strip_prefix`es the project host root, and on failure (tmp dir outside the root) returns an **empty** `RunId` via `unwrap_or_default()`; `conversations_repository.list("")` then lists relative to the project root, finds none of the tmp files, and reports `conversations_matched == 0`. This is the `over`-outside-project path bug, shared identically by `e2e_delegate_52.rs:262` and `e2e_patterns_eval.rs:284`, and has **nothing to do with tool-call wiring**. Feature 3 touches none of `tests/eval_insurance_claim.rs`, `cli/eval.rs`, or `cli/mod.rs::project_relative`; the plan/research assign no `over`-path fix to this project. The Step-5/6 `ci.sh` gates run `eval` over a run dir **inside** the project tree where `project_relative` resolves correctly, so they prove the tool-call assertions without depending on the broken `over`-path. `:147` and its two siblings stay tolerated baseline failures; their one-line locus (`project_relative` returning empty on an out-of-root `over`) is documented for whoever picks up the separate bug.

## Self-Review

**Spec coverage** — every design §"Build steps" item maps to a step: research skeleton (Step 1), assembly + eval (Step 2), noop fixture (Step 3), Rust e2e test (Step 4), research `ci.sh` (Step 5), insurance-claim structural gate + README doc-sync (Step 6). The design's six build steps are preserved 1:1 (the design folds the Rust test into its own step here for an explicit red-green locus, staying within the 7-step ceiling at 7 steps including Step 0). The feature test (`e2e/research/ci.sh` exits 0) is Step 5's deliverable and is executed, not assumed. The `:147` verdict (out of scope) and the shared-harness characterization are carried verbatim from the design.

**No production code** — confirmed against the design and live code: `cli/run.rs:97` already supplies `NoopToolExecutor::default()`, `Conversation::run` already owns the tool loop, and `web_search`/`web_fetch` + their JSONs are on disk. The noop-tool-result mechanism is the pre-filled fixture (no executable blanks), so no run/executor wiring is added.

**Type / path consistency** — `must_call_tool`/`tool_call_order`/`must_not_call_tool` (internally-tagged `Assertion`, `evaluation.rs:63/67/75`), the `eval` report path `evals/reports/<report_id>.json` (`cli/eval.rs`), and the `report` single-mode output `<run-id>-report.md` (`report.rs:78`) are used consistently across Steps 2/5/6. The fixture block schema (`tool_use { id, name, input }` / `tool_result { tool_use_id, content }`) matches `CONV_OVER_LIMIT` in `tests/eval_insurance_claim.rs`.

**Step ordering** — each step leaves `check`/`test` green: Steps 1–3 and 5–6 add only non-Rust project files (skeleton, assembly, eval, fixtures, scripts, doc), which cannot break the Rust build or suite; Step 4 adds one Rust test written to the existing tolerated-baseline convention. The feature test passes at Step 5; Step 6 extends the demonstration to insurance-claim and ships the doc-sync.
