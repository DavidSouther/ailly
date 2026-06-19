# TASK-NOTES: eval-cmd deferred decisions

Carried over from the `2026-05-24-B-eval-cmd` design doc, "Deferred decisions" section. Each item is a deliberate non-implementation: the orchestrator and CLI work as designed without it, and the trigger condition for revisiting is recorded below.

## Per-class report key for `script` and `program` assertions

The per-class rollup keys assertions by their serde tag (`script`, `program`, etc.). A regression report cannot today attribute a failure to a specific script file under those two tags; the per-match record carries the script path/contents and the suite author must read it to drill down.

**Revisit when:** a regression report needs to surface "which script failed" at the rollup level rather than in the per-match detail. The likely shape is a sub-bucket keyed on script path inside the `script` / `program` per-class entries.

## Report streaming

The orchestrator buffers the whole report in memory before writing JSON. At current scale (10k conversations × 50 cases ≈ <5 MB) this is comfortable.

**Revisit when:** a real suite exceeds the comfortable in-memory envelope, or a user reports OOM on a `runs/` directory. The replacement shape is a line-delimited intermediate written per assertion, with a final rollup pass that produces `totals` and `per_class`.

## `--report-dir` override

Report path is hard-coded to `<project>/evals/reports/<run-id>.json`. The orchestrator does not depend on the path; the CLI computes it.

**Revisit when:** the first user wants reports written elsewhere (CI artifacts directory, shared scratch, etc.). Add a `--report-dir` flag to `cli/eval.rs` and thread the resolved path through.

## Exit code for deferred-only outcome

The command exits non-zero iff `assertions_failed + assertions_malformed > 0`. A run whose every assertion is `deferred` exits zero, on the theory that "checked, not yet executable" is intentional during incremental wiring.

**Revisit when:** a CI run is silently green because every assertion deferred and the user expected that to be flagged. Likely shape: an opt-in `--strict-deferred` flag that promotes deferred to a fail-class for CI, with the default remaining permissive.

## Re-using `run-id` from `meta.assembly`

The report's `run_id` is the run directory's basename. An alternative is to read `meta.assembly` plus binding values from one of the conversations.

**Revisit when:** the directory basename stops being canonical (e.g. users start renaming directories after the fact, or running the same assembly into multiple sibling dirs that need disambiguation in the report). The conversation-side metadata is the fallback source of truth at that point.
