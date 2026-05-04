# Feature test: tool call handling in the engine

## User story

A developer drives `Generator` over a one-turn `Conversation` and registers a single `echo` tool. The fake `CompletionModel` underneath `RigEngine` runs one tool round-trip in two calls.

- **Given** a `Conversation` whose only turn has the prompt
  `"echo ok please"`, an `echo` tool that returns its `text` argument, and a
  fake `CompletionModel` that streams text, then a tool call against `echo`,
  then a `FinalResponse` on the first call, and text plus a `FinalResponse` on
  the second call.
- **When** the developer wires the tool into `Generator` and consumes the
  `TurnEvent` stream, with `Settings::default()` covering the default
  `max_tool_turns`.
- **Then** the stream contains, in order, `Started`, one or more `Delta`,
  `ToolCall { call }` whose `call.function.name == "echo"` and `call.id ==
  "call_1"`, `ToolResult { result }` whose `result.id == "call_1"`, one or
  more `Delta`, and finally `Finished` with `StopReason::EndTurn`.
- **And** the per-turn TOML file at `root/01.toml` retains `prompt = "echo ok
  please"` and gains `[[response]]` entries in this order: assistant, then
  `tool_call` carrying `name = "echo"` and `id = "call_1"`, then `tool_result`
  carrying `id = "call_1"`, then a second assistant.

## Scope

This single test exercises the user-observable surface of the slice end-to-end:

- The new tools parameter on `Engine::stream`, threaded through `Generator`
  via a `with_tools` builder.
- `EngineEvent::ToolCall` and `EngineEvent::ToolResult` produced by `RigEngine`
  out of rig's `MultiTurnStreamItem` arms.
- `TurnEvent::ToolCall` and `TurnEvent::ToolResult` forwarded by the
  `Generator`.
- `MessageFile::ToolCall` and `MessageFile::ToolResult` written to the per-turn
  TOML file in stream order with matching `id`s, alongside the assistant
  text entries that bracket the round-trip.
- Default `max_tool_turns` budget is large enough that a single round-trip
  finishes with `StopReason::EndTurn` rather than `ToolLimit`.

The narrow tests called out in `design.md` (round-trip byte-equality,
`StopReason::ToolLimit`, empty tool slice parity, multi-round-trip ordering,
metadata-leakage `grep`) are deferred to the planning step. They exercise
adjacent guarantees that this end-to-end test does not cover.

## Test artifact

`tests/engine/tool_round_trip.rs` — wired in `Cargo.toml` as the
`engine_tool_round_trip` integration test target.

The test owns its own fake `CompletionModel` and `EchoTool` impl so the
fixture stays in one file. The fake model uses an `Arc<Mutex<usize>>`
counter to distinguish rig's first stream call (text + tool call + final)
from the second (text + final).

## Decisions encoded by this test

- **Generator wiring.** The test calls
  `Generator::new(conversation, engine, settings).with_tools(vec![echo])`.
  The design lists this wiring as deferred for the production source-of-truth
  decision (`.ailly.toml` vs Workflow vs CLI). The test commits to one
  in-process shape so the slice has an observable end-to-end path. The
  implementer is free to rename the builder method as long as some
  Generator-level entry point exists.
- **Tool ergonomic.** The test treats `Arc<dyn rig::tool::ToolDyn>` as the
  tool handle type at the call site. This matches the design's
  `tools: &[Arc<dyn rig::tool::ToolDyn>]` engine signature.
- **Stop reason.** A single round-trip with default `max_tool_turns` ends in
  `StopReason::EndTurn`, not `ToolLimit`.
- **File order.** The file write happens once on `Final` and contains, in
  load order: the user prompt, the first assistant text, the tool call, the
  tool result, the second assistant text. No fused-entry shape, no
  out-of-order reordering.

## How this test fails today

`Generator::with_tools`, the new `Engine::stream` tools parameter, the new
`EngineEvent` variants, the new `TurnEvent` variants, and the new
`MessageFile` variants do not yet exist. The test will not compile against
the current tree.
