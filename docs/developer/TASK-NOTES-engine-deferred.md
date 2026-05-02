# Engine slice — deferred behaviors

Step 3 of `docs/developer/2026-05-01-A-engine/plan.md` shipped the happy path for `Generator::run` (sequence partition over Noop). The design's "Behavior of `run`" section names six behaviors that the slice intentionally left for follow-up. Each is a small, isolatable inner-loop unit test.

Out-of-scope behaviors to add, each with its own test:

- **Skip filter** — emit `TurnEvent::Skipped { reason: MetaSkip }` when `turn.meta.skip == true`. Predecessor history must still see the turn's stored response.
- **Overwrite filter** — emit `TurnEvent::Skipped { reason: AlreadyHasResponse }` when `!turn.response.is_empty() && !settings.overwrite && !turn.meta.continue`.
- **Isolated partition** — group by parent directory, mark a partition isolated when `settings.isolated == true` OR every turn has `meta.isolated == true`. Schedule isolated turns concurrently with a `tokio::sync::Semaphore` of size `settings.request_limit`.
- **Cancellation** — `cancel.is_cancelled()` checked before requesting history and before each engine event. In-flight `Stream`s must drop on cancel.
- **Mid-stream errors** — `EngineEvent::Final` with `StopReason::Error(_)` lands as `TurnEvent::Failed`. After `Failed`, no further events for that path.
- **Engine setup errors** — `engine.stream(...)` returning `Err(_)` already maps to `Failed`; verify partition behavior on failure (sequence aborts the partition; isolated does not).

The first three add new `Generator::run` branches; the last three exercise existing branches. Existing tests in `src/engine/generator.rs::tests` already cover the happy paths. Each new behavior gets one focused test.

Order of work: skip filter → overwrite filter → mid-stream error → engine setup error in sequence partition → isolated partition → cancellation. The cancellation test should use `tokio::time::timeout` with a small deadline so a stuck stream fails fast.

Touch points: `src/engine/generator.rs` for the run loop, plus possibly `src/content/mod.rs` for any extra accessors needed (e.g., `ConversationTurn::has_response()`, `ConversationTurn::meta()` exposing only the fields the generator needs — keep the metadata-leakage metric green by never letting `ContentMeta` cross into `noop.rs` or any future engine).
