# Feature 1 Design: Harness

**Project:** [../design.md](../design.md) | **Plan:** [../plan.md](../plan.md)
**Type:** Feature (the harness; Feature 1 of 3) | **Status:** Review
**Approach:** A from project design §5 (`meta.tools` + injected `ToolExecutor` + unary completion loop).

## Problem Statement

Ailly's conversation schema already carries the tool-call shapes — `ContentBlock::ToolUse` / `ToolResult` (`conversation.rs:249/254`), `Role::Tool` (`:70`), `ToolUseId` (`:29`) — and the eval read-path already scores them (`extract_tool_uses` at `assertions.rs:993`, `check_must_call_tool` at `:1070`, `check_tool_call_order` at `:1302`). Two gaps keep that machinery dark:

1. The model is never told which tools it may call. `CompletionRequest` (`engine.rs:24`) has no `tools` field, and `rig_engine.rs:62` hardcodes `tools: Vec::new()`. An assembly's `kind: tools` prefix block is concatenated into system text (`assembly.rs:255`) instead of becoming structured tool definitions.
2. `Conversation::run` (`conversation.rs:444`) fills blank assistant slots but never acts on a `tool_use` block — there is no executor and no tool turn.

This feature delivers the Step-0 contract that closes both gaps: `ToolDefinition`, `Meta.tools`, `CompletionRequest.tools` with rig forwarding, the `ToolExecutor` + `NoopToolExecutor` pair, and the `Conversation::run` tool loop. It is the foundation every later feature consumes; a harness with no tool is inert but releasable (tools stay dark until an assembly declares one and `run` is handed an executor — project design §4 *Release Flag*).

## Prior Art (in-repo patterns this feature mirrors)

- **`NoopEngine` script-queue** (`engine.rs:179-267`): a `Mutex<VecDeque<ScriptEntry>>` consumed in call order regardless of input, with `from_scripts` / `from_replies` constructors and an `auto()` empty-but-never-exhausted mode. `NoopToolExecutor` mirrors this exactly — a per-tool-name queue of scripted `tool_result` strings, popped in call order, with an empty `default()` that serves a no-tools conversation (parallels `NoopEngine::auto()` returned by `open_engine_for_model("noop")`).
- **The existing eval read-path** (`assertions.rs`): `extract_tool_uses` (`:993`) walks `Content::Blocks`, keeping only `ToolUse` blocks in (message order, block order); `tool_use_name` (`:1010`) projects the `name`. The run loop's tool-turn detection walks the just-filled slot the same way — `Content::Blocks` filtered to `ToolUse`. The producer this feature adds feeds the consumer that already exists; the implementer wires into existing machinery rather than rebuilding it.
- **`yaml_to_json` bridge** (`assertions.rs:1148`): round-trips `serde_yaml_ng::Value → serde_json::Value` through a JSON string. Rig's `ToolDefinition.parameters` is `serde_json::Value` (rig-core 0.37 `completion/request.rs:191-198`), Ailly's `input_schema` is `serde_yaml_ng::Value`; the rig-forwarding lowering reuses this exact 4-line seam, so the lowering is not its own red-green cycle.
- **The `content` / `knowledge` seam**: assertion *shapes* live in `content/evaluation.rs`; assertion *execution* lives in `knowledge/assertions.rs`. `ToolDefinition` is a schema value (serialized into `meta.tools`), so it follows the *shape* side into `content/`; `ToolExecutor` is behavior, so it follows the *execution* side into `knowledge/tools/`. (Resolves open #1 — below.)
- **The insurance-claim tool JSONs** (`e2e/insurance-claim/context/tools/lookup_policy.json`): `{ name, description, input_schema }`. `ToolDefinition`'s serde shape is fixed to round-trip these byte files unchanged.

## Metrics — what "green" means

- **Feature test passes.** A noop-scripted multi-turn `run` — engine's first scripted reply is an assistant `tool_use` block, the executor returns a scripted `tool_result` — produces the session shape `user → assistant(tool_use) → tool(tool_result) → assistant(text)`, asserting each turn's role and block kind on the resulting conversation. (Written by `developer:feature-test`; not coded in this phase.)
- **Byte-identical round-trip invariant holds.** `round_trip_preserves_three_message_fixture` (`conversation.rs:729`) and `meta_defaults_are_skipped_on_emit` (`:737`) still pass unchanged: `Meta.tools` is `#[serde(default, skip_serializing_if = "Vec::is_empty")]`, and `THREE_MESSAGE_FIXTURE` (`:465`) omits `tools:`, so an empty `tools` is never emitted and existing tool-free conversation files stay byte-for-byte identical.
- **No live API.** Every harness test and the structural CI gate run through `NoopEngine` + `NoopToolExecutor`. No network call. `mise run check` / `mise run test` / `mise run lint` green; `mise run format` clean.

## Specification

### Open #1 (resolved): `ToolDefinition` lives in `src/content/`

`ToolDefinition` is a serde schema value. It is serialized into `Meta.tools` (a `content` type), forwarded by reference on `CompletionRequest` (an `engine` type), and parsed from `PrefixBlock::Tools` JSON files (a `content/assembly` concern). Every consumer is at or below the `content`/`engine` layer; none is in `knowledge`. The codebase's layering rule is explicit in `knowledge/mod.rs` ("Parallel to `content` (domain values) and `engine` (LLM I/O)") — `content` and `engine` must not depend up into `knowledge`. Placing `ToolDefinition` in `knowledge/` would force `Meta` (`content`) and `CompletionRequest` (`engine`) to depend on `knowledge`, inverting the layering. The established precedent settles it: assertion *shapes* live in `content/evaluation.rs` while their checkers live in `knowledge/assertions.rs`; `ToolDefinition` is the schema half of the same kind of split (the executor is the behavior half, and it does go in `knowledge/tools/`).

**Decision:** `ToolDefinition` lands in `src/content/conversation.rs`, next to `Meta`, `ContentBlock`, and `ToolUseId` that reference and surround it — not a new `content/tools.rs`. It is small and tightly coupled to `Meta.tools`; a standalone file earns its keep only when the type grows behavior, which `ToolDefinition` (a pure value) does not. `content/mod.rs` already re-exports through `pub mod conversation;`, so no new export plumbing is needed.

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

(`serde_yaml_ng::Value` is the crate the codebase already uses for schema values — `ContentBlock::ToolUse.input` at `:252`, `ImageSource` at `:273`. `Eq` holds because `serde_yaml_ng::Value: Eq`, matching the `Eq` already derived on `ContentBlock`.)

### `Meta.tools` — new skip-when-empty field

Added to `Meta` (`conversation.rs:53`), after `binding`:

```rust
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub tools: Vec<ToolDefinition>,
```

Every literal `Meta { .. }` construction gains `tools: Vec::new()` (or the resolved vec in `Assembly::render`): `assembly.rs:232` (real render path — carries the resolved tools), and the test/fixture constructions at `conversation.rs:532`, `project.rs:781`, `repository.rs:797`, `cli/run.rs:195` & `:351`, `knowledge/eval.rs:704`, `knowledge/assertions.rs:395` & `:1349`. These are mechanical; the serde-default keeps deserialization of existing files working without them.

### `CompletionRequest.tools` — borrowed field + rig forwarding

`CompletionRequest<'a>` (`engine.rs:24`) gains a borrowed slice mirroring its `messages: &'a [Message]`:

```rust
pub struct CompletionRequest<'a> {
    pub model: ModelId,
    pub messages: &'a [Message],
    pub tools: &'a [ToolDefinition],
    pub debug: bool,
}
```

Borrowed (not owned) so `run` lends `&self.meta.tools` for one call without cloning, consistent with how `messages` is lent via `messages_up_to`. The two literal constructions update: the real one at `conversation.rs:449` passes `tools: &self.meta.tools`; the test helper `request()` at `engine.rs:284` and the eval-path construction at `assertions.rs:367` pass `tools: &[]`.

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

`yaml_value_to_json` is the same `serde_yaml_ng::Value → serde_json::Value` round-trip already implemented as `yaml_to_json` in `assertions.rs:1148`. Rather than make that private fn `pub(crate)` across modules, the rig adapter gets its own one-line copy (the conversion is 4 lines and already duplicated nowhere else; a shared helper is a refactor-phase candidate, not a feature requirement). Existing rig translation tests stay green because an empty `tools` slice lowers to an empty `Vec`.

### `ToolExecutor` trait + `NoopToolExecutor` — `src/knowledge/tools/`

New module `src/knowledge/tools/mod.rs`, registered with `pub mod tools;` in `knowledge/mod.rs`.

```rust
#[async_trait::async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Execute one tool call and produce its result. `call` is a
    /// `ContentBlock::ToolUse`; the return is a `ContentBlock::ToolResult`
    /// whose `tool_use_id` echoes the call's `id`.
    ///
    /// # Errors
    /// Returns [`ToolError`] when no result can be produced for `call`
    /// (e.g. a noop script exhausted for the named tool).
    async fn execute(&self, call: &ContentBlock) -> Result<ContentBlock, ToolError>;
}
```

**Argument/return projection (resolves the Step-0 "Feature 1 design detail"):** `execute` takes the whole `ContentBlock` (the `ToolUse` variant), not a destructured `{id, name, input}` tuple. Reason: the loop already holds `&ContentBlock` from walking `Content::Blocks` (mirroring `extract_tool_uses`), and the executor must echo `id` into the result's `tool_use_id` — passing the block keeps `id`/`name`/`input` together and matches the trait's "one block in, one block out" framing in the project plan. The executor destructures internally:

```rust
let ContentBlock::ToolUse { id, name, input } = call else {
    return Err(ToolError::NotAToolUse);
};
```

and returns `ContentBlock::ToolResult { tool_use_id: id.clone(), content: <result>.into(), is_error: None }`. Returning a `Result` (not a bare `ContentBlock`) lets `NoopToolExecutor` signal script exhaustion the way `NoopEngine` signals `NoopExhausted`; the run loop maps `ToolError` into a `RunError` variant.

```rust
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("no scripted tool result for tool `{name}` (call #{call_index})")]
    NoopExhausted { name: String, call_index: usize },
    #[error("executor received a non-tool_use block")]
    NotAToolUse,
}
```

`NoopToolExecutor` — scripted `tool_result` strings keyed by tool name, consumed in call order per name (mirrors `NoopEngine`'s `VecDeque` queue):

```rust
pub struct NoopToolExecutor {
    /// tool name -> queued result strings, popped front-to-back per call.
    scripts: Mutex<BTreeMap<String, VecDeque<String>>>,
}

impl NoopToolExecutor {
    /// Empty executor: any tool call errors with `NoopExhausted`. The no-tools
    /// default — a conversation that emits no `tool_use` never calls it.
    pub fn new() -> Self { /* empty map */ }

    /// Build from (tool_name, replies) pairs; replies served in call order.
    pub fn from_scripts<I, S>(scripts: I) -> Self
    where
        I: IntoIterator<Item = (S, Vec<String>)>,
        S: Into<String>;
}

impl Default for NoopToolExecutor {
    fn default() -> Self { Self::new() }
}
```

`#[async_trait::async_trait] impl ToolExecutor for NoopToolExecutor`: locks the map, pops the front of the named tool's queue, wraps it in a `ToolResult` echoing `id`; on an empty/absent queue returns `ToolError::NoopExhausted { name, call_index }`. `Send + Sync` via the `Mutex`, exactly like `NoopEngine`.

### Open #2 (resolved): `Conversation::run` always requires an executor

**Decision:** `run` takes `executor: &dyn ToolExecutor` (non-optional); a no-tools conversation passes an empty `NoopToolExecutor::default()`.

Rationale grounded in how the code actually works:
- **Mirrors the established `NoopEngine::auto()` precedent.** `open_engine_for_model("noop")` already returns an empty-but-functional engine so callers never branch on "is there an engine." `NoopToolExecutor::default()` is the executor-side analogue: an empty default that is simply never called when the conversation emits no `tool_use`. An `Option<&dyn ToolExecutor>` would reintroduce exactly the branching the engine side designed away.
- **Ergonomic for `cli/run.rs`.** The call site at `:81` already does `let engine = open_engine_for_model(...)?; ... conv.run(engine.as_ref())`. It gains one line — `let executor = NoopToolExecutor::default();` — and calls `conv.run(engine.as_ref(), &executor)`. No factory, no `None`, no per-conversation conditional. (The plan's surfaced assumption — "no noop executor factory analogous to `NoopEngine::auto()`" — is resolved by `Default`: the default *is* the factory.)
- **Ergonomic for the noop e2e gates.** Those gates run `ailly run` against a noop model; a no-tools conversation gets the default executor for free, and a tool-bearing fixture supplies a scripted `NoopToolExecutor::from_scripts(...)`. Neither path threads an `Option`.
- **No silent-skip footgun.** With `Option::None`, a `tool_use` block in a conversation handed no executor would need a runtime error path anyway (the project plan's "error if a tool_use appears with no executor"). The required-executor design makes that error structurally impossible at the type level: there is always an executor, and an empty one produces a loud `NoopExhausted` rather than a silently-dropped tool turn.

New signature:

```rust
pub async fn run(
    &mut self,
    engine: &dyn crate::engine::engine::EngineProvider,
    executor: &dyn crate::knowledge::tools::ToolExecutor,
) -> Result<(), RunError> { /* loop below */ }
```

`RunError` (`conversation.rs:340`) gains one variant:

```rust
#[error("tool execution failed: {0}")]
Tool(#[from] crate::knowledge::tools::ToolError),
```

### Run-loop tool-turn protocol

The loop body at `conversation.rs:448` keeps its existing "fill the next blank assistant slot" core and adds a tool turn after each fill:

1. Find the next blank assistant slot (`next_blank_assistant`, unchanged).
2. Build the `CompletionRequest` with `messages: self.messages_up_to(index)`, `tools: &self.meta.tools`, fill the slot (`fill_blank_assistant`, unchanged).
3. Inspect the *just-filled* slot's body. If it is `Content::Blocks` containing one or more `ContentBlock::ToolUse` (walk it the way `extract_tool_uses` does — by block kind, in block order):
   - For each `ToolUse` block in order, call `executor.execute(block).await?`, collecting the returned `ToolResult` blocks.
   - Append one `Message { role: Role::Tool, body: Some(Content::Blocks(results)), cache: false, trace: None, _phase: PhantomData }` after the filled slot.
   - Append a fresh blank assistant slot (`Message { role: Role::Assistant, body: None, .. }`) after the tool message.
   - Continue the loop; the next iteration fills that new blank slot.
4. If the filled slot carries no `ToolUse` blocks (plain `Content::Text`, or `Content::Blocks` with only `Text`/`Thinking`), the loop terminates exactly as it does today (no new blank slot is appended).

This preserves the inline-trace and mid-run-save guarantees: each assistant turn is filled and its trace populated before the next request, and the appended `Role::Tool` message + blank slot are part of the conversation aggregate that `cli/run.rs:96` saves. It mirrors the Anthropic agentic loop (`tool_use` → `tool_result` → next turn) with `model.completion()` kept unary (project design §2, Approach A).

### Assembly-resolver change (`PrefixBlock::Tools` stops being text)

Today `resolve_prefix_block` (`assembly.rs:243`) returns a `String` and the `System | Tools | Examples` arm (`:254`) globs+concats JSON files into system text, which `render` (`:212`) pushes as a `Role::System` message. For `Tools` only, that becomes structured definitions on `meta.tools`:

- `Assembly::render` splits prefix-block handling. Non-`Tools` blocks keep producing a `Role::System` text message (unchanged path). A `PrefixBlock::Tools` block instead resolves each globbed JSON file into a `ToolDefinition` (via `serde_yaml_ng::from_str` / `serde_json::from_str` over the file body — JSON is a YAML subset, so the existing `serde_yaml_ng` parser reads these `.json` files; this matches how the codebase already treats `input_schema` as a `serde_yaml_ng::Value`) and pushes the parsed defs onto the `tools` vec that seeds `Meta.tools`. The `Tools` arm is removed from the text-concatenation match arm at `:254` and handled separately in `render`.
- The render loop therefore accumulates a `Vec<ToolDefinition>` alongside the session; `Meta { .. tools }` is constructed with it. A `Tools` block produces **no** `Role::System` message — it is the one prefix-block kind that stops being concatenated text (project design §4).
- A new `RenderError` path covers a `tools` JSON file that does not parse into a `ToolDefinition` (malformed schema file), so a bad tool fixture fails loud at assemble time rather than silently dropping the tool. Assembly round-trip tests that exercised `kind: tools` as text update to the new render output (no system message; defs on `meta.tools`).

### Rig forwarding (summary)

`rig_engine.rs:62`: `tools: Vec::new()` → lower `request.tools` into `rig::completion::ToolDefinition { name, description, parameters }`, converting `input_schema: serde_yaml_ng::Value` to `parameters: serde_json::Value` via the yaml→json round-trip. Empty input ⇒ empty output ⇒ existing rig tests unchanged. This is the only line that wires Ailly tools onto a live request; until an assembly declares a `tools` block, every request still forwards an empty slice (releasable-at-every-step).

### CLI `run` wiring (`cli/run.rs`)

`fill_and_save` (`:86`) and its caller (`:81`) thread an executor. The simplest placement: construct `let executor = NoopToolExecutor::default();` in `run` (`:72`) once, pass `&executor` through `fill_and_save` into `conv.run(engine, &executor)`. `tools` flow automatically because `run` reads `&self.meta.tools` off the loaded conversation — `cli/run.rs` does not pass tools explicitly. (For v1 the CLI executor is always the empty noop; a real executor registry is a follow-on concern — project design §6. The empty default is correct today because no in-tree tool exists until Feature 2, and the e2e gates script their own `NoopToolExecutor`.)

### DESIGN.md change (ships with this feature)

Per the documentation-sync rule, the schema doc moves in lockstep with the code:
- `conversation.meta` (`DESIGN.md:12-16`) gains `tools?: ToolDefinition[]`, with a `ToolDefinition` shape (`name` / `description` / `input_schema`) documented.
- The assembly prose "the engine treats every block as ordered text concatenated into the window" (`DESIGN.md:64`) is amended: `kind: tools` resolves to structured `ToolDefinition`s carried on `meta.tools`, not text — the one prefix-block kind that stops being concatenated system text.
- The agentic-loop shape (assistant `tool_use` → `Role::Tool` `tool_result` → next blank assistant turn) is documented alongside the existing "blank assistant slot" prose (`DESIGN.md:37`).

## Split-vs-whole decision: keep Feature 1 whole (7 steps)

The project plan's *At-ceiling flag* warned Feature 1 sits at the 7-step max and flagged two bundles (step 3: borrowed field + rig lowering; step 4: `ToolExecutor` + `NoopToolExecutor`) that, if either needs its own step, tip the build over 7 and force a schema/execution split. Verifying both bundles against the actual code, neither needs splitting:

- **Step 3's rig lowering reuses an existing, proven 4-line seam** (`yaml_to_json`, `assertions.rs:1148`). The lowering is a `map`-and-`collect` over a borrowed slice; an empty slice keeps existing rig tests green. It does not warrant its own red-green cycle — adding the field and forwarding it land in one cycle.
- **Step 4's `NoopToolExecutor` mirrors `NoopEngine`'s queue directly** — a `Mutex<BTreeMap<String, VecDeque<String>>>` popped per name in call order, the same machinery as `NoopEngine`'s `Mutex<VecDeque<ScriptEntry>>`. The trait is one async method. Trait + noop impl land together because the noop is the trait's first and only consumer in this feature.

Therefore **keep Feature 1 as one 7-step feature**, exactly the project plan's sketch:
1. `ToolDefinition` value type (round-trips a `*.json` fixture).
2. `Meta.tools` skip-when-empty field (byte-identical round-trip + default-skip invariants hold).
3. `CompletionRequest.tools` borrowed field + rig forwarding (existing rig tests green).
4. `ToolExecutor` + `NoopToolExecutor` in `knowledge/tools/` (scripted results in call order).
5. Assembly resolves `Tools` to structured defs on `meta.tools` (assembly round-trip tests updated).
6. `Conversation::run` tool loop (feature-test session-shape assertion passes).
7. CLI `run` wiring (empty `NoopToolExecutor` default; `ailly run` drives the loop end to end).

Each step is a clean single red-green-refactor cycle leaving `check` + tests green. A schema/execution split (schema half: steps 1, 2, 5; execution half: steps 3, 4, 6, 7) is the pre-identified fallback if any step is found at build time to need its own cycle — but the analysis above shows it does not, so the whole-feature form is chosen.

## Alternatives

The loop-owner and tool-flow alternatives are settled at project altitude (design §5 — **Approach A** chosen over Rig `AgentBuilder.multi_turn`, which hides intermediate turns and breaks inline-trace/mid-run-save, and over tools-as-text, which breaks replay-from-file). This feature does not re-litigate them. The two feature-local alternatives are the open decisions, resolved above:
- **Open #1**: `ToolDefinition` in `knowledge/` (rejected — inverts the content/engine→knowledge layering) vs `content/` (chosen — schema value, matches the evaluation-shape precedent).
- **Open #2**: `Option<&dyn ToolExecutor>` (rejected — reintroduces the branching `NoopEngine::auto()` designed away, and still needs a runtime error for an executor-less `tool_use`) vs always-required with an empty `NoopToolExecutor` default (chosen — mirrors `NoopEngine::auto()`, ergonomic for both the CLI and the noop e2e gates, makes the silent-skip footgun structurally impossible).

A minor sub-decision settled here: `ToolDefinition` in `content/conversation.rs` (next to `Meta`) rather than a new `content/tools.rs` — it is a small pure value tightly coupled to `Meta.tools`; a standalone module is unearned until the type grows behavior.

## Summary

- **Open #1 — resolved: `ToolDefinition` in `src/content/conversation.rs`.** It is a serde schema value serialized into `Meta.tools` and forwarded on `CompletionRequest`; both consumers sit at/below the `content`/`engine` layer, and `content`/`engine` must not depend up into `knowledge`. Matches the precedent where assertion *shapes* live in `content/` and their *execution* lives in `knowledge/`.
- **Open #2 — resolved: `Conversation::run(&mut self, engine, executor: &dyn ToolExecutor)`, always required, empty `NoopToolExecutor::default()` for no-tools.** Mirrors the `NoopEngine::auto()` empty-default precedent, keeps the loop branch-free, is one extra line at `cli/run.rs`, and makes an executor-less `tool_use` impossible by construction.
- **Split decision — keep Feature 1 whole at 7 steps.** Both at-ceiling bundles (step 3 rig lowering, step 4 noop executor) reuse proven in-repo seams (`yaml_to_json`; the `NoopEngine` queue) and each fits a single red-green cycle; the schema/execution split is the held-in-reserve fallback, not needed.
</content>
