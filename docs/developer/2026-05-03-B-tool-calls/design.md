# Tool call handling in the engine

## Problem Statement

The engine layer currently ignores tool-use signals from the underlying provider.
`src/engine/rig_engine.rs:88-90` documents this with a comment that
`StreamedAssistantContent::ToolCall`, `ToolCallDelta`, `Reasoning`, and
`ReasoningDelta` are dropped on the floor. Consumers cannot observe a tool loop,
the conversation TOML file cannot record one, and `Settings` has no way to bound
how many tool round-trips the engine will perform before giving up.

This component closes that gap. It surfaces tool calls and their results as
first-class `EngineEvent`, `TurnEvent`, and `MessageFile` entries, lets the
caller bound the loop with `Settings::max_tool_turns`, and adds a new
`StopReason::ToolLimit` variant so a downstream consumer can branch on
exhaustion without parsing strings.

The component does not introduce a tool dispatcher of our own. Rig already runs
the multi-turn loop inside `Agent::stream_chat` and dispatches every tool that
was registered with the agent via `AgentBuilder::tool` or
`AgentBuilder::tools`. We register the caller-supplied tools on the agent and
let rig drive. Where those tools come from (per-conversation `.ailly.toml`, a
future Workflow concept, the CLI) is explicitly out of scope. The engine
accepts an opaque slice of `Arc<dyn rig::tool::ToolDyn>` and observes whatever
rig emits.

## Prior Art

- `rig::agent::AgentBuilder` accepts tools via `.tool(t)` for static `rig::tool::Tool` impls (builder.rs:276) and via `.tools(Vec<Box<dyn ToolDyn>>)` for dyn-compat tool collections (builder.rs:304). There is no `tool_dyn` setter on the builder. The per-call `StreamingPromptRequest::multi_turn(n)` (`agent/prompt_request/streaming.rs:303`) caps the number of agent-driven tool round-trips. The default is `0`, which permits a single tool round-trip per the docstring at `agent/prompt_request/streaming.rs:198-199`. Exhaustion surfaces as `StreamingError::Prompt(Box<PromptError>)` per `agent/prompt_request/streaming.rs:188`, where the boxed inner is the struct variant `PromptError::MaxTurnsError { .. }` defined at `completion/request.rs:136`.
- `rig::streaming::MultiTurnStreamItem` (defined at `rig-core-0.36.0/src/agent/prompt_request/streaming.rs:37-44`) tags every streamed item as either `StreamAssistantItem(StreamedAssistantContent<R>)`, `StreamUserItem(StreamedUserContent)`, or `FinalResponse(FinalResponse)`.  `StreamedAssistantContent::ToolCall { tool_call, internal_call_id }` carries the model's request, and `StreamedUserContent::ToolResult { tool_result, internal_call_id }` carries the dispatched result that rig fed back into the next request.
- The TypeScript port at `typescript/core/src/engine/tool.ts` defined a `Tool` shape (name, description, JSON-schema parameters) decoupled from any executor, plus a `ToolInformation { client: Client, tool: Tool }` envelope that paired the schema with an MCP `Client`. The OpenAI engine at `typescript/core/src/engine/openai.ts:63-126` read tool definitions from per-content meta (`c.meta?.tools`) and recorded streamed tool-use deltas into a `debug.toolUse` array. The Rust port deliberately does not port MCP in this slice.
- The prior Rust engine design (`docs/developer/2026-05-01-A-engine/`, no longer on disk but referenced in `TASK-NOTES-engine-deferred.md` and `TASK-NOTES-rig-engine-adapter.md`) established a metadata-leakage invariant: the engine layer must remain free of `ContentMeta`, `Conversation`, and `aillyrc` references. The current design preserves it.  The `TurnEvent` enum's per-path tagging convention also comes from that prior design.
- The conversation file format already round-trips assistant metadata (engine name, model id, stop reason, usage) per the `feat(content+engine): round-trip assistant metadata through file format` commit. Tool calls and results extend the same pattern with two new role variants instead of grafting tool fields onto `MessageFile::Assistant`.

## Metrics

The deployed component is operating within acceptable constraints when each of the following holds.

- Engine layer remains free of `ContentMeta`, `Conversation`, and `aillyrc` references. Verified by `grep -E "ContentMeta|ConversationTurn|Conversation|aillyrc" src/engine/` returning no rows after the change, matching the metric used in `docs/developer/TASKS.md`.
- Tool calls and tool results round-trip byte-equivalently through the per-turn TOML file. A turn loaded, written, loaded again, and written again produces byte-identical output, mirroring the existing `clean` invariant.
- `Settings::max_tool_turns` caps the loop. A test that registers a tool whose output always provokes another call terminates with `StopReason::ToolLimit` inside `max_tool_turns + 1` events.
- Both `Noop` and `RigEngine` accept an empty tool slice without panicking and produce a byte-equal `TurnEvent` sequence and byte-equal written TOML when `tools: &[]` is passed compared to a run on the pre-change signature.
- `EngineEvent::ToolCall` and `EngineEvent::ToolResult` arrive in the order rig emits them, with no reordering across the two streams. A test inspects a fake-model trace and asserts the event sequence matches `[ToolCall, ToolResult, Text*, Final]` for a single round-trip.
- Default `request_limit` semantics are unchanged. Existing isolated-partition concurrency tests still pass. `max_tool_turns` is a new orthogonal field.

## Specification

### Engine trait and signature

`Engine::stream` grows a tools parameter.

```rust
fn stream(
    &self,
    history: Vec<rig::message::Message>,
    settings: &Settings,
    tools: &[Arc<dyn rig::tool::ToolDyn>],
    request_label: &str,
) -> anyhow::Result<EngineStream>;
```

Both implementations of `Engine` (`Noop`, `RigEngine`) update to the new
signature in the same change. The slice borrows so callers retain ownership of
the tool collection across many turns. `Arc<dyn ToolDyn>` is the rig-recommended
shape for sharing a single tool implementation across calls without re-cloning
the inner state.

We accept rig's `ToolDyn` trait at the engine boundary on purpose. Wrapping it
in an `EngineTool` shim would add an indirection we cannot use yet. When MCP
or another adapter joins later, the shim can be added at that point with
real cause to.

### Settings

`Settings` gains one field.

```rust
#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub model: Option<String>,
    pub request_limit: usize,
    pub isolated: bool,
    pub overwrite: bool,
    pub max_tool_turns: usize,
}
```

`request_limit` keeps its existing meaning as the isolated-partition
concurrency bound from the prior engine design. `max_tool_turns` maps
directly onto rig's per-call `StreamingPromptRequest::multi_turn(n)` setting.
`max_tool_turns: 0` permits a single tool round-trip per the rig default.
Higher values raise the cap. We default to `5` so a typical multi-step tool
conversation is not capped prematurely. The `Default` derive keeps the field
consistent with the rest of the struct, which is constructed with defaults in
several test paths.

### EngineEvent

```rust
#[non_exhaustive]
pub enum EngineEvent {
    Text(String),
    ToolCall(rig::message::ToolCall),
    ToolResult(rig::message::ToolResult),
    Final(EngineResponse),
}
```

`ToolCall` and `ToolResult` carry rig's already-correlated values. The
`internal_call_id` rig attaches to both halves of a pair survives because
both `rig::message::ToolCall` and `rig::message::ToolResult` already encode
the correlation `id` (and `call_id` where the provider supplies one). We do
not flatten or rename these for v1.

### StopReason

```rust
#[non_exhaustive]
pub enum StopReason {
    EndTurn,
    StopSequence,
    MaxTokens,
    Refusal,
    Error(String),
    ToolLimit,
}
```

`Display` returns `tool_limit`. The conversation file format records it as
`stop_reason = "tool_limit"`. The engine maps rig's
`StreamingError::Prompt(Box<PromptError>)` whose inner is the struct variant
`PromptError::MaxTurnsError { .. }` onto this variant. Other `StreamingError`
cases continue to map onto `StopReason::Error(...)` as today.

### TurnEvent

```rust
#[non_exhaustive]
pub enum TurnEvent {
    Started   { path: VfsPath },
    Delta     { path: VfsPath, text: String },
    ToolCall  { path: VfsPath, call: rig::message::ToolCall },
    ToolResult{ path: VfsPath, result: rig::message::ToolResult },
    Skipped   { path: VfsPath, reason: SkipReason },
    Finished  { path: VfsPath, response: String, stop_reason: StopReason, usage: Option<Usage> },
    Failed    { path: VfsPath, error: Arc<anyhow::Error> },
}
```

Order matches the order rig emits them inside the loop. A consumer that wants
to render mid-loop progress (CLI, TUI, future Workflow) can do so by
discriminating on the variant. Consumers that only care about the final
assistant text can keep ignoring `ToolCall` and `ToolResult` exactly as they
ignore `Delta` today.

### MessageFile and TurnMessage

`MessageFile` adds two role variants alongside `User`, `Assistant`, `System`.

```rust
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
enum MessageFile {
    User       { text: String },
    Assistant  { text: String, model: Option<String>, engine: Option<String>,
                 stop_reason: Option<String>, usage: Option<UsageFile> },
    System     { text: String },
    ToolCall   { id: String, name: String, arguments: serde_json::Value },
    ToolResult { id: String, content: String },
}
```

`TurnMessage` mirrors the new variants.

```rust
pub enum TurnMessage {
    User(String),
    Assistant(AssistantResponse),
    System(String),
    ToolCall(rig::message::ToolCall),
    ToolResult(rig::message::ToolResult),
}
```

`TurnMessage` carries rig's tool types directly. The metadata-leakage
invariant applies to engine internals. `TurnMessage` lives in `content`,
which is the layer that already round-trips engine and provider data, so
binding to `rig::message` here is acceptable. Conversions go in both
directions inside `src/content/mod.rs`, parallel to the existing
`From<MessageFile> for TurnMessage` impl.

`AssistantResponse` is unchanged. Tool calls live as their own
`TurnMessage::ToolCall` entries adjacent to the assistant text, in stream
order, instead of as a `tool_calls` field hanging off of the assistant entry.
This keeps every entry in the file flat at one role per `[[response]]` table,
the way the format already works.

`rig::message::ToolCall` per `completion/message.rs:231` carries `id: String`,
`call_id: Option<String>`, `function: ToolFunction { name: String, arguments:
serde_json::Value }`, plus `signature` and `additional_params`. The
`MessageFile::ToolCall` variant is built from `tc.id`, `tc.function.name`, and
`tc.function.arguments`. v1 drops `call_id`, `signature`, and
`additional_params`. Dropping `call_id` discards the correlation semantics
that some providers require for follow-up requests. v1 explicitly defers
`call_id` round-trip. A future revision will add `call_id: Option<String>` to
the file variant once a provider exercises the path.

For v1, `ToolResult.content` is a plain `String`. Rig's `ToolResultContent`
supports image and multi-part content too. That is deferred until a real
caller needs it. v1 expects `ToolResultContent::Text`. Other content variants
return `ContentError::NonTextToolResult` from the load and write boundary,
mirroring `ContentError::NonTextUserPrompt` that the file format already
returns for non-text user prompts.

#### Interleaved text and tool calls in one model turn

A single model turn may emit text, a tool call, and more text in sequence.
Each emission becomes its own `[[response]]` table entry, in stream order,
even when they belong to the same model turn. A model turn that yields
"I'll search," then `ToolCall(search)`, then "Result was useful." records as
three response entries (`Assistant`, `ToolCall`, `Assistant`) in that order.
The "same turn" grouping is recovered on the load side by the `history_for(...)`
helper described in the next subsection.

#### Building rig history from `TurnMessage`

Rig groups tool calls inside an assistant message via
`OneOrMany<AssistantContent>`. A single-variant `From<&TurnMessage> for
rig::message::Message` impl cannot produce the grouped shape rig requires for
`ToolCall`. Instead, a `history_for(...)` helper walks `TurnMessage` runs and
emits the grouped `Vec<rig::message::Message>` shape rig expects.

The helper rules are.

- Walk the response in order. Group runs of consecutive `Assistant` and
  `ToolCall` entries belonging to the same model turn into a single
  `Message::Assistant` whose `OneOrMany<AssistantContent>` carries text
  content for each `Assistant` entry and `AssistantContent::ToolCall` for
  each `ToolCall` entry.
- Each `ToolResult` entry becomes its own `Message::User` whose
  `UserContent::ToolResult` carries the result.
- A fresh model turn begins after a `ToolResult` entry that is followed by
  either an `Assistant` or `ToolCall` entry, or after a top-level `User`
  message.

### RigEngine implementation

Inside `RigEngine::stream`:

1. Wrap each `Arc<dyn ToolDyn>` in a small newtype
   `struct DynToolHandle(Arc<dyn ToolDyn>)` that itself implements `ToolDyn` by
   forwarding every method to the inner `Arc`. This is necessary because
   `AgentBuilder::tools` takes `Vec<Box<dyn ToolDyn>>`, and `Box<dyn ToolDyn>`
   is not `Clone`. The wrapper lets the engine box a fresh handle per call
   while the caller keeps a reusable `Arc` slice across many turns.
2. Build the agent: `let mut builder = AgentBuilder::new(model);` plus the
   existing `preamble` wiring, then collect the tools as
   `let dyn_tools: Vec<Box<dyn ToolDyn>> = tools.iter()
       .map(|t| Box::new(DynToolHandle(t.clone())) as Box<dyn ToolDyn>)
       .collect();`
   and call `builder = builder.tools(dyn_tools);`.
3. Call
   `agent.stream_chat(last_user_text, prior).multi_turn(settings.max_tool_turns).await`.
   The `multi_turn` setter lives on `StreamingPromptRequest`, not on the
   builder, so the per-call cap is applied to the prompt request that
   `stream_chat` returns rather than to the agent.
4. In the loop over `MultiTurnStreamItem`, add three new arms.
   - `Some(Ok(MultiTurnStreamItem::StreamAssistantItem(
        StreamedAssistantContent::ToolCall { tool_call, .. })))`
     yields `EngineEvent::ToolCall(tool_call)`.
   - `Some(Ok(MultiTurnStreamItem::StreamUserItem(
        StreamedUserContent::ToolResult { tool_result, .. })))`
     yields `EngineEvent::ToolResult(tool_result)`.
   - `Some(Err(StreamingError::Prompt(boxed))) if matches!(*boxed, PromptError::MaxTurnsError { .. }) => break StopReason::ToolLimit`.
5. `ToolCallDelta`, `Reasoning`, and `ReasoningDelta` continue to fall through
   the catch-all `Some(Ok(_)) => {}` arm.

### Noop implementation

`Noop::stream` accepts the new `tools` parameter and ignores it. Existing
behavior (chunked envelope, override response, single `Final`) does not
change. Noop never emits `EngineEvent::ToolCall` or `EngineEvent::ToolResult`
in v1. Synthetic tool emission for the e2e harness is deferred and is a
follow-up TASKS entry.

### Generator

The Generator changes in three places.

1. `Generator::run` matches the new `EngineEvent::ToolCall` and
   `EngineEvent::ToolResult` variants. On each, it appends a corresponding
   `TurnMessage` to the active turn (in stream order, preserving the
   correlation id) and yields the matching `TurnEvent`.
2. The conversation's file write happens once on `EngineEvent::Final` exactly
   as today. The file then contains, in load order, every assistant text
   chunk that was finalized plus every tool call and tool result that fired
   during the loop. The next turn's `history_for(...)` therefore observes the
   full transcript and emits the grouped `Vec<rig::message::Message>` shape
   so the next request includes the prior tool round-trips.
3. The Generator must receive a tools slice from somewhere upstream. The
   wiring source (a `Generator::new` parameter, a method, a builder, or
   pulled from `Conversation`) is out of scope for this design and resolved
   alongside the not-yet-written Conversation/Workflow tool-source design.
   This design only commits to the engine accepting a slice. Where the slice
   originates is a separate decision.

### Behavior under existing deferred follow-ups

The deferred behaviors named in `TASK-NOTES-engine-deferred.md` (skip filter,
overwrite filter, isolated partition, cancellation, mid-stream
`StopReason::Error`, engine setup-error partition behavior) keep working
unchanged. Cancellation still drops in-flight streams. A turn whose
`response` already contains tool calls or tool results continues to count as
"has a response" for the overwrite filter.

### Tests to add

- `EngineEvent::ToolCall` and `EngineEvent::ToolResult` flow end-to-end
  through a fake `CompletionModel` that emits a single round-trip
  (`text -> tool_call -> tool_result -> text -> final_response`). A
  Generator wrapped around it produces `TurnEvent::ToolCall` then
  `TurnEvent::ToolResult` then `TurnEvent::Finished` in that order, and the
  written TOML file contains the corresponding `[[response]]` entries with
  matching `id`s. The fake model is in-process. Runtime under one second.
- Round-trip: load a TOML file with mixed `User`, `Assistant`, `ToolCall`,
  `ToolResult` entries, write it, and assert byte-equality. Mirrors the
  existing assistant-metadata round-trip test.
- `StopReason::ToolLimit`: a fake `CompletionModel` whose final-response
  yield is replaced with a `StreamingError::Prompt(MaxTurnsError(...))`
  ends the engine stream with `StopReason::ToolLimit`, and the Generator
  writes `stop_reason = "tool_limit"` to the file.
- Empty tool slice: both `Noop::stream` and `RigEngine::stream` accept an
  empty `tools: &[]` and produce identical output to a run on the
  pre-change signature.
- Ordering under multiple round-trips: a fake `CompletionModel` that emits
  two consecutive tool round-trips in one turn produces the engine event
  sequence `ToolCall, ToolResult, ToolCall, ToolResult` in that exact order,
  not interleaved or batched. The matching `TurnEvent`s arrive in the same
  order at the Generator boundary.
- Metadata-leakage check: a CI-level test asserts
  `grep -E "ContentMeta|ConversationTurn|Conversation|aillyrc" src/engine/`
  returns no rows after the change.

## Alternatives

### External `ToolRunner` driving the loop

The engine emits `EngineEvent::ToolCall` and stops. The Generator dispatches
the call against a registered `ToolRunner`, then re-invokes the engine with
the result appended to the history. Pros: the engine has no idea what tools
exist, the dispatcher is fully testable in isolation. Cons: we re-invent
rig's existing multi-turn loop, including provider-specific request shape and
the assistant-message bookkeeping rig already does for us. Rejected because
rig already gets this right.

### MCP-driven dispatch

The TypeScript port delegates dispatch to MCP clients via
`@modelcontextprotocol/sdk/client`. Rig has `rig::tool::rmcp` that does the
analogous job. We could declare MCP servers in `.ailly.toml`, resolve them
into rig tools at conversation load time, and rely on the same rig-managed
loop. Pros: matches the TS shape, lets users declare tools without writing
Rust. Cons: substantially larger scope and ties this slice to MCP server
lifecycle management. Deferred. The shape proposed here does not preclude
adding MCP later as one of many ways to populate the `tools` slice.

### Single `TurnEvent::ToolStep` that pairs call and result

A single fused event is simpler to consume since each event self-contains a
round-trip. The cost is losing the temporal split: a slow tool produces a
`ToolCall` event immediately and a `ToolResult` event some seconds later, and
a CLI or TUI consumer wants to render that gap. Rejected.

### `StopReason::Error("max turns reached")`

Reusing `Error` keeps the variant set small but forces every consumer to
parse strings to distinguish a tool-loop exhaustion from a transport error
or a provider 5xx. Rejected.

### Defer the file-format round-trip

A slimmer slice surfaces tool events at the `EngineEvent` and `TurnEvent`
levels only, with no `MessageFile` change in this round. Rejected because
the project has an explicit round-trip invariant: an existing turn's full
state must survive a load-write-load cycle. Skipping persistence here would
break that invariant the moment a tool-using turn is loaded a second time.

### Add `tool_calls` and `tool_results` fields to `MessageFile::Assistant`

Folds the new data into the existing assistant entry. Pros: fewer variants,
matches how the rig `AssistantContent::ToolCall` lives inside an assistant
message. Cons: the `[[response]]` array is no longer "one role per entry",
the file becomes harder to read for a human, and a long tool loop produces
one giant fused entry instead of an ordered transcript. Rejected.

### Tools registered on engine at construction

Bind the tool slice to the engine instance, e.g.
`RigEngine::new(model, tools)`. Pros: simpler `stream` signature, the engine
owns the tools for its lifetime. Cons: the engine has to be rebuilt every
time the tool set changes, which forces engine reconstruction per
conversation or per workflow step. Rejected because per-call tools matches
how rig's own builder is used and avoids tying engine identity to tool
identity.

### Tools on `Settings`

Put `tools: Vec<Arc<dyn ToolDyn>>` directly on `Settings`. Pros: the tools
ride along with the other per-call knobs. Cons: `Settings` is a small,
serialization-friendly value object holding primitives. A
provider-bound `Arc<dyn ToolDyn>` is neither serializable nor `Default`able
in any useful way, and pollutes the type. Rejected.

### Surface limit exhaustion as `TurnEvent::Failed`

Reuse the existing `Failed` variant for tool-loop exhaustion. Pros: no new
`StopReason`. Cons: `Failed` is reserved for unrecoverable errors. A
tool-loop cap is a recoverable, expected condition that the caller may have
asked for explicitly via `max_tool_turns`. Rejected. A clean `Finished` with
a new `StopReason::ToolLimit` is the right shape.

### Persist only, no `TurnEvent` change

Record tool calls and tool results into the conversation file but do not
add `TurnEvent::ToolCall` or `TurnEvent::ToolResult`. Pros: smaller surface
change, no breaking enum additions. Cons: a CLI or TUI consumer cannot show
mid-loop progress. The whole loop completes silently and the user only sees
the final assistant text. Rejected.

## Summary

Rig already runs the multi-turn tool loop. This design exposes that loop to
the rest of Ailly: per-call tool registration on `Engine::stream`, two new
`EngineEvent` variants, two new `TurnEvent` variants, two new `MessageFile`
role variants, a new `StopReason::ToolLimit`, and a new `Settings`
field `max_tool_turns`. The Generator records every tool event into the
conversation in stream order and writes the file once when the engine
finalizes. Both `Noop` and `RigEngine` ship the new signature in the same
change. The metadata-leakage invariant is preserved.

### Deferred decisions

- Where the tool slice is built. Candidates include per-conversation
  `.ailly.toml`, a future Workflow concept, and the CLI. This design only
  agrees that the engine accepts a slice, not who fills it.
- MCP dispatch via `rig::tool::rmcp`. Deferred.
- Synthetic tool calls in `Noop` for the e2e bash harness (`e2e/20_tools/`).
  Deferred until a CLI surface exists for tool registration.
- Image and multi-part `ToolResultContent` round-trip. v1 stores text only.
- Image content inside tool calls' `arguments`. v1 stores the full
  `serde_json::Value` rig provided.
- Failure semantics inside the loop, e.g. a tool whose `call` returns `Err`.
  Rig already turns this into a tool result whose content explains the
  failure, and we surface that as `EngineEvent::ToolResult` like any other.
  A future design may carry a structured `is_error` flag through the file
  format.
- Whether `Generator::new` should accept tools directly, or whether they
  should be supplied per-turn from a Conversation/Workflow source. This
  design only commits to the engine accepting a slice. The wiring source
  is resolved in another design.
