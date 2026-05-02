# Implementation Plan: Engine Module — Generator over Noop

**Feature test:** `tests/engine/simple_noop.rs` — `generator_runs_two_turn_sequence_through_noop`

**User story:** A `Generator` constructed from a two-turn `Conversation`, a `Noop` engine, and default `Settings` streams `Started → Delta(s) → Finished{EndTurn}` for each turn in load order, with `Finished.response` matching the concatenated `Delta` text and containing the deterministic Noop envelope keyed by the turn's path.

**Steps:**

- [x] Step 0: Domain types and dependencies
- [x] Step 1: `Conversation::history_for`
- [x] Step 2: `Noop` engine implementation
- [x] Step 3: `Generator::run` sequence partition

---

## Step 0: Domain types and dependencies

Lay down every new type and trait the feature test references. Bodies remain `todo!()` where behavior is deferred to later steps. The feature test file is added in this step so subsequent steps watch it move from compile error to panic to passing.

**Cargo.toml** adds two direct dependencies:

- `futures` — `Stream` trait and `Pin<Box<dyn Stream<...>>>` plumbing.
- `tokio-util` with the `rt` feature — `CancellationToken` for `Generator::cancel_token`.

`async-trait` is not added. `Engine::stream` is a synchronous stream constructor (the async work is encapsulated in the returned `Stream`), so the trait stays `dyn`-compatible without the macro.

**`src/engine/mod.rs`** declares:

- `Settings` — value object, `#[derive(Debug, Clone)]`, manual `Default`. Fields per design: `model: Option<String>`, `request_limit: usize` (default `5`), `isolated: bool`, `overwrite: bool`.
- `Usage` — `{ input_tokens: u32, output_tokens: u32 }`.
- `StopReason` — `#[non_exhaustive]` enum: `EndTurn`, `StopSequence`, `MaxTokens`, `Refusal`, `Error(String)`. The `#[non_exhaustive]` attribute reserves room for the deferred `ToolCall` work without breaking external matches.
- `EngineResponse` — `{ text: String, stop_reason: StopReason, usage: Option<Usage> }`.
- `EngineEvent` — `#[non_exhaustive]` enum: `Text(String)`, `Final(EngineResponse)`.
- `EngineStream` — `pub type EngineStream = Pin<Box<dyn Stream<Item = EngineEvent> + Send>>`.
- `Engine` — plain (non-`async-trait`) `pub trait Engine: Send + Sync { fn stream(&self, history: Vec<Message>, settings: &Settings, request_label: &str) -> anyhow::Result<EngineStream>; }`. `stream` is synchronous: it validates inputs and returns the constructed `EngineStream`; any async work runs inside the returned `Stream`.
- Submodule declarations: `pub mod generator;` and `pub mod noop;`. The feature test lives at `tests/engine/simple_noop.rs` as a `[[test]]` integration target wired in `Cargo.toml`, so it exercises only the public API.
- Re-exports: `Engine`, `EngineEvent`, `EngineResponse`, `EngineStream`, `Settings`, `StopReason`, `Usage`, `Generator`, `TurnEvent`, `SkipReason`, `Noop`.

**`src/engine/generator.rs`** declares:

- `SkipReason` — enum: `MetaSkip`, `AlreadyHasResponse`.
- `TurnEvent` — `#[non_exhaustive]` enum: `Started { path: VfsPath }`, `Delta { path: VfsPath, text: String }`, `Skipped { path: VfsPath, reason: SkipReason }`, `Finished { path: VfsPath, response: String, stop_reason: StopReason, usage: Option<Usage> }`, `Failed { path: VfsPath, error: std::sync::Arc<anyhow::Error> }`.
- `Generator` — owns `conversation: Conversation`, `engine: Arc<dyn Engine>`, `settings: Settings`, `cancel: CancellationToken`.
- `impl Generator { pub fn new(...) -> Self; pub fn cancel_token(&self) -> CancellationToken; pub fn run(self) -> Pin<Box<dyn Stream<Item = TurnEvent> + Send>> { todo!() } }`.

**`src/engine/noop.rs`** declares:

- `Noop` — `{ chunk: usize, override_response: Option<String> }`. `Default` sets `chunk = 32` and reads `AILLY_NOOP_RESPONSE` from the environment for `override_response`.
- `impl Engine for Noop { fn stream(...) -> Result<EngineStream> { todo!() } }` — plain `impl`, no macro.

**`tests/engine/simple_noop.rs`** holds `#[tokio::test] async fn generator_runs_two_turn_sequence_through_noop()` exactly per the spec in `feature-test.md`. The test fixture builds a `mem_fs!`-backed conversation with a `.aillyrc.toml` plus `01.toml` and `02.toml` siblings, constructs a `Generator` from a `Noop` engine and `Settings::default()`, drives `Generator::run().collect::<Vec<_>>()`, and asserts the event sequence shape.

**Enables:** every type and trait referenced by the feature test now exists. `cargo check` and existing unit tests still pass. The feature test compiles and fails at runtime via `todo!()` inside `Generator::run`.

---

## Step 1: `Conversation::history_for`

Add `pub fn history_for(&self, turn: &ConversationTurn) -> Vec<Message>` to `impl Conversation` in `src/content/mod.rs`. The function applies the four rules from the design's *Conversation::history_for* section: push `turn.system` unless `meta.skip_head`, walk `self.predecessor(...)` repeatedly to collect predecessors in reverse and push each one's `prompt + response`, push `turn.prompt`, drop the trailing `Assistant` message unless `meta.continue`. No metadata other than the four `ContentMeta` fields the design names is read.

Unit tests live in the existing `tests` module of `src/content/mod.rs` and use the existing `mem_fs!` fixtures. Cases:

- Single turn, no predecessors, `skip_head=false`, `continue=false`. Result is `[system..., prompt...]`.
- Two turns in load order, second turn has no isolation. Result is `[system..., t0.prompt..., t0.response..., t1.prompt...]`.
- `skip_head=true` on the queried turn drops the inherited system chain; predecessor messages remain.
- `isolated=true` on the queried turn drops every predecessor; system chain remains.
- Trailing `Assistant` content with `continue=false` is popped.
- Trailing `Assistant` content with `continue=true` is preserved.

**Enables:** the path inside `Generator::run` from a `ConversationTurn` to a `Vec<Message>` passed to `engine.stream`. The feature test still fails because `Generator::run` and `Noop::stream` are still `todo!()`.

---

## Step 2: `Noop` engine implementation

Fill in `impl Engine for Noop`. Assemble the deterministic envelope:

```
noop response for {request_label}:
[history follows]
[message 0] {role}: {text}
[message 1] {role}: {text}
...
[response] {text of the last user message in history}
```

Yield the assembled text in `chunk`-byte slices as `EngineEvent::Text(_)`, then exactly one `EngineEvent::Final(EngineResponse { text, stop_reason: StopReason::EndTurn, usage: None })`. The body of `Noop::stream` is fully synchronous: it builds a `Vec<EngineEvent>` and returns `Ok(Box::pin(futures::stream::iter(events)))`. There is no `tokio::time::sleep` and no `await` inside the trait method. When `override_response` is `Some`, emit the override as one or more `Text` chunks plus the same `Final`.

Unit tests in `src/engine/noop.rs`:

- Empty history, label `"alpha"`: collected events are N `Text` chunks whose concatenation equals the envelope, followed by one `Final{EndTurn, usage: None}`.
- `override_response = Some("hi")`: stream is `[Text("hi"), Final{EndTurn}]`.
- Two runs over identical input produce byte-equal `Text` payloads in order (deterministic-noop metric).

**Enables:** when the Generator finally drives `engine.stream`, the Noop produces real chunks and a terminal `Final`. The feature test still fails because `Generator::run` is `todo!()`.

---

## Step 3: `Generator::run` sequence partition

Replace the `todo!()` in `Generator::run` with the happy-path sequence partition. Out of scope for this step and explicitly carried by the design's deferred-decision list: skip filter, overwrite filter, isolated mode, cancellation, error mapping, mid-stream `StopReason::Error`. Each is a separate inner-loop unit test once the feature test is green.

Take `mut self` so the generator owns the `Conversation` and can write each turn's response back into the in-memory turn before iterating to the next one. Build the output `Stream` with `async_stream::try_stream!` (or `async_stream::stream!`) for readability — both are zero-cost wrappers over `futures` and keep the body imperative. Add `async-stream` to `Cargo.toml` if it is not already pulled in by `rig-core`.

Behavior:

1. Iterate `self.conversation.turns` in load order. (For the feature test, this is one partition with two turns; partitioning by `parent` directory is implemented but does not branch behavior in this step because every test fixture sits in one directory.)
2. For each turn:
   - Emit `TurnEvent::Started { path: turn.path.clone() }`.
   - `let history = self.conversation.history_for(turn);`
   - `let mut events = self.engine.stream(history, &self.settings, turn.path.as_str())?;` — note the call is synchronous; `?` lifts setup errors out of the trait method directly.
   - Loop on `events.next().await`. For `EngineEvent::Text(t)` emit `TurnEvent::Delta { path: turn.path.clone(), text: t }`. For `EngineEvent::Final(r)`: write `r.text` back into the in-memory turn (push as an `Assistant` message onto `turn.response`), emit `TurnEvent::Finished { path: turn.path.clone(), response: r.text, stop_reason: r.stop_reason, usage: r.usage }`, break out of the inner loop.
3. End the outer stream after the last turn's `Finished` event.

The write-back step is what lets the second turn in the feature test see the first turn's response when `history_for` walks predecessors. Without it, the second turn's Noop envelope would not include the first turn's response and the feature test would still fail on the second `Finished` payload.

Unit tests in `src/engine/generator.rs`:

- Single-turn conversation through `Noop`: emitted sequence is `[Started, Delta+, Finished{EndTurn}]`; concatenated `Delta.text` equals `Finished.response`; `Finished.response` contains the literal `noop response for <path>` prefix.
- Two-turn sequence in one partition: events appear `[Started(t0), Delta+(t0), Finished(t0), Started(t1), Delta+(t1), Finished(t1)]`; the second turn's `Finished.response` envelope contains the first turn's response text (proving write-back wired into `history_for`).

**Enables:** the feature test passes. `generator_runs_two_turn_sequence_through_noop` sees the full event sequence with deterministic Noop envelopes containing each turn's path as the request label, and no `Failed`, `Skipped`, or trailing events.
