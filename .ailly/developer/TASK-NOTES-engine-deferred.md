# TASK-NOTES: Engine deferred decisions

Carried over from [2026-05-24-A-engine-provider/design.md](2026-05-24-A-engine-provider/design.md) (now removed). Each item is a decision to revisit when the named downstream consumer lands and forces the question. Until then the current shape in [src/engine/engine.rs](../../src/engine/engine.rs) stands.

## Narrow EngineError::Provider into typed variants

Resolved by [2026-05-24-B-engine-rig](2026-05-24-B-engine-rig/design.md). `EngineError` now carries `Auth`, `RateLimited`, `Timeout`, `ModelNotFound`, `MalformedResponse`, and the residual `Provider` per the design's "EngineError, narrowed" section.

## Live wire-up for OpenAI, Gemini, Bedrock

`anthropic_from_env` is the only constructor exercised against a real provider. `openai_from_env`, `gemini_from_env`, and (under the `bedrock` Cargo feature) `bedrock_from_env` ship as stubs returning `EngineError::Provider { message: "rig_engine: not yet implemented" }`. Revisit when the first e2e project that needs each provider lands; the `engine-multi-provider` task in TASKS.md is the named trigger. Bedrock additionally needs `cargo check --features bedrock` exercised in CI.

## Message.cache forwarding into the Rig request

`Message.cache == true` is preserved on the Ailly message but does not flow into the outgoing Rig request; `cache_hit` and `cache_write` are still read off the response. Revisit when Rig exposes a per-block cache marker on its generic `Message` enum, or when an Assembly knob asks for finer-grained TTL than the provider default.

## Streaming

The adapter calls `model.completion(...)` unary; `rig::streaming` is untouched. Revisit when an interactive consumer arrives or when tool loops require multi-turn streaming.

## Tool-definition wiring on requests

Assemblies currently bake tools into the prefix text per the existing assembly schema; the adapter does not forward structured tool definitions. Revisit when an Assembly knob or a Conversation field carries tool definitions, at which point Rig's `AgentBuilder::tools` + `multi_turn` path becomes the implementation reference.

## RateLimited quota vs request-rate distinction

Anthropic, OpenAI, Bedrock, and Gemini all surface 429s but mix request-per-minute throttling and organisation- or project-level quota exhaustion under different headers. The current single `RateLimited { retry_after }` variant collapses both. Revisit if the CI step needs to alert separately on quota exhaustion versus transient throttling.

## Keyed Noop table

Currently `NoopEngine` consumes scripts in call order via `VecDeque`. A future `NoopEngine::from_table(...)` constructor could route by "last user message hash" or by `meta.binding` for deterministic per-binding responses. Revisit when the first test consumer needs routing rather than ordinal consumption: the eval-judge tests or the multi-turn-skeletons tests are the likely first source. The trait does not need to change; this is a constructor addition.

## open_engine bootstrap helper

Symmetric to `open_fs_repositories` in [src/content/repository.rs](../../src/content/repository.rs). Returns `Box<dyn EngineProvider>` chosen by kind at runtime (Noop for tests, Rig for the CLI). The trait is already dyn-compatible via `#[async_trait]`, so the helper lands the moment a caller needs the runtime choice. Revisit during the `run-cmd` task when the CLI must select between Noop and Rig based on configuration or environment.

## Conversation::request_at helper

Names the `(&Conversation, usize) -> CompletionRequest<'_>` conversion so the run handler does not extract `meta.model` and `messages_up_to(idx)` by hand at every call site. Belongs on the `Conversation` aggregate per `patterns:domain-objects`. Revisit during the `run-cmd` task when the handler arrives and the call sites would otherwise duplicate the extraction.

## Tokio in main

`tokio` is currently a dev-dependency only, used by `#[tokio::test]`. The first `async` call in production code lives in the `run-cmd` slice when the handler awaits `EngineProvider::complete`. Revisit during `run-cmd`: promote `tokio` to a runtime dependency, choose between `#[tokio::main]` on `fn main` or a manual `Runtime::new()` based on whether the CLI needs a multi-thread runtime for concurrent matrix completions.
