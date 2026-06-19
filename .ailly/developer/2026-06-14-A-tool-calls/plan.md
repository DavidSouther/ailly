# Implementation Plan: Tool Calls

*Cleared 2026-06-14 — controlling-agent review passed; load-bearing citations verified against HEAD (12cf2ed).*

**Project design:** [design.md](design.md)  |  **Closing Bell:** [closing-bell.md](closing-bell.md)

**Goal:** Wire end-to-end tool-call support so an assembly can declare tools, `ailly run` drives a multi-turn tool loop, and `ailly eval` scores the resulting tool calls.

**Architecture:** Approach A from design §5. `assemble` resolves `PrefixBlock::Tools` JSON files into structured `Vec<ToolDefinition>` written onto `meta.tools`; `run` reads `meta.tools` off the conversation file and forwards it on every `CompletionRequest` (`model.completion()` stays unary). `Conversation::run` owns the agentic loop: after a blank assistant slot is filled, any `tool_use` blocks are dispatched through an injected `ToolExecutor`, the results are appended as a `Role::Tool` message, a fresh blank assistant slot is appended, and the loop continues. Because the definitions live in the run artifact, a conversation replays without re-opening its assembly.

**Build order (DAG):**

```
Step 0 (shared contract, settled here)
   └─> Feature 1: Harness          (sequential, first — delivers the Step-0 contract)
         └─> Feature 2: Web tools   (depends on Feature 1's ToolExecutor + knowledge/tools/)
         └─> Feature 3: e2e + testing (depends on Feature 1 for the loop and Feature 2 for the two tools)
```

Dependency edges:
- Feature 2 → Feature 1: `web.rs` implements the `ToolExecutor` contract and the `knowledge/tools/` module that Feature 1 creates.
- Feature 3 → Feature 1: the `e2e/research/` and insurance-claim gates exercise the `Conversation::run` tool loop and `meta.tools` resolution.
- Feature 3 → Feature 2: the `e2e/research/` assembly declares `web_search` + `web_fetch`, which only exist after Feature 2.

**Ordering (per design Features table):** Feature 2 is *parallel* and Feature 3 is *sequential, last* — Feature 3 must not start until Feature 2 is complete, because Feature 3 references Feature 2's web tools and their JSON fixtures. **Single owner of the web tool JSONs:** `e2e/research/context/tools/web_search.json` and `web_fetch.json` are created **only** by Feature 2 (Feature 2 Files list); Feature 3 references them and never re-creates them. This removes the duplicate-ownership collision between the two feature cycles.

Each feature is its own design → plan → build → cleanup cycle (design §"Features"). This plan is project-altitude: it fixes the Step-0 contract and the feature boundaries, names the deferred decisions each feature must resolve in its own design, and sketches each feature's ≤7 build steps. It does **not** lock per-step implementation code; that is produced by each feature's own `developer:plan` during build. The cleanup phase (`developer:cleanup` / `developer:refactor`) closes every feature cycle and is not enumerated per-step below; it is the named checkpoint that runs after each feature's build steps are green and before the next feature builds on it. It matters most after **Feature 1**, whose Step-0 contract (the new `knowledge/tools/` module and the changed assembly render path) every later feature consumes, so Feature 1's cleanup pass settles that surface before Features 2 and 3 depend on it.

---

## Step 0: Shared contract (settle before per-feature parallel work)

The harness feature (Feature 1) delivers this contract; every later feature depends on it. These are concrete type signatures, drawn from design §"Step 0" and the research smallest-version table. Signatures only — no bodies. Where a decision is deferred to a feature design, it is named here, not resolved.

**`ToolDefinition`** — a schema value, not behavior. Mirrors the JSON in `e2e/insurance-claim/context/tools/*.json` (`name` / `description` / `input_schema`):

```rust
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_yaml_ng::Value,
}
// derives: Clone, Debug, PartialEq, Eq, Serialize, Deserialize
```

(Location — `src/content/` vs `src/knowledge/` — is open #1, resolved in Feature 1's design. The serialized shape above is fixed regardless of module.)

**`meta.tools`** — new optional field on `Meta` (`src/content/conversation.rs:53`):

```rust
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub tools: Vec<ToolDefinition>,
```

Skip-serialized when empty so existing tool-free conversations remain byte-identical. Both named invariants are verified present: `round_trip_preserves_three_message_fixture` (`conversation.rs:729`) and `meta_defaults_are_skipped_on_emit` (`conversation.rs:737`), driven by the in-file `THREE_MESSAGE_FIXTURE` (`conversation.rs:465`), which omits `tools:` — so a `skip_serializing_if = "Vec::is_empty"` field leaves the fixture's bytes unchanged and both invariants still hold without editing the fixture. This is a DESIGN.md schema change (`meta.tools?: ToolDefinition[]`).

**`CompletionRequest.tools`** — new borrowed field on `CompletionRequest<'a>` (`src/engine/engine.rs:24`), empty by default:

```rust
pub struct CompletionRequest<'a> {
    pub model: ModelId,
    pub messages: &'a [Message],
    pub tools: &'a [ToolDefinition],
    pub debug: bool,
}
```

`rig_engine.rs:62` replaces the literal `tools: Vec::new(),` with the forwarded Ailly tools, lowered into Rig's `tools` shape.

**`ToolExecutor`** — async dispatch trait, one tool call → one tool result. Lives in `src/knowledge/tools/`:

```rust
#[async_trait::async_trait]
pub trait ToolExecutor: Send + Sync {
    async fn execute(&self, call: &ContentBlock /* ToolUse */) -> ContentBlock /* ToolResult */;
}
```

The exact argument/return projection (whether `execute` takes the `ToolUse` block or a destructured `{id, name, input}` tuple, and how it produces the `ToolResult { tool_use_id, content, is_error }`) is a Feature 1 design detail; the contract is "one call in, one result out, async."

**`NoopToolExecutor`** — scripted replies consumed in call order, keyed by tool name (mirrors the `NoopEngine` script-queue pattern at `engine.rs:179`). Used by harness tests and the structural CI gate. Lives in `src/knowledge/tools/`.

**`Conversation::run` tool-turn protocol** — the loop body inserted at `src/content/conversation.rs:444`:

> After a blank assistant slot is filled, if its content carries `tool_use` blocks: execute each via the injected executor, append one `Role::Tool` message whose body is the `tool_result` block(s), append a fresh blank assistant slot, and continue the loop. When the filled slot carries no `tool_use` blocks, the loop terminates as it does today.

Whether `run` takes `executor: Option<&dyn ToolExecutor>` (error if `tool_use` appears with no executor) or always requires one (callers pass an empty `NoopToolExecutor` when no tools are needed) is open #2, resolved in Feature 1's design. The call site `src/cli/run.rs:94` (`conv.run(engine)`) changes accordingly.

---

## Feature 1: Harness  (sequential, first)

**Delivers / scope (design Features table, row 1):** the entire Step-0 contract — `ToolDefinition`, `meta.tools`, `CompletionRequest.tools`, `rig_engine` forwarding, `ToolExecutor` + `NoopToolExecutor`, and the `Conversation::run` tool loop. This is the foundation; a harness with no tool is inert but releasable (tools stay dark until an assembly declares one).

**Files:**
- Create: `src/knowledge/tools/mod.rs` (module root; `ToolExecutor`, `NoopToolExecutor`), registered in `src/knowledge/mod.rs` (Modify).
- Create or Modify: `ToolDefinition` lands in `src/content/conversation.rs` (or a new `src/content/tools.rs`) **or** `src/knowledge/tools/` — open #1. Whichever module owns it, `src/content/mod.rs` / `src/knowledge/mod.rs` is updated to export it.
- Modify: `src/content/conversation.rs` — add `Meta.tools` field; insert the tool loop into `Conversation::run` (`:444`); the loop reads `tool_use` from the just-filled slot, appends a `Role::Tool` message and a fresh blank assistant.
- Modify: `src/engine/engine.rs` — add `CompletionRequest.tools` (`:24`); update the `request()` test helper at `:283`.
- Modify: `src/engine/rig_engine.rs` — replace `tools: Vec::new(),` (`:62`) with the lowered, forwarded Ailly tools.
- Modify: `src/content/assembly.rs` — `PrefixBlock::Tools` stops resolving to concatenated system text (`resolve_prefix_block` at `:243`, the `System | Tools | Examples` arm at `:254`); it now parses each JSON file into a `ToolDefinition` and the render path writes them onto `meta.tools` instead of a `Role::System` text message. `Assembly::render` (`:212`) changes to carry the resolved tools through to `Meta`.
- Modify: `src/cli/run.rs` — pass `conv.meta.tools` on the request and supply an executor to `conv.run` per the open-#2 resolution.
- Modify: `DESIGN.md` — see "DESIGN.md change" below (rides with Feature 1).

**Feature-test intent (one sentence, written by Feature 1's `developer:feature-test`):** a noop-scripted multi-turn `run` — where the engine's first scripted reply is an assistant `tool_use` block and the executor returns a scripted `tool_result` — produces the session shape `user → assistant(tool_use) → tool(tool_result) → assistant(text)`, asserting each turn's role and block kind on the resulting conversation. (Do not write the test code here.)

**Deferred design decisions to resolve in THIS feature's design:**
- **Open #1** — does `ToolDefinition` live in `src/content/` (it is part of the assembly/conversation schema, referenced by `PrefixBlock::Tools` and serialized into `meta.tools`) or `src/knowledge/` (it is consumed by the run loop at execution time)? State and resolve in Feature 1's design.
- **Open #2** — does `Conversation::run` take `executor: Option<&dyn ToolExecutor>` (returning an error if a `tool_use` appears with no executor) or always require an executor (callers pass an empty `NoopToolExecutor` when no tools are needed)? State and resolve in Feature 1's design.

**Sketch of its ≤7 build steps** (Feature 1's own `developer:plan` expands these; each leaves `check` + tests green):
1. **`ToolDefinition` value type** — introduce the struct with its serde shape; round-trips a `*.json` tool fixture. (Domain step; lands `ToolDefinition` in the module chosen by Feature 1's design per open #1 — the decision belongs to the design phase, the step only implements it.)
2. **`Meta.tools` field** — add the skip-when-empty field; the existing byte-identical round-trip and default-skip invariants still pass.
3. **`CompletionRequest.tools` + rig forwarding** — add the borrowed field; replace `rig_engine.rs:62` to lower and forward; existing rig translation tests stay green.
4. **`ToolExecutor` + `NoopToolExecutor`** — create `knowledge/tools/`; the noop executor serves scripted `tool_result`s in call order.
5. **Assembly resolves `Tools` to structured defs** — `PrefixBlock::Tools` parses JSON into `ToolDefinition`s onto `meta.tools` instead of a system-text message; assembly round-trip tests updated for the new render output.
6. **`Conversation::run` tool loop** — insert the tool-turn protocol; wire the executor per open #2; the feature test's session-shape assertion passes.
7. **CLI `run` wiring** — `src/cli/run.rs` forwards `meta.tools` and supplies the executor; `ailly run` against a noop conversation drives the loop end to end.

**At-ceiling flag (7-step methodology limit):** Feature 1 sits exactly on the 7-step maximum with zero slack — it is the only feature delivering 6+ distinct contract artifacts, and two of these steps already bundle pairs (step 3: borrowed field + rig lowering; step 4: `ToolExecutor` trait + `NoopToolExecutor`). If Feature 1's own `developer:plan` finds any of these warrants its own step (e.g. the rig lowering, or non-trivial scripted-replies-in-call-order machinery in `NoopToolExecutor`), the build tips over 7 and **must return to design to simplify** per `developer:plan`. The pre-identified simplification is a feature-level split along the existing `content`/`knowledge` seam: a **schema half** (`ToolDefinition` + `Meta.tools` + the assembly `Tools`→structured-defs resolver) and an **execution half** (`ToolExecutor` + `NoopToolExecutor` + the `run` loop + rig forwarding + CLI wiring). Feature 1's design owns the decision to keep it whole or split; this plan flags the risk here so it surfaces at design time, not at build time.

**Closing Bell roll-up:** unblocks the mechanism behind bell tasks 1 (a declared tool reaches `meta.tools`), 2 (the `tool_use → tool_result → text` shape), and the harness half of 3 (`tool_use` blocks exist for assertions to read). None of these bell tasks *pass* on Feature 1 alone — they need a tool (Feature 2) and the e2e wiring (Feature 3) to be exercised as the participant would.

---

## Feature 2: Web tools  (depends on Feature 1)

**Delivers / scope (design Features table, row 2):** `web_search` and `web_fetch` in `src/knowledge/tools/web.rs`, each implementing the Feature 1 `ToolExecutor` contract, each with its own unit test exercising the tool logic directly. Harness tests use `NoopToolExecutor`; these tool tests exercise the real tool logic, separate from the harness (research.md "Do individual tools get individual tests?" → Yes).

**Files:**
- Create: `src/knowledge/tools/web.rs` (`web_search`, `web_fetch` implementations + their unit tests), registered in `src/knowledge/tools/mod.rs` (Modify).
- Create: `e2e/research/context/tools/web_search.json` and `web_fetch.json` (the `ToolDefinition` JSON the e2e assembly declares; mirror the `e2e/insurance-claim/context/tools/*.json` shape). These tool-definition fixtures are authored here so Feature 3's assembly has them to declare; the e2e *project wiring* is Feature 3.

**Search-provider TBD (research.md §"Search / Expand", "WebSearch ... call a search engine provider (tbd)"):** the concrete search backend for `web_search` is unresolved in research. Feature 2's design must pick a provider (or a vendor-neutral seam over one) before the `web_search` unit test is written; the unit test mocks the provider per the project's external-dependency mocking convention, so the harness and e2e gates never make a live web call. **No internal prior-art seam to copy:** unlike `NoopToolExecutor` (mirrors `NoopEngine`) or the web tool JSONs (mirror the insurance-claim JSONs), `src/` has no existing HTTP/search-client module (verified — no `web*.rs`/`http*.rs` under `src/`), so Feature 2's design builds the provider seam from zero rather than mirroring an existing one. Do not expect a pattern to copy here.

**Feature-test intent:** Feature 2 has no single end-to-end feature test of its own; per the design, each tool's correctness is its own unit test. `web_search`'s unit test asserts it issues a query and shapes results into a `tool_result`; `web_fetch`'s unit test asserts it fetches a URL and shapes the body into a `tool_result`. Both mock the network.

**Deferred design decisions to resolve in THIS feature's design:** the `web_search` provider choice and its mock seam (above). No project-open decisions (#1/#2/#3) belong to Feature 2.

**Sketch of its ≤7 build steps** (each leaves `check` + tests green):
1. **`web_search` provider seam** — Feature-2-design's chosen provider behind a mockable interface; a unit test drives it with a mocked response.
2. **`web_search` `ToolExecutor` impl** — implements `execute` to run a search and return a `tool_result`.
3. **`web_search.json` tool definition** — the declared schema fixture under `e2e/research/context/tools/`.
4. **`web_fetch` `ToolExecutor` impl** — fetches a URL (mocked in test) and returns the body as a `tool_result`.
5. **`web_fetch.json` tool definition** — the declared schema fixture.

(Three to five steps; if Feature 2's design finds it needs more than 7, it returns to design to simplify per `developer:plan`.)

**Closing Bell roll-up:** provides the concrete tools that bell tasks 1, 3, and 4 reference by name (`web_search`, `web_fetch`). Still no bell task passes here — they are exercised through the e2e fixtures in Feature 3.

---

## Feature 3: e2e + testing  (depends on Feature 1 and Feature 2)

**Delivers / scope (design Features table, row 3):** the `e2e/research/` project (assembly declaring `web_search` + `web_fetch`, a research prompt, evals asserting the tool calls + order, a noop-run `ci.sh`) and the `e2e/insurance-claim` multi-turn structural gate. Both run noop — no live web or model API in CI (research.md "noop run — no live API"; live run gated on `ANTHROPIC_API_KEY`).

**Files:**
- Create: `e2e/research/` following the `~/devel/davidsouther/domain-driven-design/research/e2e/` layout — `assemblies/`, `prompts/`, `context/`, `evals/`, `ci.sh`, `README.md`, `AGENTS.md`, `.gitignore`. The `context/tools/web_search.json` + `web_fetch.json` fixtures are **owned and authored by Feature 2** (Feature 2 Files list); Feature 3 only references them — it does not re-create them. The assembly declares a `{ kind: tools, path: context/tools/*.json }` prefix block and a research prompt that needs both tools; the eval suite asserts the tool calls and their order. Written in the internally-tagged `Assertion` schema (verified in `src/content/evaluation.rs`: `MustCallTool { tool, with_args }` at `:63`, `ToolCallOrder { sequence }` at `:75`), the assertions are `{ type: must_call_tool, tool: web_search }`, `{ type: must_call_tool, tool: web_fetch }`, and `{ type: tool_call_order, sequence: [web_search, web_fetch] }` — the `type:` key is mandatory and `tool_call_order` keys its list under `sequence:`, not a bare array. The live `e2e/insurance-claim/evals/regression.yaml` is the working example to copy.
- Modify: `e2e/insurance-claim/ci.sh` and `e2e/insurance-claim/README.md` — add the structural multi-turn gate. The insurance assembly already declares `{ kind: tools, path: context/tools/*.json }`. The existing `evals/regression.yaml` spans three cases with distinct tool-call expectations: `missing-fields` asserts `must_call_tool: lookup_policy`; `ambiguous` asserts `must_not_call_tool: auto_approve`; `over-limit` asserts `must_not_call_tool: auto_approve` **and** `tool_call_order: [lookup_policy, lookup_claim_history]`. The gate therefore runs `assemble → run (noop) → eval → report` with per-case scripted noop replies — the positive cases emit `lookup_policy` (and, for `over-limit`, `lookup_claim_history` after it, in order), and **no case emits `auto_approve`** (emitting it would fail the two `must_not_call_tool` assertions and invert the gate's intent). The gate asserts the tool-call assertions fire on the multi-turn conversations. The existing live half (gated on `ANTHROPIC_API_KEY`) is preserved. The README's "Current limitations" section (`README.md:188-196`) currently states the rig adapter sends `tools: Vec::new()` unconditionally and consequently `must_call_tool`/`tool_call_order` assertions fail; Feature 1 removes that limitation and this gate makes those assertions pass, so the section is updated to reflect the now-live behavior.
- Create (shape per open #3): a noop conversation/script fixture for the insurance-claim structural gate. The fixture is per-case aware — the regression suite's three cases (`missing-fields`, `ambiguous`, `over-limit`) carry differing tool-call expectations, so the scripted replies differ per case rather than being one shared scripted conversation.

**Feature-test intent:** the executable feature test for Feature 3 is that `e2e/research/ci.sh` exits green — it drives `assemble → run (noop) → eval → report` and asserts the report shows `must_call_tool` and `tool_call_order` passing on the noop-scripted conversation, with no live API call and no manual fix-ups.

**Deferred design decisions to resolve in THIS feature's design:**
- **Open #3** — for `e2e/insurance-claim`, is the noop fixture a standalone YAML conversation file (e.g. under `e2e/insurance-claim/runs/fixture/`) or generated by `ci.sh` from the assembly with scripted replies injected? State and resolve in Feature 3's design. (The `e2e/research/` gate inherits whichever shape Feature 3 chooses, for consistency.)

**Sketch of its ≤7 build steps** (each leaves `check` + tests green; e2e scripts exit 0):
1. **`e2e/research/` skeleton** — `assemblies/`, `prompts/`, `context/tools/` (referencing the two web tool JSONs authored in Feature 2 — not re-created here), `evals/`, `README.md`, `AGENTS.md`, `.gitignore`, following the DDD research-e2e layout.
2. **research assembly + prompt** — assembly declares both web tools and a research prompt requiring both; `assemble` produces the expected conversation count.
3. **research eval suite** — `{ type: must_call_tool, tool: web_search }`, `{ type: must_call_tool, tool: web_fetch }`, `{ type: tool_call_order, sequence: [web_search, web_fetch] }` (internally-tagged `Assertion` schema; copy the shape from `e2e/insurance-claim/evals/regression.yaml`).
4. **research `ci.sh`** — noop-scripted `assemble → run → eval → report`; resolves open #3's fixture shape; exits green.
5. **insurance-claim structural gate** — extend `e2e/insurance-claim/ci.sh` with the multi-turn noop gate over the already-present tools and tool-call assertions; live half preserved.

**Closing Bell roll-up:** this is where the Critical bell tasks become *achievable* by the participant. Task 4 (the research journey from a clean clone) is the automatable portion — `e2e/research/ci.sh` exits green. Tasks 1, 2, 3 are exercised through the shipped `e2e/research/` and insurance-claim fixtures plus the amended `DESIGN.md` and `e2e/research/README.md` (the documentation the participant authors from). Task 5 (secondary, live insurance-claim run) is enabled but stays gated on `ANTHROPIC_API_KEY` and does not block the bell.

---

## DESIGN.md change

The `conversation` `meta` schema (`DESIGN.md:12`) gains `tools?: ToolDefinition[]`, with a `ToolDefinition` shape (`name` / `description` / `input_schema`) documented. The `assembly` prose that says "the engine treats every block as ordered text concatenated into the window" (`DESIGN.md:64`) is amended: `kind: tools` resolves to structured `ToolDefinition`s carried on `meta.tools`, not text — it is the one prefix-block kind that stops being concatenated system text. The agentic-loop shape (assistant `tool_use` → `Role::Tool` `tool_result` → next blank assistant turn) is documented alongside the existing "blank assistant slot" description (`DESIGN.md:37`).

**This edit ships as part of Feature 1.** The schema change (`meta.tools`) and the `kind: tools` resolution change are introduced by Feature 1's code (the field, the assembly resolver, the loop), so the doc must move in lockstep with that code per the documentation-sync rule. Feature 3 adds only `e2e/research/README.md` (its own deliverable), not further `DESIGN.md` edits.

---

## Closing Bell coverage matrix

| Bell task | Tier | Feature that enables it | Automatable now | Human-study-only |
|---|---|---|---|---|
| 1. Declare a tool (assembly `tools` block → `meta.tools` lists it) | Critical | Feature 1 (mechanism) + Feature 3 (fixture to copy from) | `assemble` output carries `meta.tools` (covered by Feature 1 feature test + e2e assemble step) | Participant authoring the `tools` block unaided from docs; ≤10 min; ease ≥4/5 |
| 2. Run a tool conversation (`tool_use → tool_result → text` shape, no money) | Critical | Feature 1 (loop) + Feature 3 (noop fixture) | Noop `run` produces the shape (Feature 1 feature test + e2e run step) | Participant pointing to each turn and naming it; ≤10 min |
| 3. Assert on tool calls (`must_call_tool` + `tool_call_order`, read report) | Critical | Feature 1 (produces the `tool_use` blocks — the only new piece) + Feature 2 (the tools) + Feature 3 (eval suite + fixture) | `eval`/`report` shows the tool-call assertion class passing (e2e eval/report steps). The read path already exists — `extract_tool_uses` (`assertions.rs:993`), `check_must_call_tool` (`:1070`), `check_tool_call_order` (`:1302`), and the `report` CLI command (`src/cli/report.rs`) already consume `ToolUse` blocks; Feature 1 only supplies the upstream producer, so the implementer feeds existing machinery rather than rebuilding it | Participant authoring the assertions and correctly reading which passed; ≤15 min |
| 4. The research journey works (CI-style, exits green) | Critical | Feature 3 (`e2e/research/ci.sh`) | **Fully automatable** — `e2e/research/ci.sh` exits 0, single command, no fix-ups | Participant running it from a clean clone (the act of running; the green exit is automated) |
| 5. A live multi-turn run (real key, insurance-claim) | Secondary | Feature 1 (loop) + Feature 3 (gate) | Gate runs only when `ANTHROPIC_API_KEY` present; CI half stays noop | Live model fills tool turns; informational, does not block the bell |

---

## Self-Review

**Spec coverage** — every row of the design's Features table maps to a feature section (Harness → F1, Web tools → F2, e2e + testing → F3). Step-0 contract items (`ToolDefinition`, `meta.tools`, `CompletionRequest.tools`, `rig_engine` forwarding, `ToolExecutor` + `NoopToolExecutor`, the `run` loop) each appear in §"Step 0" with a signature and are delivered by Feature 1. The DESIGN.md change (design §"DESIGN.md change") has its own section and a ship-with-Feature-1 assignment. All three deferred decisions (open #1/#2/#3, research.md "Open for design") are named in their owning feature without being resolved here. Every Critical bell task (1–4) and the secondary task (5) appears in the coverage matrix with its enabling feature and its automatable-vs-human split. The Release-Flag decision (design §4 — no flag) is reflected in the architecture and the "releasable at every step" framing.

**Placeholder scan** — no "TBD/TODO/implement later" left as a plan gap. The one genuinely open external choice (the `web_search` provider) is research.md's "(tbd)"; it is surfaced as a Feature-2-design decision, not papered over — that is a faithful relay of the source, not a plan placeholder. Deferred decisions are deliberately *not* resolved here because resolving them would pre-empt each feature's own design phase, which the design mandates.

**Type consistency** — `ToolDefinition { name, description, input_schema }`, `Meta.tools: Vec<ToolDefinition>`, `CompletionRequest.tools: &[ToolDefinition]`, `ToolExecutor::execute`, and `NoopToolExecutor` are used with identical names across Step 0, all three feature sections, the DESIGN.md section, and the matrix. Anchored code locations (`conversation.rs:444`, `engine.rs:24`, `rig_engine.rs:62`, `assembly.rs:243/254`, `cli/run.rs:94`) match the verified ground truth.

**Assumption surfaced** — the design under-specifies how the CLI `run` path obtains a `ToolExecutor` for a noop conversation. Today `open_engine_for_model("noop")` yields `NoopEngine::auto()`, but there is no analogous executor factory, and the e2e gates run noop. This plan assumes the executor injection at `cli/run.rs` (and how the e2e noop gate scripts tool results) is settled by Feature 1's open-#2 resolution together with Feature 3's open-#3 fixture-shape decision; it is flagged so the owning feature designs address it rather than discovering it at build time.

---

## Controlling-agent review (2026-06-14)

A second review pass on top of the built-in 5-lens panel. Scope: trace every load-bearing citation to source and confirm build-readiness.

**Citations verified against `HEAD` (12cf2ed).** All confirmed accurate, none invented:
- `Assertion` is internally-tagged (`#[serde(tag = "type")]`, evaluation.rs:37); `MustCallTool{tool,with_args}` (:63), `MustNotCallTool{tool}` (:67), `ToolCallOrder{sequence}` (:75) — the Feature 3 eval YAML shapes are correct.
- `regression.yaml` per-case assertions match the plan: `missing-fields`→`must_call_tool: lookup_policy` (:5); `ambiguous`/`over-limit`→`must_not_call_tool: auto_approve` (:12/:18); `over-limit`→`tool_call_order: [lookup_policy, lookup_claim_history]` (:19). No case emits `auto_approve` — the gate's intent is preserved.
- **Read-path already exists** (the load-bearing de-risk): `extract_tool_uses` (assertions.rs:993), `check_must_call_tool` (:1070), `check_must_not_call_tool` (:1104), `check_tool_call_order` (:1302). Feature 1 supplies only the upstream producer.
- `Meta` (conversation.rs:53); skip-when-empty invariants `round_trip_preserves_three_message_fixture` (:729) and `meta_defaults_are_skipped_on_emit` (:737) over `THREE_MESSAGE_FIXTURE` (:465).
- DESIGN.md anchors :12 (`meta:`), :37 (blank-slot prose), :64 (the quoted "ordered text concatenated into the window"); `tools` is already a documented `kind` (:51).
- `conv.run(engine)` (cli/run.rs:94); `request()` helper (engine.rs:283); `Assembly::render` (assembly.rs:212); `resolve_prefix_block` (:243).
- `ToolDefinition` shape (`name`/`description`/`input_schema`) mirrors `e2e/insurance-claim/context/tools/lookup_policy.json`; `serde_yaml_ng::Value` is the crate the codebase already uses for schema values.

**Completeness:** every design §4 item, all three features, the DESIGN.md change, and the closing-bell matrix have a home. The three deferred decisions (#1/#2/#3) are named, not resolved. The one genuine gap — no noop `ToolExecutor` factory analogous to `NoopEngine::auto()` — is surfaced as an assumption for Feature 1 (open #2) and Feature 3 (open #3) to settle, not left silent.

**No UI surface:** Ailly is a CLI/library; the e2e gates are `ci.sh` process runs, so no browser E2E applies. Execution verification is `mise run check/test/lint` green plus `ci.sh` exit 0.

**Gate cleared.** The `*Draft*` marker is removed; the plan is build-ready. Build agents may trust the anchors above without re-verifying.
