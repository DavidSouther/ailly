# `synthetic_matrix.jsonl`

## What this is

15 matrix.jsonl-shaped lines, one per conversation generated in this
directory, for the five `patterns-eval` judges that had zero organically-mined
calibration candidates:

- `patterns-eval/baseline/newtype`
- `patterns-eval/invocation/newtype`
- `patterns-eval/baseline/emitting-logs`
- `patterns-eval/invocation/emitting-logs`
- `patterns-eval/discovery/paired-log-handler-success`

Each line has the same fields as an entry in
`e2e/judge-calibration/mined/matrix.jsonl` (see that file and
`e2e/judge-calibration/evals/judges.yaml` in the
`e-judge-calibration-labels-miner` worktree for the reference schema/judge
registry), plus:

- `source: "synthetic-live"` instead of `"claude-code"`/`"codex"`, so the
  grading app and any future analysis can tell this purpose-generated-live
  data apart from organically-mined sessions.
- `live_judge_outcome` / `live_judge_reason` / `live_judge_transcript` --
  the real, live `ailly eval` judge verdict already captured for this
  conversation (see `judge_verdicts.yaml` and `evals/judges/` in this
  directory), so a future integration step can show the live verdict
  alongside a human's grading decision for comparison. **The grading app
  does not need to consume these fields yet** -- producing them here is
  just laying the groundwork.

`conversation_draft` (and `source_file`, `live_judge_transcript`) are paths
relative to this worktree's repo root (`e2e/judge-calibration/evals/synthetic/...`),
matching the convention of `conversation_draft` in the mined matrix.

## How it was generated

See `README.md` in this same directory for the full generation story: real
prompts, run for real through the Anthropic API via `ailly assemble` +
`ailly run`, scored for real via `ailly eval`. Nothing here is hand-written
or fabricated model output. `synthetic_matrix.jsonl` itself is a derived
index over `judge_verdicts.yaml` + `conversations/` + `prompts/synthetic/`
in this directory -- it does not introduce any new conversation content.

## Action required when merging this branch

This file is **separate from, and not yet merged into,** the real
`e2e/judge-calibration/mined/matrix.jsonl` that the grading app reads
(that file lives in the `e-judge-calibration-labels-miner` worktree, on a
different branch, and is itself gitignored there since it excerpts private
client sessions). This task deliberately did **not** write into that other
worktree's uncommitted local data.

A human integrating this branch should, as part of that integration:

1. Append the 15 lines of `synthetic_matrix.jsonl` into
   `e2e/judge-calibration/mined/matrix.jsonl` (or wherever the grading app
   ultimately reads its matrix from at merge time).
2. Confirm the grading app either ignores or meaningfully handles the new
   `source: "synthetic-live"` value and the `live_judge_*` fields (it is
   not required to consume `live_judge_*` yet -- that's a future
   integration step).
3. Decide whether `conversation_draft` paths need rewriting if the
   synthetic project's directory is relocated/copied into the mined tree
   rather than referenced in place.
