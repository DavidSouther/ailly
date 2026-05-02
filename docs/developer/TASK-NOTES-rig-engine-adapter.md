# RigEngine<M> adapter

`docs/developer/2026-05-01-A-engine/design.md` specifies a `RigEngine<M: rig::completion::CompletionModel>` adapter alongside `Noop`. Step 0–3 shipped Noop only. This task implements the rig adapter so a real provider can drive the engine.

Scope from the design's "RigEngine adapter" section:

- New file `src/engine/rig_engine.rs`. Module name avoids colliding with the `rig` crate at use sites.
- Struct `RigEngine<M> { model: M, preamble: Option<String> }` with `new(model)` and `with_preamble(self, s)`.
- `impl<M> Engine for RigEngine<M>` — split `history` into `(prior_messages, last_user_message)`, build a `rig::agent::AgentBuilder<M>`, call `agent.stream_chat(last_user_text, prior_messages)`, map `StreamedAssistantContent::Text(_)` to `EngineEvent::Text`, ignore `ToolCallDelta`, `Reasoning`, and `ReasoningDelta`, and on the terminating `Final(_)` emit one `EngineResponse { text, stop_reason: StopReason::EndTurn, usage: None }`.
- One concrete constructor next to the adapter: `pub fn openai_from_env(model: &str) -> Result<RigEngine<rig::providers::openai::CompletionModel>>`.
- Re-export `RigEngine` from `src/engine/mod.rs`.
- Per-provider mapping of `StopReason` and `Usage` is explicitly deferred (Deferred Decision #4).
- Tests run against an in-process fake `CompletionModel` impl, never a network provider; total runtime stays under one second.

Reference: `docs/research/2026-05-01-A-engine/dependencies.md` for the rig-core 0.36 streaming API.

Watch-outs:

- The `Engine::stream` method is synchronous; rig's `stream_chat` returns a `StreamingPromptRequest` whose await yields the actual stream. Wrap that future inside the returned `Stream` (e.g., via `async_stream::stream!`) so the trait method stays sync.
- Last-message extraction must reject non-`User` last messages with `Err(...)` from `stream`. Validate this at the boundary, not silently inside the stream body.
