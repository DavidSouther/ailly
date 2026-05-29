# Developer Tasks

<!-- Tasks left here by developer sessions. Remove a task when you start it. -->
<!-- Ignore # comment lines and HTML section comments. -->

## ailly report — out-of-scope follow-ups

The dual-mode `report` command (commits `bc6bb42`, `999ca85`) supersedes the older quadrant/verdict design. The two feature tests in `tests/report_cmd.rs` cover the happy path for single mode and comparison mode; the items below are not yet exercised:

- Three-or-more-run mode: more than two run IDs is rejected today (`expected 1 or 2 run IDs`). If a strategy is needed, decide whether to keep oldest as `arm_a` / newest as `arm_b` and ignore intermediates, or to render N-column tables.
- `deferred` / `malformed` footnote rendering in `render_single_markdown` and `render_comparison_markdown` (currently the cell shows `defer` / `err` inline; no footnote).
- `--label-a` / `--label-b` override flags on `ReportCmdArgs` (struct fields exist, no CLI plumbing in `main.rs`). Today both default to `None` and the markdown extracts the label from the trailing `-`-segment of the run_id.
- `assemblies/invocation.yaml` prefix fix — pre-existing issue noted during this session, not touched by the report redesign.

## eval-judge — deferred decisions

`Assertion::Judge` is wired (topic `2026-05-28-A-eval-judge`). Ten trigger-gated follow-ups (per-conversation engine dispatch, forced tool-call verdict, judge-model override, cost accounting, `text_semantic_match` runtime, refusal/position-bias handling, report linkage, orphan/collision policy) are recorded in [TASK-NOTES-eval-judge-deferred.md](TASK-NOTES-eval-judge-deferred.md). Each waits on its own trigger; none is active work.

