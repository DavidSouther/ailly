"""Pre-check: run a judge's REAL deterministic sibling assertions against a
matrix cell's candidate response, as an informational hint alongside the
human grade -- NOT an auto-filled grade.

Every judge in ``evals/judges.yaml`` was discovered from a real ``type:
judge`` assertion inside some ``Case`` in an actual ``e2e/*/evals/*.yaml``
eval suite file. That same ``Case`` usually carries other, deterministic
sibling assertions right alongside the judge -- ``text_contains``/
``text_not_contains`` for the discovery cases, or a real Python ``script``
checker for the invocation/baseline/clean-comments-review cases. This module
reads those sibling assertions straight out of the real suite file (via
``backend/eval_case_yaml.py`` -- never a hardcoded duplicate) and actually
executes them against the matrix cell's mined candidate content, so the
human grader gets a second, independent, deterministic opinion to compare
their own judgment against.

What gets executed, and what doesn't
-------------------------------------
- ``text_contains`` / ``text_not_contains`` / ``text_matches`` /
  ``text_equals`` -- direct string/regex logic, matching DESIGN.md's
  documented semantics (``case_sensitive`` default ``True``, ``text_matches``
  compiling ``pattern`` with optional inline ``flags``, byte-offset failure
  reasons). Run against the assistant turn's TEXT ONLY (``text_only`` --
  every real discovery case is entirely about whether the response NAMES the
  right skill/pattern in prose, so text-only is correct and sufficient; no
  flattening is applied here).
- ``script`` -- a real subprocess invocation of the actual checker file
  named by the assertion (``runtime: python``, ``script: {path: ...}``),
  with the real stdin/env contract from ``src/knowledge/assertions.rs``:
  candidate response on stdin, the mined user turn's text in
  ``AILLY_USER_QUESTION``, exit 0 = Pass, exit!=0 with stdout = Fail (stdout
  is the reason), exit!=0 with empty stdout but non-empty stderr = Errored
  (a broken checker, not a failing candidate), exit!=0 with both streams
  empty = Fail. Run against the assistant turn's FLATTENED content (see
  below).
- ``judge`` is always skipped -- it is the thing this whole calibration
  effort exists to calibrate, not a pre-check.
- ``tokens`` / ``latency_ms`` are always skipped -- these narrow,
  reconstructed drafts do not carry reliable full-conversation trace data
  (token counts, latency) to check them against.
- Any other/future assertion type is skipped with a note rather than
  crashing the whole pre-check run, so a suite-file change surfaces as a new
  "skipped: unsupported type" line instead of an exception.

The flattening wrinkle (mined cells only)
------------------------------------------
15 of the 19 matrix cells are "synthetic-live": the assistant turn is a
plain block-literal string, already containing any code directly as fenced
markdown -- exactly what a checker script's ``extract_code`` helper expects.
The other 4 are "mined" real agent-session excerpts, and 3 of those have a
structured (ContentBlock) assistant turn: prose PLUS ``tool_use`` blocks
(Edit/Write/MultiEdit) that carry the real file diff in the tool call's
*input*, not in the narration text.

Production's real ``script``/``judge`` assertions read only "the final
assistant turn's text content" (see DESIGN.md and
``src/knowledge/assertions.rs::final_assistant_text``/``check_script``) --
tool_use blocks contribute nothing there. Feeding a script checker that same
text-only projection for a mined cell would starve it of the one thing it's
looking for (the code), reproducing that blindness as a pre-check bug rather
than a helpful hint. So for ``script`` assertions ONLY, this module instead
hands the checker ``conversation_draft.flatten_for_checker``'s output: the
narration text, plus a fenced code block (language inferred from the file
extension) for every real Write/Edit/MultiEdit ``tool_use`` block, appended
in order. It never fabricates code that wasn't really there -- a tool_use
block this module doesn't know how to flatten (Task, Bash, Read, ...)
contributes nothing, same as production.

**This flattening is deliberately MORE generous than what the real,
production `judge`/`script` assertion sees today.** A pre-check "Pass" or
"Fail" against a mined cell reflects that more-generous read, not a claim
about what the live evaluator currently does with that same conversation.
Text assertions (``text_contains`` and friends) are NOT given this
treatment -- they run against ``text_only`` exactly as production does,
because a discovery case is genuinely about the response's own prose, not
about code buried in a tool call.
"""
from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path
from typing import Any

from . import conversation_draft
from .eval_case_yaml import load_case_assertions
from .judges import JudgeRegistry
from .matrix import MatrixStore

SKIPPED_ASSERTION_TYPES = {"judge", "tokens", "latency_ms"}
TEXT_ASSERTION_TYPES = {"text_contains", "text_not_contains", "text_matches", "text_equals"}
SCRIPT_TIMEOUT_SECONDS = 30.0


def _byte_len(s: str) -> int:
    return len(s.encode("utf-8", errors="surrogatepass"))


def _byte_offset(haystack: str, char_index: int) -> int:
    return _byte_len(haystack[:char_index])


def _check_text_contains(text: str, assertion: dict[str, Any]) -> tuple[str, str | None]:
    value = assertion.get("value", "")
    case_sensitive = assertion.get("case_sensitive")
    case_sensitive = True if case_sensitive is None else bool(case_sensitive)
    haystack = text if case_sensitive else text.lower()
    needle = value if case_sensitive else value.lower()
    if needle in haystack:
        return "pass", None
    return (
        "fail",
        f"text_contains: expected substring {value!r} not found in {_byte_len(text)}-byte response",
    )


def _check_text_not_contains(text: str, assertion: dict[str, Any]) -> tuple[str, str | None]:
    value = assertion.get("value", "")
    case_sensitive = assertion.get("case_sensitive")
    case_sensitive = True if case_sensitive is None else bool(case_sensitive)
    haystack = text if case_sensitive else text.lower()
    needle = value if case_sensitive else value.lower()
    idx = haystack.find(needle)
    if idx == -1:
        return "pass", None
    return (
        "fail",
        f"text_not_contains: forbidden substring {value!r} found at byte offset {_byte_offset(haystack, idx)}",
    )


def _check_text_matches(text: str, assertion: dict[str, Any]) -> tuple[str, str | None]:
    pattern = assertion.get("pattern", "")
    flags_str = assertion.get("flags") or ""
    flag_map = {"i": re.IGNORECASE, "m": re.MULTILINE, "s": re.DOTALL, "x": re.VERBOSE}
    py_flags = 0
    for ch in flags_str:
        py_flags |= flag_map.get(ch, 0)
    try:
        regex = re.compile(pattern, py_flags)
    except re.error as exc:
        return "malformed", f"text_matches: regex compile error: {exc}"
    if regex.search(text):
        return "pass", None
    return "fail", f"text_matches: pattern {pattern!r} did not match {_byte_len(text)}-byte response"


def _check_text_equals(text: str, assertion: dict[str, Any]) -> tuple[str, str | None]:
    value = assertion.get("value", "")
    if text == value:
        return "pass", None
    return (
        "fail",
        f"text_equals: expected {value!r} ({_byte_len(value)} bytes) but got "
        f"{text!r} ({_byte_len(text)} bytes)",
    )


_TEXT_CHECKERS = {
    "text_contains": _check_text_contains,
    "text_not_contains": _check_text_not_contains,
    "text_matches": _check_text_matches,
    "text_equals": _check_text_equals,
}


def _run_text_assertion(assertion: dict[str, Any], assistant_text: str) -> tuple[str, str | None]:
    return _TEXT_CHECKERS[assertion["type"]](assistant_text, assertion)


def _build_child_env(project_root: Path, user_question: str) -> dict[str, str]:
    """Mirrors ``build_child_env`` in ``src/knowledge/assertions.rs``: a
    cleared env carrying just ``PATH``, ``AILLY_PROJECT_ROOT``,
    ``AILLY_USER_QUESTION``, and ``HOME``/``TMPDIR``/``LANG`` when present.
    None of the ``script`` assertions in scope for this pre-check declare a
    ``pass_env``, so that opt-in allowlist isn't implemented here."""
    env: dict[str, str] = {}
    if "PATH" in os.environ:
        env["PATH"] = os.environ["PATH"]
    env["AILLY_PROJECT_ROOT"] = str(project_root)
    if "\0" not in user_question:
        env["AILLY_USER_QUESTION"] = user_question
    for key in ("HOME", "TMPDIR", "LANG"):
        if key in os.environ:
            env[key] = os.environ[key]
    return env


def _run_script_assertion(
    assertion: dict[str, Any],
    project_root: Path,
    candidate_text: str,
    user_question: str,
) -> tuple[str, str | None]:
    runtime = assertion.get("runtime")
    if runtime != "python":
        return "malformed", f"script: pre-check only supports runtime=python (got {runtime!r})"
    script = assertion.get("script")
    if not isinstance(script, dict) or not script.get("path"):
        return "malformed", f"script: pre-check only supports {{script: {{path: ...}}}} (got {script!r})"

    project_root = project_root.resolve()
    resolved = (project_root / script["path"]).resolve()
    try:
        resolved.relative_to(project_root)
    except ValueError:
        return "malformed", f"script: path {script['path']!r} escapes project root {project_root}"
    if not resolved.is_file():
        return "errored", f"script: checker file not found: {resolved}"

    env = _build_child_env(project_root, user_question)
    try:
        proc = subprocess.run(
            [sys.executable, str(resolved)],
            input=candidate_text.encode("utf-8"),
            cwd=str(project_root),
            env=env,
            capture_output=True,
            timeout=SCRIPT_TIMEOUT_SECONDS,
        )
    except subprocess.TimeoutExpired:
        return "errored", f"script: timed out after {SCRIPT_TIMEOUT_SECONDS}s"
    except OSError as exc:
        return "errored", f"script: failed to spawn {resolved}: {exc}"

    stdout = proc.stdout.decode("utf-8", errors="replace").strip()
    stderr = proc.stderr.decode("utf-8", errors="replace").strip()
    if proc.returncode == 0:
        return "pass", None
    if stdout:
        return "fail", stdout
    if stderr:
        return "errored", f"script: exit {proc.returncode}, no stdout, stderr: {stderr[:500]}"
    return "fail", f"script: exit {proc.returncode} (no output)"


def project_root_for_suite_file(repo_root: Path, suite_file: str) -> Path:
    """The ``e2e/<suite>/`` project directory a ``script`` assertion's
    ``script.path`` resolves against (mirrors ``ailly``'s own
    ``AILLY_PROJECT_ROOT`` for that suite -- see e.g.
    ``e2e/patterns-eval/ci.sh``'s ``project_dir``): the eval suite's own
    directory, two levels up from ``evals/<file>.yaml``."""
    return (repo_root / suite_file).resolve().parent.parent


def precheck_cell(
    repo_root: Path,
    judge: dict[str, Any],
    candidate_id: str,
    conversation: dict[str, Any],
) -> dict[str, Any]:
    """Run ``judge``'s real sibling assertions against one already-parsed
    matrix-cell conversation draft (``conversation_draft.parse_conversation_draft``'s
    return value). ``judge`` is one record from ``JudgeRegistry`` (i.e. one
    entry from ``evals/judges.yaml``)."""
    judge_id = judge["judge_id"]
    suite_file = judge["suite_file"]
    case_name = judge["case_name"]

    assertions = load_case_assertions(repo_root / suite_file, case_name)

    user_raw, assistant_raw = conversation_draft.raw_user_and_assistant(conversation)
    assistant_text = conversation_draft.text_only(assistant_raw)
    user_text = conversation_draft.text_only(user_raw)
    content_shape = "blocks" if isinstance(assistant_raw, list) else "scalar"

    checks: list[dict[str, Any]] = []
    flattening_applied = False
    flattened_text: str | None = None

    for assertion in assertions:
        a_type = assertion.get("type")
        if a_type in SKIPPED_ASSERTION_TYPES:
            checks.append(
                {
                    "type": a_type,
                    "outcome": "skipped",
                    "reason": "not pre-checked by design (see backend/precheck.py module docstring)",
                    "source": assertion,
                }
            )
            continue
        if a_type in TEXT_ASSERTION_TYPES:
            outcome, reason = _run_text_assertion(assertion, assistant_text)
            checks.append({"type": a_type, "outcome": outcome, "reason": reason, "source": assertion})
            continue
        if a_type == "script":
            if flattened_text is None:
                flattened_text = conversation_draft.flatten_for_checker(assistant_raw)
                flattening_applied = content_shape == "blocks"
            project_root = project_root_for_suite_file(repo_root, suite_file)
            outcome, reason = _run_script_assertion(assertion, project_root, flattened_text, user_text)
            checks.append({"type": "script", "outcome": outcome, "reason": reason, "source": assertion})
            continue
        checks.append(
            {
                "type": a_type,
                "outcome": "skipped",
                "reason": f"unsupported assertion type {a_type!r} for pre-check",
                "source": assertion,
            }
        )

    executed = [c for c in checks if c["outcome"] != "skipped"]
    if not executed:
        overall = "no_checks"
    elif all(c["outcome"] == "pass" for c in executed):
        overall = "all_pass"
    else:
        overall = "some_fail"

    return {
        "judge_id": judge_id,
        "candidate_id": candidate_id,
        "suite_file": suite_file,
        "case_name": case_name,
        "content_shape": content_shape,
        "flattening_applied_for_script_checks": flattening_applied,
        "checks": checks,
        "overall": overall,
    }


def run_all(repo_root: Path, judges: JudgeRegistry, matrix: MatrixStore) -> list[dict[str, Any]]:
    """Pre-check every (judge, candidate) matrix cell, in matrix.jsonl order."""
    results = []
    for cell in matrix.all_cells():
        judge_id = cell["judge_id"]
        candidate_id = cell["candidate_id"]
        judge = judges.get(judge_id)
        if judge is None:
            results.append(
                {
                    "judge_id": judge_id,
                    "candidate_id": candidate_id,
                    "overall": "no_checks",
                    "checks": [],
                    "error": f"unknown judge_id {judge_id!r} (not in evals/judges.yaml)",
                }
            )
            continue
        draft_path = matrix.resolve_conversation_draft_path(cell)
        try:
            conversation = conversation_draft.load_conversation_draft(draft_path)
            result = precheck_cell(repo_root, judge, candidate_id, conversation)
        except (ValueError, OSError) as exc:
            result = {
                "judge_id": judge_id,
                "candidate_id": candidate_id,
                "overall": "no_checks",
                "checks": [],
                "error": f"{type(exc).__name__}: {exc}",
            }
        results.append(result)
    return results


class PrecheckStore:
    """Loaded ``mined/precheck_results.json`` -- the output of a separate,
    already-run batch step (``python3 -m backend.precheck``, see this
    module's ``main`` below) -- indexed by ``(judge_id, candidate_id)`` for
    the grader app's ``GET /api/precheck`` route (``backend/app.py``).

    Deliberately read-only and never re-runs a ``script`` assertion's
    subprocess itself: the app a human grader is using stays fast and free
    of surprise side effects, and a served pre-check result always traces
    back to one distinct, auditable batch run (re-run explicitly with
    ``python3 -m backend.precheck``) rather than to eval-suite checker
    scripts silently re-executing on every page view."""

    def __init__(self, path: Path | None):
        self.path = Path(path) if path is not None else None
        self._by_pair: dict[tuple[str, str], dict[str, Any]] = {}
        self.reload()

    def reload(self) -> None:
        by_pair: dict[tuple[str, str], dict[str, Any]] = {}
        if self.path is not None and self.path.exists():
            records = json.loads(self.path.read_text(encoding="utf-8"))
            for record in records:
                judge_id = record.get("judge_id")
                candidate_id = record.get("candidate_id")
                if judge_id and candidate_id:
                    by_pair[(judge_id, candidate_id)] = record
        self._by_pair = by_pair

    def get(self, judge_id: str, candidate_id: str) -> dict[str, Any] | None:
        """The cached pre-check result for this exact (judge, candidate)
        pair, or ``None`` if the batch run never covered it (a stale cache,
        or the cache file doesn't exist at all yet) -- callers must treat
        ``None`` as "no informational hint available", never as "0 checks
        pass"."""
        return self._by_pair.get((judge_id, candidate_id))

    def __len__(self) -> int:
        return len(self._by_pair)


def main(argv: list[str] | None = None) -> int:
    app_dir = Path(__file__).resolve().parent.parent  # .../grader
    default_repo_root = app_dir.parent.parent.parent  # grader -> judge-calibration -> e2e -> repo root

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, default=default_repo_root)
    parser.add_argument("--mined-dir", type=Path, default=app_dir.parent / "mined")
    parser.add_argument("--evals-dir", type=Path, default=app_dir.parent / "evals")
    parser.add_argument("--judges-path", type=Path, default=None)
    parser.add_argument(
        "--output",
        type=Path,
        default=None,
        help="where to write precheck_results.json (default: <mined-dir>/precheck_results.json)",
    )
    args = parser.parse_args(argv)

    judges_path = args.judges_path or (args.evals_dir / "judges.yaml")
    matrix_path = args.mined_dir / "matrix.jsonl"
    output_path = args.output or (args.mined_dir / "precheck_results.json")

    judges = JudgeRegistry(judges_path)
    matrix = MatrixStore(matrix_path, mined_dir=args.mined_dir)

    results = run_all(args.repo_root.resolve(), judges, matrix)

    output_path.write_text(json.dumps(results, indent=2) + "\n", encoding="utf-8")

    total = len(results)
    all_pass = sum(1 for r in results if r["overall"] == "all_pass")
    some_fail = sum(1 for r in results if r["overall"] == "some_fail")
    no_checks = sum(1 for r in results if r["overall"] == "no_checks")
    print(f"pre-checked {total} matrix cells -> {output_path}")
    print(f"  all_pass: {all_pass}  some_fail: {some_fail}  no_checks: {no_checks}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
