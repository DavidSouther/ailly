# Tasks

- Evaluate the `ignore` crate (https://docs.rs/ignore/latest/ignore/) as a replacement for the in-tree `GitignoreParser` in `src/content/gitignore_fs.rs`. See `TASK-NOTES-ignore-crate.md`.
- Reconsider Engine `TurnEvent` shape: switch from inline `Delta { path, text }` to embedded per-turn `ReceiverStream<String>` on `Started`. Channels are cheap; the embedded-stream variant gives consumers per-turn ownership without re-partitioning by path. Revisit once a real consumer (CLI/TUI/ACP) is built against the inline form. Decided 2026-05-01 in `docs/developer/2026-05-01-A-engine/design.md`.
- Implement the deferred `Generator` behaviors named in `docs/developer/2026-05-01-A-engine/design.md` and excluded from Step 3 of the engine plan: skip filter, overwrite filter, isolated partition, cancellation, mid-stream `StopReason::Error`, and engine setup-error partition behavior. See `TASK-NOTES-engine-deferred.md`.
- Implement the `RigEngine<M>` adapter (`src/engine/rig_engine.rs`) per `docs/developer/2026-05-01-A-engine/design.md` so a real `rig::completion::CompletionModel` can drive the engine. See `TASK-NOTES-rig-engine-adapter.md`.
