# Engine Module — Design

## Problem Statement

`ailly_rust` has a working `Conversation` model (`src/content/mod.rs`) that loads `.toml` turn files into `ConversationTurn { system, prompt, response }` using `rig::message::Message` directly. It has no way to actually run an LLM. The TypeScript port (`ailly_typescript/core/src/{engine,actions}/`) does this in two layers: a pluggable `Engine` interface with multiple implementations (OpenAI, Bedrock, Mistral, noop) and an actions layer (`GenerateManager`, `PromptThread`) that partitions turns, schedules concurrent runs, and manages MCP / tools.

We need the equivalent in Rust, scoped to a first slice that excludes MCP, agent-client-protocol, the tool-use loop, RAG/plugins, lifecycle hooks, and permissions. Those will each be designed and added later. This design covers two concerns at once:

1. An `Engine` trait that takes a flat chat history and returns a stream of generation events.
2. A `Generator` that walks a `Conversation`, decides which turns to run, and drives them through an `Engine`.

The work targets `rig-core 0.36.0` (already pinned). A `Noop` engine is required for unit tests so the generator can be exercised without network access.

## Prior Art

| Source | Lesson taken |
|---|---|
| `ailly_typescript/core/src/engine/index.ts` | Streaming-first interface (`{ stream, message(), debug(), done }`). Multiple backends behind one shape. Take the streaming-first idea, drop `format` from the trait. |
| `ailly_typescript/core/src/engine/openai.ts` (`getMessages`, `format`) | Each backend re-walked `content.context.predecessor` to build messages. **Reject this**: a Conversation→Message flattener belongs on `Conversation`, not on every backend. (See user feedback memory `feedback_engine_does_not_walk_content`.) |
| `ailly_typescript/core/src/actions/generate_manager.ts` | `partitionPrompts` groups by `dirname`. Take this. |
| `ailly_typescript/core/src/actions/prompt_thread.ts` | `runIsolated` vs `runSequence`; `scheduler` with `request_limit`. Take both. The MCPClient/tool-loop interleaving is deferred. |
| `ailly_typescript/core/src/engine/noop.ts` | Deterministic envelope of system + messages + prompt, streamed in chunks. Take the deterministic-envelope idea; drop the configurable sleep and chunk-size knobs. |
| `rig-core 0.36` | `rig::completion::CompletionModel` (`stream` is built into the trait), `rig::agent::AgentBuilder` (`stream_chat(prompt, history)`), `StreamingCompletionResponse<R>` yielding `StreamedAssistantContent<R>`. We adapt to rig at the bottom of our stack, not at the top. See `docs/research/2026-05-01-A-engine/dependencies.md`. |

## Metrics

This is internal infrastructure. Operational metrics are not yet meaningful. The following are the design metrics we will hold this work to:

- **Engine independence.** A new `Engine` implementation (rig provider, or a custom `CompletionModel`) is added by writing one new file under `src/engine/`, with no edits to `Conversation`, `Generator`, or `TurnEvent`.
- **No metadata leakage into engines.** No `Engine` implementation imports `ContentMeta`, `ConversationTurn`, `Conversation`, or `.aillyrc.toml` parsing. Verified by `grep`.
- **Deterministic noop.** A `Noop` run on a fixed `Conversation` yields the same byte stream and the same final response under repeated runs.
- **Test isolation.** All `engine` and `generator` unit tests run with no network and complete under one second total.
- **Cancellation.** Dropping the `Generator` stream stops further turns within one in-flight turn boundary.

## Specification

### Module layout

```
src/engine/
├── mod.rs          # Engine trait, Settings, EngineEvent, EngineResponse, EngineStream, public re-exports
├── rig_engine.rs   # RigEngine<M: rig::completion::CompletionModel> adapter
├── noop.rs         # Noop engine
└── generator.rs    # Generator, TurnEvent, partition, run loop
```

The file is named `rig_engine.rs` rather than `rig.rs` to avoid module-name collision with the `rig` crate at use sites.

`Conversation::history_for` is added to `src/content/mod.rs`. Nothing else in `content/` changes.

### New dependencies

This slice introduces three new direct dependencies (Cargo.toml is not edited by this design; the implementation step adds them):

- `futures` — for the `Stream` trait and `Pin<Box<dyn Stream<...>>>` plumbing on `EngineStream` and `Generator::run`.
- `tokio-util` (with the `rt` feature) — for `CancellationToken`, the cancellation primitive shared between `Generator` and in-flight engine calls.
- `async-trait` — required because `Engine::stream` is an async method on a `dyn`-friendly trait.

### Engine trait

```rust
// src/engine/mod.rs
use rig::message::Message;
use std::pin::Pin;
use futures::Stream;

pub struct Settings {
    pub model: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub request_limit: usize,   // default 5
    pub isolated: bool,         // default false; OR'd with per-turn meta.isolated per partition
    pub overwrite: bool,        // default false
}

pub enum EngineEvent {
    Text(String),
    Final(EngineResponse),
}

pub struct EngineResponse {
    pub text: String,           // assembled text content
    pub stop_reason: StopReason,
    pub usage: Option<Usage>,
}

pub enum StopReason { EndTurn, StopSequence, MaxTokens, Refusal, Error(String) }

pub struct Usage { pub input_tokens: u32, pub output_tokens: u32 }

pub type EngineStream = Pin<Box<dyn Stream<Item = EngineEvent> + Send>>;

#[async_trait::async_trait]
pub trait Engine: Send + Sync {
    async fn stream(
        &self,
        history: Vec<Message>,
        settings: &Settings,
        request_label: &str,
    ) -> Result<EngineStream>;
}
```

Throughout this section, `Result<T>` means `anyhow::Result<T>`.

Setup errors (auth, malformed history, etc.) come back from `stream` as `Err(anyhow::Error)`. Mid-stream errors come back as a `Final` carrying `StopReason::Error(_)`. On a mid-stream error, `Final.text` carries whatever assistant text was assembled so far, `usage` is `None`, and `stop_reason` is `Error(message)`. Every `EngineEvent::Final` is the last event; the stream ends after it. Generator treats the first `Final` as terminal; any events after `Final` are ignored. Engine implementations must emit exactly one `Final`. Consumers always see exactly one `Final` per successful `stream` call.

`request_label` is a per-turn debug label (the generator passes the turn's path). Engine implementations may surface it in logs but must not depend on its content.

`EngineEvent::ToolCall` is intentionally absent in this slice. When tool-use is added, a new variant is appended; existing matches will either compile (non-exhaustive) or be updated then.

### `Conversation::history_for`

Added to `src/content/mod.rs`:

```rust
impl Conversation {
    /// Build the chat history that should be sent to the engine for `turn`.
    ///
    /// Order: system chain (unless `meta.skip_head`), predecessor turns'
    /// `prompt + response` in load order (unless `meta.isolated` on the turn),
    /// then `turn.prompt`. Trailing assistant message is dropped when the
    /// turn does not have `continue == true`.
    pub fn history_for(&self, turn: &ConversationTurn) -> Vec<Message>;
}
```

Rules:

1. If `turn.meta.skip_head == false`, push every message in `turn.system` first. `turn.system` already represents the inherited system chain at the turn's location (built by `AillyRc::load`); `history_for` does not re-synthesize the chain from predecessors' `system` fields.
2. If `turn.meta.isolated == false`, walk predecessors by calling `self.predecessor(t)` repeatedly: start from `turn`, call `predecessor` to get its prior, then call `predecessor` on that prior, and so on until `predecessor` returns `None`. Collect the visited predecessors into a `Vec`, reverse it to load order (oldest first), then for each predecessor push every message in `prompt` followed by every message in `response`.
3. Push every message in `turn.prompt`.
4. If the last message is an `Assistant` message and `turn.meta.continue == false`, pop it.

This mirrors `getMessagesPredecessor` from the TS engine but with no metadata fields leaking past `Conversation`. The rules are testable in isolation against a constructed `Conversation`.

### Generator

```rust
// src/engine/generator.rs
use crate::content::{Conversation, ConversationTurn};
use crate::engine::{Engine, Settings};
use vfs::VfsPath;
use tokio_util::sync::CancellationToken;

pub enum TurnEvent {
    Started  { path: VfsPath },
    Delta    { path: VfsPath, text: String },
    Skipped  { path: VfsPath, reason: SkipReason },
    Finished { path: VfsPath, response: String, stop_reason: StopReason, usage: Option<Usage> },
    Failed   { path: VfsPath, error: Arc<anyhow::Error> },
}

pub enum SkipReason { MetaSkip, AlreadyHasResponse }

pub struct Generator {
    conversation: Conversation,
    engine: Arc<dyn Engine>,
    settings: Settings,
    cancel: CancellationToken,
}

impl Generator {
    pub fn new(conversation: Conversation, engine: Arc<dyn Engine>, settings: Settings) -> Self;
    pub fn cancel_token(&self) -> CancellationToken;
    pub fn run(self) -> Pin<Box<dyn Stream<Item = TurnEvent> + Send>>;
}
```

Behavior of `run`:

1. **Partition.** Group turns by parent directory (`dirname` of the turn's path). Order partitions by load order of their first turn.
2. **Mode.** A partition is *isolated* iff `settings.isolated == true` OR every turn in the partition has `meta.isolated == true`. Otherwise *sequence*.
3. **Filter (per turn, before invoking the engine).** Emit `Skipped { reason: MetaSkip }` when `turn.meta.skip == true`. Emit `Skipped { reason: AlreadyHasResponse }` when `!turn.response.is_empty() && !settings.overwrite && !turn.meta.continue`. Skipped turns still contribute their stored response to subsequent predecessor history. If a turn is skipped and has an empty `response`, it contributes nothing to subsequent history; no error.
4. **Sequence partition.** Run turns one at a time. For each turn: emit `Started`; ask `Conversation::history_for(turn)`; call `engine.stream(history, &settings, request_label)` where `request_label` is the turn's path as a string; for each `EngineEvent::Text(t)` emit `Delta { path, text: t }`; on `EngineEvent::Final(r)` emit `Finished { path, response: r.text, ... }`.
5. **Write-back.** On `EngineEvent::Final(r)`, write `r.text` back into the in-memory turn. This happens in BOTH sequence and isolated modes. In sequence mode, subsequent predecessor walks observe the new response. In isolated mode, sibling turns do not observe it because their `history_for` skips predecessors anyway; the write-back is still performed so that a subsequent `Conversation::write` persists the new responses.
6. **Isolated partition.** Schedule turns with a counting semaphore of size `settings.request_limit`; otherwise per-turn behavior is identical to sequence (each turn's `history_for` is independent of its siblings because `meta.isolated` skips predecessors).
7. **Cancellation.** Each turn checks `cancel.is_cancelled()` before requesting history and before reading from the engine stream. In-flight engine streams are dropped on cancel; rig propagates that to the underlying HTTP call.
8. **Errors.** Any error from `engine.stream` or from the per-event loop produces `Failed { path, error: Arc::new(e) }`. A failure in *sequence* mode aborts that partition; remaining partitions still run. A failure in *isolated* mode does not affect other turns in the same partition. After `Failed` fires for a turn, no further events fire for that turn's path.

### Noop

```rust
// src/engine/noop.rs
pub struct Noop {
    pub chunk: usize,                   // default 32
    pub override_response: Option<String>, // takes AILLY_NOOP_RESPONSE if unset and env present
}
```

`Noop::stream` builds:

```
noop response for {request_label}:
[history follows]
[message 0] system: …
[message 1] user: …
…
[response] {last user message text}
```

`{request_label}` is the `request_label: &str` argument the generator passes to `Engine::stream` (the turn's path as a string). Generator-side tests assert the deterministic shape including this label.

Streaming: the assembled text is yielded in `chunk`-byte slices as `EngineEvent::Text(_)`, then a `EngineEvent::Final` with `stop_reason = EndTurn` and `usage = None`. No sleeps. If `AILLY_NOOP_RESPONSE` is set, that string is returned wholesale instead of the envelope.

### RigEngine adapter

```rust
// src/engine/rig_engine.rs
pub struct RigEngine<M: rig::completion::CompletionModel> {
    model: M,
    preamble: Option<String>,
}

impl<M> RigEngine<M> { pub fn new(model: M) -> Self; pub fn with_preamble(self, s: impl Into<String>) -> Self; }
```

`Engine::stream` for `RigEngine<M>`:

1. Split `history` into `(prior_messages, last_user_message)`. Last message must be `User`; otherwise `Err`. Extract its text content as `last_user_text`. The slice before it (`prior_messages`) forms rig's "chat history" argument.
2. Build a `rig::agent::Agent` via `AgentBuilder`:
   ```rust
   let mut builder = rig::agent::AgentBuilder::new(self.model.clone());
   if let Some(p) = &self.preamble {
       builder = builder.preamble(p);
   }
   let agent = builder.build();
   ```
3. Call `agent.stream_chat(last_user_text, prior_messages)`. Per `dependencies.md`, this returns a `StreamingPromptRequest` which yields items of type `StreamedAssistantContent<R>` where `R = M::StreamingResponse`. Map each `StreamedAssistantContent::Text(t)` to `EngineEvent::Text(t)`; ignore `ToolCallDelta`, `Reasoning(_)`, and `ReasoningDelta` for now.
4. On the terminating `StreamedAssistantContent::Final(_r)`, build an `EngineResponse` from the assembled text. The terminating `Final(R)` carries the provider-specific `R`; rig does not expose a unified stop-reason or usage at that level. For this slice, every successful stream termination maps to `StopReason::EndTurn` with `usage = None`. Per-provider mapping of `R` into `StopReason` and `Usage` is deferred. Emit a single `EngineEvent::Final(EngineResponse { text, stop_reason: StopReason::EndTurn, usage: None })`.
5. `request_label` is currently ignored by `RigEngine` (the implementation may pass it to `tracing::debug!` for diagnostics; no behavior depends on it).

Concrete constructors live next to the adapter, e.g.:

```rust
pub fn openai_from_env(model: &str) -> Result<RigEngine<rig::providers::openai::CompletionModel>>;
```

The CLI wires one of these directly. No factory function in this slice.

### Public API

`src/engine/mod.rs` re-exports: `Engine`, `Settings`, `EngineEvent`, `EngineResponse`, `EngineStream`, `StopReason`, `Usage`, `Generator`, `TurnEvent`, `SkipReason`, `Noop`, `RigEngine`. Engine-implementation-specific constructors stay in their respective files.

### Testing strategy

- **Unit (engine):** `Noop::stream` over a fixed history produces a known-byte stream and a `Final` event. `RigEngine` is exercised against an in-process fake `CompletionModel` impl in a test file, not against any external provider.
- **Unit (generator):** Build a `Conversation` from `mem_fs!` fixtures (matching `content::test_util`). Drive `Generator::run` with `Noop`. Assert event sequences for: sequence partition, isolated partition, mixed `meta.skip`, `overwrite=false` skip, mid-partition failure, cancellation between turns.
- **Unit (history_for):** Construct `Conversation` with predecessors and assert exact `Vec<Message>` shape under each combination of `skip_head`, `isolated`, `continue`, and trailing assistant message.

Total runtime under one second; no network; no `tokio::time::sleep`.

## Alternatives

**Lift rig's `CompletionModel` directly as the public Engine surface.** Rejected. Implementing `CompletionModel` for a noop requires constructing rig-internal response shapes (`CompletionResponse`, `OneOrMany<AssistantContent>`, a `Response` associated type). Our shape is also a moving target — a non-rig Engine implementation (a stub that reads from a file, an MCP-only "engine", a future ACP server) should not have to satisfy `CompletionModel`. The thin `Engine` trait is one method; the cost of carrying it is small.

**Engine trait takes `&Conversation` and `&ConversationTurn`.** Rejected; explicitly contradicted by user feedback (`feedback_engine_does_not_walk_content`). An Engine implementation that walks Content metadata is exactly the TS pattern we are escaping.

**`PromptThread` analogue exposed publicly.** Considered; rejected for this slice. The TS split paid off only because `PromptThread` owned an MCPClient and an EventEmitter, both of which are deferred. Re-evaluate when MCP lands.

**Per-turn `ReceiverStream<String>` on `TurnEvent::Started`.** Deferred — see Deferred Decision #8.

**Typed `thiserror` errors instead of `anyhow`.** Deferred — see Deferred Decision #7.

**`Engine::format` method (TS shape).** Rejected for the reasons above and in the metrics ("No metadata leakage into engines").

## Summary

The engine module pairs an `Engine` trait (flat history in, stream of `EngineEvent`s out, exactly one `Final`) with a `Generator` that walks a `Conversation`, partitions by directory, runs sequence or isolated, and emits `TurnEvent`s. `Conversation::history_for` is the single place where Content metadata is read on the way to an engine call. `Noop` enables network-free tests; `RigEngine<M>` is a thin adapter over `rig::completion::CompletionModel`.

### Deferred decisions / open questions

1. **Tool-use loop.** Engine has no `ToolCall` variant yet. Adding it will append a variant and extend `Generator::run` with a tool-result re-feed step.
2. **MCP / rmcp.** Belongs on `Generator` (it owns the lifecycle), not on `Engine`. Future design.
3. **agent-client-protocol.** Will be a *consumer* of the `Stream<TurnEvent>`, not a sibling of the engine. Future design.
4. **Per-provider StopReason / Usage mapping for `RigEngine`.** This slice maps every successful stream termination to `StopReason::EndTurn` with `usage = None`. Per-provider extraction (reading the provider-specific `R` from `StreamedAssistantContent::Final(R)` and mapping its stop-reason and token usage) is deferred until a second concrete provider lands.
5. **Lifecycle hooks and permissions.** ARCHITECTURE.md calls for them on the engine. They belong on the generator (it brackets the call). Future design.
6. **`Settings` split.** When a second concrete consumer appears, split into `EngineSettings` and `GeneratorSettings`.
7. **`anyhow` vs `thiserror`.** Task filed for re-evaluation.
8. **`TurnEvent` channel form.** Task filed for re-evaluation against per-turn `ReceiverStream`.

The next step after this design clears the draft gate is to write the feature test that pins down `Generator::run` event sequences against `Noop` for one sequence partition and one isolated partition.
