# Implementation Plan: Tool call handling in the engine

**Feature test:** `tests/engine/tool_round_trip.rs`
**User story:** A developer drives `Generator` over a one-turn `Conversation` with an `echo` tool registered, and the resulting `TurnEvent` stream and on-disk TOML file both surface the single tool round-trip in stream order.
**Steps:**
- [ ] Step 1: Engine surface — signature, `Settings::max_tool_turns`, `StopReason::ToolLimit`, `EngineEvent::ToolCall`/`ToolResult`
- [ ] Step 2: Content surface — `MessageFile`, `TurnMessage`, `history_for` grouping, recorder methods
- [ ] Step 3: Generator surface — `TurnEvent` variants, `with_tools` builder, run-loop forwarding
- [ ] Step 4: `RigEngine` wires tools and surfaces tool events — `DynToolHandle`, `multi_turn`, new `MultiTurnStreamItem` arms, `ToolLimit` mapping

## Step 1: Engine surface

**Enables:** the feature test compiles past `Engine::stream` calls and `Settings::default()`. The compile-time errors at [tool_round_trip.rs:154](tests/engine/tool_round_trip.rs#L154) and [tool_round_trip.rs:162](tests/engine/tool_round_trip.rs#L162) remain (those land in steps 3 and 4). `EngineEvent` and `StopReason` themselves stop being a blocker.

Implement, in [src/engine/mod.rs](src/engine/mod.rs):

- Add field `pub max_tool_turns: usize` to `Settings`. Default = `5`. Switch the manual `impl Default for Settings` to `#[derive(Default)]` only if every existing field is `Default`-friendly; otherwise extend the manual impl with `max_tool_turns: 5`.
- Add `EngineEvent::ToolCall(rig::message::ToolCall)` and `EngineEvent::ToolResult(rig::message::ToolResult)` variants. The enum is already `#[non_exhaustive]`.
- Add `StopReason::ToolLimit`. Extend `impl fmt::Display for StopReason` so `ToolLimit => "tool_limit"`. Extend the existing snake-case test in [src/engine/mod.rs:108-119](src/engine/mod.rs#L108-L119) with the new variant.
- Change `Engine::stream` to take `tools: &[Arc<dyn rig::tool::ToolDyn>]` between `settings` and `request_label`:

```rust
fn stream(
    &self,
    history: Vec<Message>,
    settings: &Settings,
    tools: &[Arc<dyn rig::tool::ToolDyn>],
    request_label: &str,
) -> anyhow::Result<EngineStream>;
```

Implement, in [src/engine/noop.rs](src/engine/noop.rs):

- Add `_tools: &[Arc<dyn rig::tool::ToolDyn>]` to `Noop::stream`. Ignore it. Keep behavior byte-identical for the empty-slice case.
- Update the three Noop tests in that file to pass `&[]` for the new parameter.

Implement, in [src/engine/rig_engine.rs](src/engine/rig_engine.rs):

- Add `_tools: &[Arc<dyn rig::tool::ToolDyn>]` to `RigEngine::stream` and ignore it for now (step 4 wires it in).
- Update the existing tests in this file that call `engine.stream(...)` to pass `&[]`.

Implement, in [src/engine/generator.rs](src/engine/generator.rs):

- Update the existing `self.engine.stream(history, &self.settings, path.as_str())` call to pass `&[]` for tools. The Generator's tool slice arrives in step 3.
- Extend the inner `match ev` arm-set so the new `EngineEvent::ToolCall` and `EngineEvent::ToolResult` variants compile. v1 inside this step: a catch-all `_ => {}` arm. Step 3 fills these in with real handling.
- Update the in-file test impls of `Engine` (`MetadataEngine` at [src/engine/generator.rs:178-202](src/engine/generator.rs#L178-L202)) to take the new parameter.

`cargo check`, `cargo test`, `cargo test --features bedrock`, and `cargo build` all pass at the end of this step. The feature test still fails to compile on `with_tools` and the missing `TurnEvent` variants.

## Step 2: Content surface

**Enables:** the file-write assertions at [tool_round_trip.rs:228-269](tests/engine/tool_round_trip.rs#L228-L269) become reachable once steps 3–4 produce the data. `From` impls and a new recorder method exist that step 3 will call.

Implement, in [src/content/mod.rs](src/content/mod.rs):

- Add to `MessageFile`:

```rust
ToolCall {
    id: String,
    name: String,
    arguments: serde_json::Value,
},
ToolResult {
    id: String,
    content: String,
},
```

  Both fields use the existing `#[serde(tag = "role", rename_all = "lowercase")]` envelope, producing `role = "tool_call"` and `role = "tool_result"` on disk.

- Add to `TurnMessage`:

```rust
ToolCall(rig::message::ToolCall),
ToolResult(rig::message::ToolResult),
```

- Extend `From<MessageFile> for TurnMessage` and `From<TurnMessage> for MessageFile` to round-trip both new variants. For `ToolResult`, only `rig::message::ToolResultContent::Text` round-trips on the write path; other variants return a new `ContentError::NonTextToolResult { path }` error built by the caller (parallel to the existing `ContentError::NonTextUserPrompt`). For v1, `MessageFile::ToolResult { id, content }` always builds a `rig::message::ToolResult` whose `content` is `OneOrMany::one(ToolResultContent::Text(content))`.
- Replace the body of `From<&TurnMessage> for Message` for the `Assistant`/`User`/`System` arms with `unreachable!("history_for handles grouping")` so accidental future use breaks loudly. Keep the impl on the type so existing tests that reference it still compile, but reroute the only in-tree caller (`history_for`) to a new helper. *Alternative if cleaner:* delete the impl outright and update `history_for` in the same change. The skill leaves implementer's choice.
- Replace the body of `history_for` so it walks `prev.response` runs and emits the grouped `Vec<Message>` shape rig requires:
  - Group runs of consecutive `TurnMessage::Assistant` and `TurnMessage::ToolCall` belonging to the same model turn into one `Message::Assistant` whose `OneOrMany<AssistantContent>` carries text content per assistant entry and `AssistantContent::ToolCall(...)` per tool-call entry.
  - Each `TurnMessage::ToolResult` becomes its own `Message::User` whose `UserContent::ToolResult(...)` carries the rig value.
  - A new model turn begins after a `ToolResult` followed by either an `Assistant` or `ToolCall`, or after a top-level `User` message.
  - The trailing-assistant pop ([src/content/mod.rs:439-443](src/content/mod.rs#L439-L443)) keeps its current behavior on the new grouped output.
- Extend `Conversation::record_response` (or add `record_tool_call(&mut self, idx: usize, call: rig::message::ToolCall)` and `record_tool_result(&mut self, idx: usize, result: rig::message::ToolResult)` next to it) so the Generator has a single method per `EngineEvent` variant. Decide between extending one method and adding two during implementation; the test in [src/engine/generator.rs:128-171](src/engine/generator.rs#L128-L171) and the new feature test do not constrain the choice.

Existing content tests (round-trip assistant metadata, `history_for_*`, etc.) remain green. New content unit tests are deferred to step refactor; this step is structural plumbing.

`cargo check`, `cargo test`, `cargo test --features bedrock` all pass. The feature test still fails to compile on `Generator::with_tools` and `TurnEvent::ToolCall`/`ToolResult`.

## Step 3: Generator surface

**Enables:** the feature test compiles end-to-end. The `position` lookups at [tool_round_trip.rs:160-167](tests/engine/tool_round_trip.rs#L160-L167) start running. Without step 4, they still fail at runtime because `RigEngine` does not yet emit the tool events.

Implement, in [src/engine/generator.rs](src/engine/generator.rs):

- Add to `TurnEvent`:

```rust
ToolCall   { path: VfsPath, call: rig::message::ToolCall },
ToolResult { path: VfsPath, result: rig::message::ToolResult },
```

- Add field `tools: Vec<Arc<dyn rig::tool::ToolDyn>>` to `Generator`, default empty in `Generator::new`.
- Add builder:

```rust
pub fn with_tools(mut self, tools: Vec<Arc<dyn rig::tool::ToolDyn>>) -> Self {
    self.tools = tools;
    self
}
```

- In `run`, change the `engine.stream(...)` call site to pass `&self.tools`.
- Replace the catch-all arms added in step 1 with real handling:
  - `EngineEvent::ToolCall(call)`: call the conversation recorder added in step 2, then `yield TurnEvent::ToolCall { path: path.clone(), call }`.
  - `EngineEvent::ToolResult(result)`: call the conversation recorder, then `yield TurnEvent::ToolResult { path: path.clone(), result }`.
- The existing `EngineEvent::Final` arm still drives the file write. The file now contains, in load order, the assistant-text entries plus the tool-call/tool-result entries that were recorded during the loop.

`cargo check`, `cargo test`, `cargo test --features bedrock`, `cargo build --tests` all pass except for the feature test, which now compiles but asserts on `TurnEvent::ToolCall` not appearing (because step 4 has not landed yet). At this point the failure is a runtime panic out of `events.iter().position(...).expect("expected a TurnEvent::ToolCall")`, not a compile error.

## Step 4: RigEngine wires tools and surfaces tool events

**Enables:** every assertion in the feature test (`engine_surfaces_one_tool_round_trip_through_generator_and_file`) at [tool_round_trip.rs:140-270](tests/engine/tool_round_trip.rs#L140-L270).

Implement, in [src/engine/rig_engine.rs](src/engine/rig_engine.rs):

- Add a private newtype:

```rust
struct DynToolHandle(Arc<dyn rig::tool::ToolDyn>);

impl rig::tool::ToolDyn for DynToolHandle {
    // forward every method on ToolDyn to self.0
}
```

  This sidesteps the fact that `Box<dyn ToolDyn>` is not `Clone` while `AgentBuilder::tools` takes `Vec<Box<dyn ToolDyn>>`. The wrapper lets the engine re-box per call while the caller's `Arc` stays shared.

- In `stream`, after the existing `AgentBuilder::new(model)` and preamble wiring, build:

```rust
let dyn_tools: Vec<Box<dyn ToolDyn>> = tools
    .iter()
    .map(|t| Box::new(DynToolHandle(t.clone())) as Box<dyn ToolDyn>)
    .collect();
builder = builder.tools(dyn_tools);
```

- Apply `.multi_turn(settings.max_tool_turns)` to the request returned by `agent.stream_chat(last_user_text, prior)`. Note that `multi_turn` lives on `StreamingPromptRequest`, not on the builder, so it lands on the prompt-request value.
- Add three new arms inside the `loop { match stream.next().await { ... } }` over `MultiTurnStreamItem`:

```rust
Some(Ok(MultiTurnStreamItem::StreamAssistantItem(
    StreamedAssistantContent::ToolCall { tool_call, .. },
))) => {
    yield EngineEvent::ToolCall(tool_call);
}
Some(Ok(MultiTurnStreamItem::StreamUserItem(
    StreamedUserContent::ToolResult { tool_result, .. },
))) => {
    yield EngineEvent::ToolResult(tool_result);
}
Some(Err(rig::streaming::StreamingError::Prompt(boxed)))
    if matches!(*boxed, rig::completion::request::PromptError::MaxTurnsError { .. }) =>
{
    break StopReason::ToolLimit;
}
```

  Existing `ToolCallDelta`, `Reasoning`, and `ReasoningDelta` continue to fall through the catch-all `Some(Ok(_)) => {}` arm. Other `Err` cases continue to map onto `StopReason::Error(...)`.

- Read the `tools` slice the engine was given. The parameter rename in step 1 used `_tools`; drop the underscore in this step.

`cargo check`, `cargo test`, `cargo test --features bedrock` all pass, including the feature test `engine_surfaces_one_tool_round_trip_through_generator_and_file`.

After this step, run the metric checks named in the deferred TASKS entry:

```sh
cargo build
cargo test
cargo test --features bedrock
./e2e/e2e.sh
grep -rE "ContentMeta|ConversationTurn|Conversation|aillyrc" src/engine/   # expected: empty
```

The deferred narrow tests (round-trip byte-equality, `StopReason::ToolLimit` exhaustion, empty-tool-slice parity, multi-round-trip ordering, metadata-leakage `grep`) are appropriate refactor-pass additions once the four steps are green.
