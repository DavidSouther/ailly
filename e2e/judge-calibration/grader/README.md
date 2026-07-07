# Judge Calibration Grader

A local-only web app for a human to grade real judge-assertion rubrics
against the mined (judge, candidate) relevance matrix at
`e2e/judge-calibration/mined/matrix.jsonl`, writing verdicts **directly**
into the real, checked-in `e2e/judge-calibration/evals/labels.yaml`.

Stdlib-only: the backend is `http.server` plain Python (no Flask/FastAPI,
no PyYAML), and the frontend is plain HTML/CSS/vanilla JS (no npm build, no
bundler). `python3 server.py` is the entire install step.

## The grading unit: a (judge, candidate) matrix cell

Earlier iterations of this tool graded a bare mined conversation
("pass/fail this whole candidate"). That was the wrong shape: judges are
graded against *their own* rubric, and the same candidate can be relevant
to several different judges with different verdicts. The grading unit is
now a single matrix cell -- one judge, one candidate, one narrow
(user, assistant) excerpt -- and the question is **"was the judge
satisfied?"**, not "is this candidate good?".

The flow is judge-first:

1. Pick a judge (its full rubric/prompt text, pulled straight from the eval
   suite that defines it, is shown prominently).
2. Review that judge's relevant matrix cells one at a time -- each cell
   shows just the narrow (user, assistant) pair mined for it.
3. Grade: **Pass**, **Fail**, or **Inconclusive**.

## Deterministic pre-check panel (informational only)

Above the (user, assistant) excerpt, a cell view shows a **"Deterministic
pre-check"** panel when one is available. Every judge's real eval-suite
`Case` usually carries other, *deterministic* sibling assertions right next
to the `judge` assertion -- `text_contains`/`text_not_contains` for the
discovery cases, or a real Python `script` checker for the
invocation/baseline/clean-comments-review cases. This panel is a cached,
already-run, real execution of those sibling assertions (see
`backend/precheck.py`) against the same candidate response the human is
about to grade: each assertion's type, its real pass/fail/errored outcome,
and its reason text (when the checker produced one), plus a one-line
overall summary (`"2/2 deterministic checks pass"`, or `"1/2 fail: script
check found <reason>"`).

**This is a hint, not a grade.** It is rendered in a visually distinct,
dashed/accent-bordered box above the rubric-adjacent content, labeled "hint,
not a grade", and carries its own disclaimer -- it never pre-fills, never
auto-submits, and has no click handler wired to Pass/Fail/Inconclusive.
Those three buttons read only the human's own click/keypress; nothing about
this panel's content is consulted when a grade is written to
`evals/labels.yaml`. This mirrors how a judge's `needs_human_review` tag is
already shown as a purely informational badge rather than a control. A cell
with zero *executed* deterministic checks (only `judge`/`tokens`/`latency_ms`
present) says so explicitly and is never rendered as if it were a pass --
"no checks" and "checks passed" are visually and textually distinct. If a
cell has no cached pre-check data at all, the panel simply doesn't render
(no misleading placeholder).

For the 4 **mined** (real agent-session) matrix cells whose assistant turn
is structured content (prose plus `tool_use` blocks), a `script` check is
run against the narration *plus* a fenced code block synthesized from that
session's own Edit/Write/MultiEdit tool calls (see
`conversation_draft.flatten_for_checker`) -- because a script checker
otherwise sees no code at all in the prose-only projection. When this
flattening was used, the panel says so plainly with a small caption:
*"checked against code from this session's Edit/Write calls, rendered as a
code block"*. **This pre-check is informational only, and deliberately more
generous than what the real, production judge/script assertion currently
sees for those same mined cells** -- production reads only the final
assistant turn's *text*, so a mined cell's real tool-call content is
invisible to it today. A pre-check "pass" here is not a claim about what the
live evaluator currently does with that same conversation; treat it as a
second, independent data point for your own judgment, not as ground truth.

The panel is served by `GET /api/precheck?judge_id=...&candidate_id=...`,
backed by a `PrecheckStore` that loads a cached batch run,
`mined/precheck_results.json` (produced separately by `python3 -m
backend.precheck`, or override its location with `--precheck-path`) --
the server never re-runs a `script` assertion's subprocess itself on a
page view.

## Running it

```sh
cd e2e/judge-calibration/grader
python3 server.py                 # binds 127.0.0.1:8765, opens a browser tab
python3 server.py --port 9000     # pick a different port
python3 server.py --no-browser    # don't auto-open a tab
```

The server binds to `127.0.0.1` only, by design -- there is no `--host`
flag. This tool reads verbatim excerpts of real agent-session transcripts
from your (and possibly your clients') private codebases; it must never be
reachable from anywhere but this machine.

By default it reads judges from `../evals/judges.yaml`, matrix cells from
`../mined/matrix.jsonl` (and their conversation drafts from
`../mined/matrix/`), the deterministic pre-check cache from
`../mined/precheck_results.json` (see above -- optional; grading works fine
without it, the pre-check panel just doesn't render), and writes verdicts to
`../evals/labels.yaml`. Override with `--judges-path`, `--mined-dir`,
`--evals-dir`, and `--precheck-path` if needed.

## Keyboard shortcuts

While reviewing a judge's cell (and not focused in a text field):

| Key | Action |
| --- | --- |
| `p` | **Pass** -- the judge would have been satisfied. Writes immediately, advances to the next ungraded cell. |
| `f` | **Fail** -- the judge would not have been satisfied. Writes immediately, advances to the next ungraded cell. |
| `i` | **Inconclusive** -- can't tell from this excerpt. Skips with **no write at all** (functionally identical to a prior "Skip" action); the cell resurfaces once every other ungraded cell for this judge has been through the queue. |

There is no undo. Pass/Fail is a direct read-modify-write of the real
`evals/labels.yaml` the moment you press the key or click the button --
there is no draft file, no "include in export" toggle, and no separate
export step. If you mis-grade a cell, click it again from the sidebar list
and re-grade it (the write overwrites the existing entry for that cell).

## evals/labels.yaml: one flat file, composite-keyed

Because a grading unit is now `(judge, candidate)` rather than a bare
candidate, and a label set spans every judge in `judges.yaml` (11 judges
across several different eval suites, not just one), `labels.yaml` is a
single flat map keyed by a stable composite key:

```
<judge_id, with '/' replaced by '__'>__<candidate_id>: Pass|Fail
```

e.g.:

```yaml
patterns-eval__baseline__configuring-logging__claude-code-ailly-two-66e52d4e02-001: Pass
```

A cell that has never been graded simply has **no entry** -- there is no
`TODO` placeholder in this file (unlike the old mined-pool draft format).
This matches `src/knowledge/calibration.rs`'s own `MissingLabel` error for
an absent key: absence *is* "not graded yet."

Values are exactly `Pass`/`Fail`, matching `calibration.rs`'s
`HumanVerdict` enum's serde representation (`#[derive(Serialize,
Deserialize)]`, no `rename_all` -- verified empirically against a live
`serde_yaml_ng::to_string`, not assumed) -- capitalized, not lowercase.

One flat file was chosen over one-file-per-suite: nothing in
`compute_calibration`'s current API requires a per-suite split (it takes a
plain `BTreeMap<String, HumanVerdict>` the *caller* assembles), and a
label set this size doesn't yet justify the extra bookkeeping of a
file-per-suite convention. A future CLI step can slice this flat map by
judge_id/suite when wiring it into a per-suite `compute_calibration` call.

## The mined data (judges.yaml, matrix.jsonl, matrix/, candidates.jsonl)

This app reads data produced by a separate, already-run pipeline (a judge
discovery step and a relevance-matrix build step, in
`e2e/judge-calibration/evals/scripts/`):

- **`evals/judges.yaml`** -- the judge registry (one entry per real `judge`
  assertion found under `e2e/*/evals/*.yaml`: judge_id, suite, case_name,
  full prompt/rubric text, keywords). This lives under `evals/`, is
  **not** gitignored, and is committed like any other source eval data
  (mirroring how `evals/labels.yaml` itself is committed).
- **`mined/matrix.jsonl`** -- one line per relevant (judge, candidate)
  cell (judge_id, candidate_id, matched_keyword/tag, source,
  project_cwd, and a `conversation_draft` path). Read fresh at server
  start; nothing here assumes a fixed row count, so this file growing
  (e.g. a parallel synthetic-dataset effort landing more rows) needs no
  code change to be picked up.
- **`mined/matrix/<judge-id-safe>/<candidate-id>.yaml`** -- the narrow
  (user, assistant) conversation-schema draft for each cell, already
  trimmed down from the full mined conversation.
- **`mined/candidates.jsonl`** -- the full mined-candidate pool, kept
  alongside for id cross-referencing.
- **`mined/precheck_results.json`** -- optional, the deterministic
  pre-check panel's cache (see above): one record per matrix cell, produced
  by a separate batch run of `python3 -m backend.precheck` (which itself
  reads `judges.yaml` + `matrix.jsonl` and re-derives each judge's real
  sibling assertions straight from its eval-suite file via
  `backend/eval_case_yaml.py` -- nothing about the checks themselves is
  hardcoded here). Missing this file just means the pre-check panel doesn't
  render; it never blocks grading.

`mined/` (all of it, including `matrix.jsonl` and `matrix/`) stays
gitignored -- it contains verbatim excerpts of real agent-session
transcripts (personal projects and client codebases alike) and must never
be committed. This mirrors exactly how the mined pool's own
`candidates.jsonl` was already excluded. Copy this data in the same way it
was copied here originally: from the sibling worktree that ran the
judge-discovery and relevance-matrix pipeline
(`e2e/judge-calibration/evals/judges.yaml`, `mined/matrix.jsonl`,
`mined/matrix/`, and `mined/candidates.jsonl`); regenerate
`mined/precheck_results.json` locally with `python3 -m backend.precheck`.

## Layout

```
grader/
  server.py            entry point (argparse: --port, --judges-path, --mined-dir, --evals-dir,
                        --precheck-path, --no-browser)
  backend/
    app.py               HTTP routing + AppContext (state, mutations, locking)
    judges.py            evals/judges.yaml loader; hand-rolled parser for its fixed
                          (PyYAML-generated) block-sequence-of-mappings shape; the
                          judge-id half of the security allowlist lives here
    matrix.py            mined/matrix.jsonl loader; the candidate-id half of the
                          security allowlist lives here; resolves conversation_draft
                          paths from trusted, already-loaded records only
    conversation_draft.py parses a narrow (user, assistant) draft yaml file (scalar or
                          ContentBlock-list content), plus flatten_for_checker's
                          tool_use-to-fenced-code-block projection for script pre-checks
    eval_case_yaml.py     loads one Case's real assertions straight out of an
                          e2e/*/evals/*.yaml suite file, for backend/precheck.py
    precheck.py           runs a judge's real deterministic sibling assertions
                          (text_contains/text_not_contains/script) against a mined
                          candidate; PrecheckStore loads the cached batch-run JSON
                          for the GET /api/precheck route (informational only)
    labels_store.py      the real evals/labels.yaml composite-key flat-map read/write
                          (hand-rolled, stdlib-only, atomic write)
  static/
    index.html, style.css, app.js    the single-page frontend (judge picker -> judge + cell
                          review, including the informational-only "Deterministic
                          pre-check" panel)
  tests/                unittest suite: parsers (judges.yaml, matrix.jsonl,
                         conversation drafts), precheck's assertion runners + PrecheckStore,
                         labels_store round-trip, and a real in-process HTTP integration
                         test (including path-traversal / unknown-id security cases for
                         BOTH judge_id and candidate_id, and /api/precheck's availability
                         states)
```

## Tests

```sh
cd e2e/judge-calibration/grader
python3 -m unittest discover -s tests -t .
```

No `pip install` required -- everything here is Python's standard library
(`http.server`, `unittest`, `json`, `urllib`).

## Security note: judge-id AND candidate-id allowlist

Every endpoint that takes a judge id and/or candidate id (`/api/judge`,
`/api/cell`, `/api/precheck`, `/api/grade`) validates the judge id against
`JudgeRegistry.is_known_id` (built from `evals/judges.yaml`) and the
`(judge_id, candidate_id)` pair against `MatrixStore.is_known_pair` (built
from `mined/matrix.jsonl`) *before* any filesystem access. A conversation
draft's path is only ever taken from the already-loaded, trusted matrix
record -- never reconstructed by concatenating raw request input into a
path -- so a path-traversal-shaped id is rejected at the allowlist check
and never reaches a file read. See
`tests/test_app_integration.py::test_path_traversal_judge_id_rejected_without_touching_filesystem`
and its candidate-id counterpart.

## Dropped from v1: manual include/export, and the similarity heuristic

Two things from the earlier bare-candidate grader were deliberately not
carried forward:

- **The include/export ceremony.** Grading now writes directly to the
  real `evals/labels.yaml` the moment you press Pass/Fail -- there is no
  "mark for export" toggle, no separate draft file, and no export button.
- **The cosine-similarity suggestion heuristic.** It doesn't naturally fit
  the new narrow-excerpt-per-cell model: each judge today has only 1-3
  relevant cells (the whole matrix is 5 cells across 4 judges as of this
  writing), which is too few neighbors per judge to make a similarity
  suggestion meaningful, and neighbors from a *different* judge's cells
  aren't comparable at all (a "Pass" under one judge's rubric says nothing
  about whether a different judge would be satisfied). If the matrix grows
  large enough per-judge for this to make sense again, it would need to be
  re-scoped to compare only within a single judge's own graded cells.
