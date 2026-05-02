# Engine Module — Feature Test

## User Story

**Given** a `Conversation` loaded from a directory holding two sibling turn
files (`01.toml`, `02.toml`) and a shared `.aillyrc.toml` system message,

**When** a developer constructs a `Generator` with that conversation, a
`Noop` engine, and default `Settings`, then drives the stream returned by
`Generator::run` to completion,

**Then** for each turn, in load order, the stream emits exactly one
`Started`, one or more `Delta`s whose concatenated text equals the final
`response`, and one `Finished` carrying `StopReason::EndTurn`. The
`response` text contains the deterministic `Noop` envelope, including the
turn's path as the request label. No `Failed`, `Skipped`, or events
beyond the last `Finished` appear.

This pins the slice in the design titled *Engine Module — Design*: the
`Engine` trait's streaming contract (one `Final`, terminal), the
`Generator`'s sequence-partition behavior, and `Conversation::history_for`
as the single flattening point. It does not exercise the rig adapter,
isolated partitions, `meta.skip` / overwrite filtering, mid-stream errors,
or cancellation. Those belong to inner-loop unit tests once the plan is
written.

## Test Location

- `tests/engine/simple_noop.rs` — `generator_runs_two_turn_sequence_through_noop`.

The file is wired into the crate as an integration test target via a
`[[test]]` entry in `Cargo.toml`. It exercises only the public API
(`ailly::content::Conversation`, `ailly::engine::*`, `ailly::mem_fs!`).

## Initial Failure Mode

The test will not compile until the engine module provides
`Engine`, `Generator`, `Noop`, `Settings`, `StopReason`, and `TurnEvent`,
and `Conversation::history_for` is added in `src/content/mod.rs`. The
`futures` and `async-trait` dependencies will need to be added to
`Cargo.toml` as part of the implementation. Compile failure is the
intended starting state.

## Out of Scope for This Test

Per the design's deferred-decision list:

- Tool-use loop (`ToolCall` variant on `EngineEvent`).
- MCP, agent-client-protocol, lifecycle hooks, permissions.
- Per-provider `StopReason` / `Usage` mapping in `RigEngine`.
- Cancellation, mid-stream `StopReason::Error`, isolated scheduling.
- Persistence of write-back via `Conversation::write` after `Generator::run`.

These will be covered by inner-loop unit tests.
