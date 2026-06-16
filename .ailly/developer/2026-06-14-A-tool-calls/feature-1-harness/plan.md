# Implementation Plan: Feature 1 — Tool-Call Harness

*Cleared 2026-06-14 — design/feature-test/plan gates auto-cleared per controlling-agent authorization; anchors re-verified against worktree HEAD (f9968e8).*

**Feature design:** [design.md](design.md) | **Project plan:** [../plan.md](../plan.md)
**Feature test:** `tests/tool_loop.rs::run_drives_tool_use_then_tool_result_then_text`
**User story:** Driving `Conversation::run` with a noop-scripted engine (first reply an assistant `tool_use` block, second reply assistant text) and a noop-scripted executor produces the agentic session shape `user → assistant(tool_use) → tool(tool_result) → assistant(text)`.

**Steps:**
- [ ] Step 1: `ToolDefinition` value type
- [ ] Step 2: `Meta.tools` skip-when-empty field
- [ ] Step 3: `CompletionRequest.tools` borrowed field + rig forwarding
- [ ] Step 4: `NoopToolExecutor::execute` body (the executor's only `todo!()`)
- [ ] Step 5: Assembly resolves `kind: tools` to structured defs on `meta.tools`
- [ ] Step 6: `Conversation::run` tool loop (flips the feature test green)

---

## State of the world at plan time (verified, not assumed)

The feature-test commit (`f9968e8`) already landed more than bare type stubs. The plan is sized against the **actual** stub surface, not the design's "all-stubs" framing:

- **Already present (do not re-create):** the `ToolExecutor` trait, `ToolError` (`NoopExhausted` / `NotAToolUse`), and `NoopToolExecutor` with real `new` / `from_scripts` / `Default` bodies — all in `src/knowledge/tools/mod.rs`; `RunError::Tool(#[from] …ToolError)` (`conversation.rs:345`); `Conversation::run`'s new two-arg signature `(engine, executor)` with a type-first body (`conversation.rs:454`, `let _ = executor;`); the CLI executor wiring (`cli/run.rs:97-98` already builds `NoopToolExecutor::default()` and calls `conv.run(engine, &executor)`); the `RunError::Tool → RunCmdError::Tool` map (`cli/run.rs:61`); and the `engine_provider.rs` integration test already updated to the two-arg `run`.
- **The only executable `todo!()` in `src/`** is `NoopToolExecutor::execute` (`tools/mod.rs:96`) — Step 4.
- **Does NOT exist yet:** `ToolDefinition` (referenced only in a doc-comment today), `Meta.tools`, `CompletionRequest.tools`; `rig_engine.rs:62` still hardcodes `tools: Vec::new()`; the assembly `Tools` arm still concatenates JSON files into system text (`assembly.rs:255`).

**Consequence for the step count — design step 7 collapses.** The design sketched a 7th "CLI `run` wiring" step; that wiring is already committed (`cli/run.rs:97-98`), and the loop reads `&self.meta.tools` off the loaded conversation so the CLI threads tools *implicitly* once Steps 2–3 exist. Nothing remains to wire there. Feature 1 lands in **6 steps**, one under the design's ceiling — the schema/execution split the design held in reserve is not needed, and there is now slack rather than zero slack.

**Baseline at plan time.** `mise run check --all-targets` is GREEN (the feature test *compiles*; it is red at runtime — `[User, Assistant]` ≠ `[User, Assistant, Tool, Assistant]` at `tool_loop.rs:121`). The unit/integration suite is green **except**:
- the feature test `tool_loop.rs` (red by design, flips green at Step 6), and
- three pre-existing e2e gates — `eval_insurance_claim`, `e2e_delegate_52`, `e2e_patterns_eval` — that fail on the `must_call_tool` / `tool_call_order` limitation documented in `e2e/insurance-claim/README.md`. **These are out of scope for Feature 1.** They were not touched by the feature-test commit (`git diff 12cf2ed f9968e8` lists only `cli/run.rs`, `conversation.rs`, `knowledge/mod.rs`, `knowledge/tools/mod.rs`, `engine_provider.rs`, `tool_loop.rs`), they are exercised through e2e fixtures that **Feature 3** owns, and Feature 1 supplies only the upstream `tool_use` producer. Each step below must keep `check` green and must not *newly* break any currently-passing test; it need not turn the three pre-existing e2e reds green (that is Feature 3's job). Treat "green" in each step as "no regression from this baseline."

---

## The frozen-feature-test `Meta` literal (resolved here, because the design missed it)

The feature test constructs its setup `Meta { model, debug, assembly, binding }` with **exactly four fields and no `..` spread** (`tool_loop.rs:65-70`). `Meta` does not (and cannot) derive `Default` — `ModelId` is a `string_newtype!` with no `Default`. Adding `pub tools: Vec<ToolDefinition>` to `Meta` therefore breaks this literal at compile time, along with **14** `Meta {…}` literals across `src/` and `tests/`.

The design's rule "every literal `Meta {..}` construction gains `tools: Vec::new()`" (design §"Meta.tools") covers exactly this mechanical update, but its enumeration omitted `tests/tool_loop.rs:65`. **Resolution:** Step 2 adds `tools: Vec::new()` to *all* `Meta {…}` literals, **including the feature test's setup literal**. This touches only the test's Arrange setup, never its Act or its assertions — the behavioral contract (session-shape assertions at `tool_loop.rs:120-150`) is untouched. A one-line setup edit to keep a struct literal compiling is a mechanical co-change, not a relitigation of the frozen contract; it is the same class of co-change the test author already applied to `engine_provider.rs` in the feature-test commit (updating its `run(&engine)` call to `run(&engine, &executor)`).

**All 14 sites Step 2 must touch:** `assembly.rs:232` (real render path — see Step 5 for the *resolved-vec* variant), `conversation.rs:548` (inline `meta()` helper), `project.rs:781`, `repository.rs:797`, `cli/run.rs:199` & `:355`, `knowledge/assertions.rs:395` & `:1349`, `knowledge/eval.rs:704`, `tests/eval_script.rs:167`, `tests/eval_program_outputs.rs:219`, `tests/eval_assertions.rs:186`, `tests/eval_judge.rs:169`, and `tests/tool_loop.rs:65`.

---

## Step 1: `ToolDefinition` value type

**Enables:** the type that every later step references — `Meta.tools: Vec<ToolDefinition>` (Step 2), `CompletionRequest.tools: &[ToolDefinition]` (Step 3), the assembly resolver's parse target (Step 5). The feature test does not name `ToolDefinition` directly, so no `tool_loop.rs` assertion flips here; this is the domain-value foundation (Step-0 contract item).

Introduce `ToolDefinition` in `src/content/conversation.rs`, next to `Meta` / `ContentBlock` / `ToolUseId` (open #1 resolved → `content/`, per feature design §"Open #1"). Pure value, no behavior:

```rust
/// A tool the model may call, declared by an assembly's `kind: tools` prefix
/// block and carried on `meta.tools`. Mirrors the JSON in
/// `e2e/insurance-claim/context/tools/*.json` and Anthropic's tool shape.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_yaml_ng::Value,
}
```

`content/mod.rs` already re-exports through `pub mod conversation;`, so no export plumbing. `Eq` holds (`serde_yaml_ng::Value: Eq`, matching `ContentBlock`'s derive). The doc-comment in `knowledge/tools/mod.rs:4` that already links `[crate::content::conversation::ToolDefinition]` resolves once the type exists.

**RED test (this step's own unit test):** a round-trip over the real `lookup_policy.json` byte shape — `serde_yaml_ng::from_str::<ToolDefinition>(…)` parses `{ name, description, input_schema }`, and the parsed `name` / `description` / a nested `input_schema` key assert exactly. (JSON is a YAML subset, so the project's `serde_yaml_ng` parser reads the `.json` file body unchanged — the same treatment `ContentBlock::ToolUse.input` already gets.) `check` + tests stay green; the feature test stays red.

## Step 2: `Meta.tools` skip-when-empty field

**Enables:** `meta.tools` becomes a carrier — Step 3 lends `&self.meta.tools` on the request, Step 5 writes resolved defs onto it. No `tool_loop.rs` assertion flips, but the test's setup literal now compiles against the new field (see "frozen-feature-test" section above).

Add to `Meta` (`conversation.rs:53`), after `binding`:

```rust
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub tools: Vec<ToolDefinition>,
```

Then add `tools: Vec::new()` to **all 14** `Meta {…}` literals enumerated above (the `assembly.rs:232` site takes `Vec::new()` here; Step 5 swaps it for the resolved vec).

**Invariants that must still pass (no new test needed — these already exist):** `round_trip_preserves_three_message_fixture` (`conversation.rs:729`) and `meta_defaults_are_skipped_on_emit` (`:737`), over `THREE_MESSAGE_FIXTURE` (`:465`) which omits `tools:`. Because the field is `default` + `skip_serializing_if = "Vec::is_empty"`, an empty `tools` deserializes from the tools-free fixture and never re-emits — both invariants hold unedited, so existing tool-free conversation files stay byte-identical. `check` + tests green; feature test still red.

## Step 3: `CompletionRequest.tools` borrowed field + rig forwarding

**Enables:** the model is *told* its tools. No `tool_loop.rs` assertion flips (the noop engine ignores the request's tools — it serves its script regardless of input), but this lands the Step-0 request-side contract and keeps the rig path honest. It is a prerequisite for the loop being correct end-to-end through a live engine.

`CompletionRequest<'a>` (`engine.rs:24`) gains a borrowed slice mirroring `messages`:

```rust
pub struct CompletionRequest<'a> {
    pub model: ModelId,
    pub messages: &'a [Message],
    pub tools: &'a [ToolDefinition],
    pub debug: bool,
}
```

Update the **two** `CompletionRequest {…}` constructions: the real one in `Conversation::run` (`conversation.rs:465`) passes `tools: &self.meta.tools`; the `request()` test helper (`engine.rs:284`) passes `tools: &[]`. (`assertions.rs` constructs no `CompletionRequest`; it only reads `tool_use` blocks — no change there.)

`rig_engine.rs:62` replaces `tools: Vec::new()` with the lowered Ailly tools:

```rust
tools: request
    .tools
    .iter()
    .map(|t| rig::completion::ToolDefinition {
        name: t.name.clone(),
        description: t.description.clone(),
        parameters: yaml_value_to_json(&t.input_schema),
    })
    .collect(),
```

`yaml_value_to_json` is a 4-line local copy of the proven `yaml_to_json` round-trip (`assertions.rs:1148`) — a private fn in `rig_engine.rs` rather than crossing a module boundary to make the existing one `pub(crate)` (a shared helper is a refactor-phase candidate, not a feature requirement). Existing `messages_to_rig` translation tests (`rig_engine.rs:679+`) test message translation, not request assembly, so they are untouched; with an empty `tools` slice the lowering yields an empty `Vec`, so any rig-request-level coverage stays green.

**RED test (this step's own unit test):** assert the lowering — given a `CompletionRequest` carrying one `ToolDefinition`, the rig `tools` vec has length 1 with the matching `name` and a `parameters` JSON object equal to the yaml→json of `input_schema`; given an empty slice, the rig `tools` vec is empty. `check` + tests green; feature test still red.

## Step 4: `NoopToolExecutor::execute` body

**Enables:** `assert_tool_result(&conversation.session[2], CALL_ID)` (`tool_loop.rs:131`) — the executor can finally produce a `tool_result` echoing the call id. (The assertion only fires once Step 6 appends the tool turn; this step makes the *production* real so Step 6 has something to append.) Replaces the single executable `todo!()` in `src/` (`tools/mod.rs:96`).

Implement `execute`:

```rust
async fn execute(&self, call: &ContentBlock) -> Result<ContentBlock, ToolError> {
    let ContentBlock::ToolUse { id, name, input: _ } = call else {
        return Err(ToolError::NotAToolUse);
    };
    let mut scripts = self.scripts.lock().expect("NoopToolExecutor mutex poisoned");
    let queue = scripts.get_mut(name);
    let reply = queue.and_then(VecDeque::pop_front).ok_or_else(|| {
        ToolError::NoopExhausted { name: name.clone(), call_index: /* per-name index */ }
    })?;
    Ok(ContentBlock::ToolResult {
        tool_use_id: id.clone(),
        content: Content::from(reply),
        is_error: None,
    })
}
```

Mirrors `NoopEngine`'s `Mutex<VecDeque>` pop-in-call-order pattern (the `scripts` field already holds `Mutex<BTreeMap<String, VecDeque<String>>>` and its `#[expect(dead_code, …)]` is removed once the body reads it). The `call_index` for `NoopExhausted` is the per-name call ordinal (track via a sibling `Mutex<BTreeMap<String, usize>>` counter, or report the post-pop depth — implementer's choice; the feature test never exercises the exhausted path, so any honest value satisfies the contract). The `Content::from(String)` impl already exists (`conversation.rs:230`).

**RED test (this step's own unit test):** `from_scripts([("lookup_policy", vec!["status: in_force"])])`, call `execute` with a `ToolUse { id: "toolu_001", name: "lookup_policy", … }`, assert the returned `ToolResult.tool_use_id == "toolu_001"`; a second call to the same tool with an empty queue returns `Err(ToolError::NoopExhausted { name: "lookup_policy", … })`; `execute` on a non-`ToolUse` block returns `Err(ToolError::NotAToolUse)`. `check` + tests green; feature test still red (the loop does not yet call `execute`).

## Step 5: Assembly resolves `kind: tools` to structured defs on `meta.tools`

**Enables:** an assembly that declares `{ kind: tools, … }` now lands `ToolDefinition`s on `meta.tools` instead of a `Role::System` text message — the end-to-end "declare a tool → meta.tools lists it" path (Closing Bell task 1). The feature test constructs its `Conversation` by hand and never calls `Assembly::render`, so no `tool_loop.rs` assertion flips here; this step is required for the **CLI/e2e** path to feed the loop (Features 2/3 depend on it) and is the schema half's last piece.

In `Assembly::render` (`assembly.rs:212`), split prefix-block handling. Non-`Tools` blocks keep producing a `Role::System` text message (unchanged). A `PrefixBlock::Tools` block instead resolves each globbed JSON file into a `ToolDefinition` (parse the file body with `serde_yaml_ng::from_str` — JSON is a YAML subset, consistent with Step 1) and pushes the parsed defs onto a `Vec<ToolDefinition>` that seeds `Meta.tools`. A `Tools` block produces **no** `Role::System` message — it is the one prefix-block kind that stops being concatenated system text.

Mechanics: remove `PrefixBlock::Tools` from the `System | Tools | Examples` text-concat arm in `resolve_prefix_block` (`assembly.rs:255`) and handle it in `render` (reading each globbed file body and parsing). The `Meta {…}` at `assembly.rs:232` (set to `Vec::new()` in Step 2) takes the accumulated `tools` vec. Add a `RenderError` variant for a `tools` JSON file that does not parse into a `ToolDefinition`, so a malformed tool fixture fails loud at assemble time rather than silently dropping the tool.

**Test surface:** any existing assembly test asserting a `kind: tools` block renders to a `Role::System` text message updates to the new output (no system message; defs on `meta.tools`). The `assemble_against_insurance_claim_is_byte_identical_across_runs` test (passes today, declares `kind: tools`) must stay green — its assertion is byte-identity across two assembles, which holds regardless of *where* tools land, but verify it still passes after the render change.

**RED test (this step's own unit test):** seed a memory project with one `tools/*.json` (the `lookup_policy.json` shape), render an assembly whose prefix declares `{ kind: tools, path: tools/*.json }`, assert the resulting `conversation.meta.tools` has length 1 with `name == "lookup_policy"` **and** the session contains no `Role::System` message from that block. A malformed JSON file asserts the new `RenderError` variant. `check` + tests green; feature test still red.

## Step 6: `Conversation::run` tool loop (flips the feature test green)

**Enables:** the whole feature test — the role-shape assertion (`tool_loop.rs:121`, `[User, Assistant, Tool, Assistant]`), `assert_tool_use` (`:128`), `assert_tool_result` (`:131`), the final-assistant-text assertion (`:135-144`), and `next_blank_assistant().is_none()` (`:147`).

Replace the type-first stub body of `Conversation::run` (`conversation.rs:454`, currently `let _ = executor;` + the plain fill loop). Keep the existing "fill the next blank assistant slot" core; after each fill, add the tool turn:

1. Find the next blank assistant slot (`next_blank_assistant`, unchanged) and build the `CompletionRequest` with `tools: &self.meta.tools` (the field added in Step 3); fill the slot (`fill_blank_assistant`, unchanged).
2. Walk the *just-filled* slot's body. If it is `Content::Blocks` containing one or more `ContentBlock::ToolUse` (filter by block kind in block order, mirroring `extract_tool_uses` at `assertions.rs:993`):
   - For each `ToolUse` block in order, call `executor.execute(block).await?` (the `?` maps `ToolError` into `RunError::Tool` via the existing `#[from]`), collecting the returned `ToolResult` blocks.
   - Append one `Message { role: Role::Tool, body: Some(Content::Blocks(results)), cache: false, trace: None, _phase: PhantomData }` after the filled slot.
   - Append a fresh blank assistant slot (`Message { role: Role::Assistant, body: None, … }`).
   - Continue the loop; the next iteration fills that new blank slot from the engine's *next* scripted reply.
3. If the filled slot carries no `ToolUse` blocks (plain `Content::Text`, or `Blocks` with only `Text` / `Thinking`), the loop terminates exactly as today — no new blank slot is appended.

Borrow note: `execute` borrows `&self.session[i]` (the filled slot) while the appends mutate `self.session`. Resolve by cloning the `ToolUse` blocks (or their owned fields) out of the filled slot before the append, so the executor calls and the `push`es do not overlap a borrow — `ContentBlock: Clone` is already derived.

With the noop engine's two scripted replies (`tool_use` then text) and the noop executor's one scripted `status: in_force`, the loop produces `user → assistant(tool_use) → tool(tool_result) → assistant(text)` and terminates when the second (text) reply carries no `tool_use`. The frozen feature test goes green; `engine_provider.rs`'s no-tools `run` test stays green (its replies are plain text, so no tool turn is ever appended). `check` + full suite green except the three pre-existing e2e gates (Feature 3's scope, unchanged by this step).

---

## Dependency flow (why this order)

value type (1) → meta carrier (2) → request field + rig lowering (3) → executor body (4) → assembly resolver (5) → run loop (6). Each arrow is a hard compile/semantic dependency: Step 2 needs the type from 1; Step 3 lends `&self.meta.tools` introduced in 2; Step 6's loop calls the `execute` made real in 4 and reads tools placed by 3; Step 5 is the CLI/e2e feeder that 6's loop consumes through the loaded conversation (independent of the feature test, but required for Features 2/3 and sequenced before 6 so the schema half is complete before the execution half closes). The design's step 7 (CLI wiring) is dropped — already committed in the feature-test stub.

## Step-count justification

6 steps, within the 3–7 band. The design analyzed two at-ceiling bundles and kept them whole; this plan finds the path is actually *shorter* than the design's 7 because the CLI wiring is pre-committed. No bundle needs its own cycle: Step 3's rig lowering reuses the proven `yaml_to_json` seam (empty slice keeps rig tests green); Step 4 is a single method body over an already-built `Mutex<BTreeMap<…>>`; Step 5 is a single render-path split. The schema/execution split the design held in reserve is not invoked. No step requires returning to design to simplify.
