# Judge Calibration Grader

A local-only web app for a human to review and grade the mined
judge-calibration candidates at `e2e/judge-calibration/mined/candidates.jsonl`,
and to curate a final calibration set into `e2e/judge-calibration/evals/labels.yaml`.

Stdlib-only: the backend is `http.server`/`wsgiref`-style plain Python (no
Flask/FastAPI), and the frontend is plain HTML/CSS/vanilla JS (no npm build,
no bundler). `python3 server.py` is the entire install step.

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

By default it looks for data at `../mined` and writes curated exports to
`../evals`, relative to `server.py` (i.e. the standard
`e2e/judge-calibration/{mined,evals}` layout). Override with `--mined-dir`
and `--evals-dir` if needed.

## Keyboard shortcuts

While reviewing a candidate (and not focused in a text field):

| Key | Action |
| --- | --- |
| `p` | Grade **Pass** and advance to the next ungraded candidate |
| `f` | Grade **Fail** and advance to the next ungraded candidate |
| `s` or `n` | **Skip** for now (no write) -- comes back around after the rest of the current queue |
| `i` | Toggle **Include in calibration export** for the currently graded candidate |

Grading writes to `mined/labels.yaml` immediately -- there is no separate
save step, and no undo. Skip never writes anything; the candidate stays
`TODO` and resurfaces once every other ungraded candidate (in the current
filter) has been through the queue.

## mined/labels.yaml vs evals/labels.yaml

- **`mined/labels.yaml`** is the *draft* map covering all 663 mined
  candidates (`id: pass|fail|TODO`). It is what this app reads and writes
  as you grade. It is scratch/local-only -- see the next section.
- **`e2e/judge-calibration/evals/labels.yaml`** is the *curated,
  checked-in* file actually used for judge calibration. It only ever
  contains candidates that are **both** graded (pass/fail) **and**
  explicitly marked "Include in calibration export" (`i` key, or the
  checkbox). It is written only when you click **Export calibration set**
  in the top bar and confirm the dialog -- never automatically, and never
  as a side effect of grading. The mined pool is 663 candidates; a
  calibration set only needs on the order of 20-50, so exporting is a
  deliberate curation step, not "export everything."

## Suggestions are never confirmed grades

Two independent, clearly-labeled "Suggestion" badges may appear above a
candidate:

- **Weak signal from the human's next message** -- for the 5 candidates
  where mining found a low-confidence phrase in the human's following
  message (e.g. "looks good", "perfect"). Shows the verdict, the evidence
  phrase, and an explicit "not a confirmed label" disclaimer.
- **Similarity heuristic** -- once ~20 real (pass/fail) grades have
  accumulated, and again every ~20 grades after that, a dependency-free
  cosine-similarity-over-term-frequency heuristic (`backend/similarity.py`)
  compares each remaining ungraded candidate against every already-graded
  one. If a clear majority of its top-K most similar graded neighbors agree
  on a label above a similarity threshold, that's surfaced as a suggestion,
  along with the neighbor id/label/score that justifies it.

Neither badge ever pre-fills or auto-writes a grade. Both are purely
informational (plus a subtle highlight ring on the suggested Pass/Fail
button) until you explicitly press `p`/`f` or click the button yourself.
Suggestions are cached in `mined/suggestions.json` (also gitignored) so
they survive a server restart; recompute cadence and thresholds live in
`backend/suggestions_store.py`.

## Never commit mined/ data

`e2e/judge-calibration/mined/` (including this app's own
`included.json` and `suggestions.json` caches) is excluded by the
repo-root `.gitignore` (`e2e/judge-calibration/mined/`). It contains
verbatim excerpts of real session transcripts, some from other
clients'/projects' codebases. Only the curated `evals/labels.yaml` is
meant to be committed, and only after a human has actually reviewed and
exported it.

## Layout

```
grader/
  server.py            entry point (argparse: --port, --mined-dir, --evals-dir, --no-browser)
  backend/
    app.py              HTTP routing + AppContext (state, mutations, locking)
    candidates.py        loads candidates.jsonl; the id allowlist lives here
    labels_store.py      mined/labels.yaml flat-map read/write (hand-rolled, stdlib-only)
    included_store.py    mined/included.json (the "mark for export" flags)
    suggestions_store.py cadence + cache for the heuristic pre-fill
    similarity.py         the tokenize/cosine-similarity/suggest heuristic itself
    export.py             curation logic -> evals/labels.yaml
  static/
    index.html, style.css, app.js    the single-page frontend
  tests/                unittest suite (67 tests): stores, similarity, export,
                         id allowlist, and a real in-process HTTP integration
                         test (including path-traversal / unknown-id security cases)
  scripts/
    simulate_grading.py  grades ~20 real candidates via the live API, to
                          exercise the suggestion-recompute cadence without
                          20 rounds of manual clicking
```

## Tests

```sh
cd e2e/judge-calibration/grader
python3 -m unittest discover -s tests -t .
```

No `pip install` required -- everything here is Python's standard library
(`http.server`, `unittest`, `collections.Counter`, `json`, `urllib`).

## Security note: candidate-id allowlist

Every endpoint that takes a candidate id (`/api/candidate/<id>`, its
`/conversation` sub-route, `/api/grade`, `/api/include`) checks the id
against `CandidateStore.is_known_id` -- built from the ids actually present
in `candidates.jsonl` -- *before* it is used to build any filesystem path.
An id outside that allowlist (including path-traversal attempts) is
rejected with 404 and never touches the filesystem. See
`tests/test_app_integration.py::test_path_traversal_id_rejected_without_touching_filesystem`.
