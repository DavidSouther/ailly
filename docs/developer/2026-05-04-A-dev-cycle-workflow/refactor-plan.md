# Refactor Plan: slice 1 (inputs and templates)

Smells found in the files touched by Steps 0-3 of `plan-1-inputs-templates.md`.

- [x] **Three-Strikes / Inconsistent Names** `src/workflow/runtime.rs:154-168` The unresolved-template arm constructs `WorkflowStopReason::TaskFailed { task, error: Arc::new(anyhow::Error::new(...)) }` inline while every other failure path in this loop goes through the `task_failed(task_name, error)` helper. Route through `task_failed` so the construction is consistent and the inline `Arc::new(anyhow::Error::new(...))` ceremony disappears.

No further smells worth addressing this pass:

- `validate_inputs` malformed-pattern path uses a sentinel string `"<pattern compile error>"`. The plan calls this out explicitly as the chosen behavior; a third error variant was rejected upstream.
- `build_template_context` rebuilds the context per dispatch. Idempotent and cheap; caching it on `WorkflowState` would tangle dispatch with persistence.
- `resolve_context_seed` hardcodes the `"topic"` input name. The design names this contract; promoting it to a constant would only obscure the wiring.
