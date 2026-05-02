# Refactor Plan — RigEngine adapter and CLI wiring (2026-05-02)

Scope: files committed in `b290552` (engine: wire RigEngine adapter and CLI runtime against Anthropic).

## Smells

- [x] ~~**Three-Strikes Refactor** `src/engine/rig_engine.rs:66-106` — `EngineEvent::Final(EngineResponse { text, stop_reason, usage: None })` is emitted at three sites (FinalResponse arm, Err arm, post-loop fallback) using a manual `emitted_final` flag and `break`/fallback to keep them in sync. Collapse into a single yield by computing the `StopReason` inside a `loop` and yielding the Final once after the loop ends.~~
