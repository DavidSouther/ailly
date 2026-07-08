# Judge Calibration

Measures how well `ailly_two`'s `judge` assertion (`src/knowledge/assertions.rs::check_judge`)
agrees with a human labeler, by folding a real `EvalReport` against a
human-authored label map (`src/knowledge/calibration.rs::compute_calibration`).

## Pipeline

Four scripts under `evals/scripts/`, run in order:

1. **`discover_judges.py`** — finds every real `type: judge` assertion under
   `e2e/*/evals/*.yaml` and writes the judge registry to `evals/judges.yaml`
   with topic keywords derived from its `prompt:` rubric. A judge with no
   extractable keyword is flagged `needs_human_review: true` and skipped by
   automated matching.
2. **`mine_calibration_candidates.py`** — mines candidate `(question,
   candidate response)` examples out of the operator's past agent session
   transcripts, wherever those sessions actually invoked Ailly. Tags each
   candidate with the invoked skills and references.
3. **`build_relevance_matrix.py`** — matches judges to candidates
   by tag overlap (`skill_signals.tags_match`), then re-opens each matched
   candidate's raw transcript to extract a *narrow* excerpt (the message pair
   nearest where the match actually occurred, not the whole broad turn).
   Writes `mined/matrix.jsonl` and one excerpt file per cell under
   `mined/matrix/`.
4. **`skill_signals.py`** — not a pipeline step; shared regex-based
   skill/reference-identifier detection used identically by steps 1, 2, and 3
   so a change to "what counts as mentioning skill X" stays consistent
   everywhere it's checked.

## Confidentiality — `mined/` is never committed

Session transcripts span every client, employer, and personal project the
operator has used Ailly in. Mined output necessarily contains verbatim
excerpts from all of them. **`e2e/judge-calibration/mined/` is listed in
`.gitignore` and must never be committed.** Only the scripts themselves (and
the human-reviewed, explicitly-promoted `evals/labels.yaml` ground truth) are
checked in.

## Ground truth and running calibration

`evals/labels.yaml` holds the actual human Pass/Fail verdicts, keyed by
`<judge_id, '/' -> '__'>__<candidate_id>` — written directly by the grader
app the moment a human grades a cell, no separate export step. Feed it and a
real `EvalReport` (from running the judge-calibration eval suite) to
`compute_calibration` (`src/knowledge/calibration.rs`) to get an
`agreement_rate` and `meets_bar` verdict.
