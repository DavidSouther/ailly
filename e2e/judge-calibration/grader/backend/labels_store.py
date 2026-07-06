"""Read/write for the real, checked-in e2e/judge-calibration/evals/labels.yaml.

A grading unit is now a (judge, candidate) matrix cell rather than a bare
candidate, so this file is a single FLAT map across every judge, keyed by a
stable composite key rather than one file per suite:

    <judge_id, with '/' replaced by '__'>__<candidate_id>: Pass|Fail

e.g. ``patterns-eval__baseline__configuring-logging__claude-code-ailly-two-66e52d4e02-001: Pass``.

Composite-key rationale: ``calibration.rs``'s ``ExampleAgreement.id`` doc
comment says it "Matches the suite case's `name:`", i.e. today's labels.yaml
convention is one flat map per suite, keyed by case name. A judge-calibration
label set now spans 11 judges across several different suites, and folding
that many small per-suite files together would multiply bookkeeping (one
label file per suite, discovered by scanning `judges.yaml`) for no benefit
this app's own callers need yet; nothing in calibration.rs's current API
requires a per-suite split (`compute_calibration` takes a plain
`BTreeMap<String, HumanVerdict>` the *caller* assembles -- a future CLI step
can slice this flat map by judge_id/suite when wiring it into a per-suite
`compute_calibration` call, same as it would have to look up any other
subset of a larger map). One flat, diff-friendly file was chosen over an
early, unforced split.

Value casing: verified empirically against `src/knowledge/calibration.rs`'s
`HumanVerdict` enum (`#[derive(Serialize, Deserialize)]` with no
`rename_all`) -- serde's default unit-variant representation is the
capitalized Rust identifier, i.e. `Pass`/`Fail`, NOT lowercase. This module
writes and expects exactly that casing so a future Rust loader that
deserializes this file's values straight into `HumanVerdict` works with no
translation step.

There is no `TODO` sentinel here (unlike v1's mined/-draft labels.yaml):
an ungraded cell is simply *absent* from the map, matching
`compute_calibration`'s own `MissingLabel` error for an absent key -- the
real ground-truth semantics, not a placeholder value.
"""
from __future__ import annotations

import re
import tempfile
from pathlib import Path

VALID_VERDICTS = ("Pass", "Fail")

_ENTRY_RE = re.compile(r"^([A-Za-z0-9_.-]+):\s*(Pass|Fail)\s*$")

DEFAULT_HEADER = """\
# Judge-calibration ground truth: human verdicts for (judge, candidate)
# matrix cells, one flat map across every judge in evals/judges.yaml.
#
# Key shape: "<judge_id, '/' -> '__'>__<candidate_id>", e.g.
#   patterns-eval__baseline__configuring-logging__claude-code-ailly-two-66e52d4e02-001: Pass
#
# Written directly by the judge-calibration grader app (e2e/judge-calibration/grader)
# the moment a human presses Pass or Fail while reviewing a cell -- there is
# no separate draft/export step. A cell simply absent from this map has not
# been graded yet ("Inconclusive" in the grader UI skips a cell with no
# write, same as leaving it out entirely).
#
# Values are "Pass" or "Fail", matching src/knowledge/calibration.rs's
# HumanVerdict serde representation exactly (capitalized, no rename_all).
"""


def composite_key(judge_id: str, candidate_id: str) -> str:
    """Build the stable composite key for one (judge, candidate) cell."""
    return f"{judge_id.replace('/', '__')}__{candidate_id}"


def _split_header(text: str) -> tuple[str, list[str]]:
    lines = text.splitlines()
    i = 0
    while i < len(lines) and (lines[i].strip() == "" or lines[i].lstrip().startswith("#")):
        i += 1
    header = "\n".join(lines[:i])
    if header:
        header += "\n"
    return header, lines[i:]


def parse_labels_text(text: str) -> dict[str, str]:
    """Parse the flat ``key: Pass|Fail`` entries out of labels.yaml text."""
    _, entry_lines = _split_header(text)
    labels: dict[str, str] = {}
    for line in entry_lines:
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        m = _ENTRY_RE.match(stripped)
        if not m:
            raise ValueError(f"unrecognized labels.yaml entry line: {line!r}")
        labels[m.group(1)] = m.group(2)
    return labels


def read_labels(path: Path) -> dict[str, str]:
    """Read the composite-key -> verdict map from ``path``. Missing file -> empty map."""
    p = Path(path)
    if not p.exists():
        return {}
    return parse_labels_text(p.read_text(encoding="utf-8"))


def read_header(path: Path) -> str:
    p = Path(path)
    if not p.exists():
        return DEFAULT_HEADER
    header, _ = _split_header(p.read_text(encoding="utf-8"))
    return header or DEFAULT_HEADER


def render_labels_text(labels: dict[str, str], header: str | None = None) -> str:
    """Render the composite-key map to labels.yaml text, keys sorted for a
    stable, diff-friendly on-disk order (there is no candidates.jsonl-style
    canonical ordering that spans every judge, so lexicographic is the
    simplest stable choice)."""
    header = header if header is not None else DEFAULT_HEADER
    if header and not header.endswith("\n"):
        header += "\n"
    lines = [header.rstrip("\n"), ""]
    for key in sorted(labels):
        lines.append(f"{key}: {labels[key]}")
    return "\n".join(lines) + "\n"


def write_labels_atomic(path: Path, labels: dict[str, str], header: str | None = None) -> None:
    """Atomically write labels.yaml (write-temp-then-rename)."""
    p = Path(path)
    p.parent.mkdir(parents=True, exist_ok=True)
    text = render_labels_text(labels, header=header)
    fd, tmp_name = tempfile.mkstemp(dir=str(p.parent), prefix=".labels-", suffix=".tmp")
    try:
        with open(fd, "w", encoding="utf-8") as f:
            f.write(text)
        Path(tmp_name).replace(p)
    finally:
        if Path(tmp_name).exists():
            Path(tmp_name).unlink(missing_ok=True)


def set_verdict(path: Path, judge_id: str, candidate_id: str, verdict: str) -> dict[str, str]:
    """Read-modify-write one cell's verdict into labels.yaml. Returns the new map."""
    if verdict not in VALID_VERDICTS:
        raise ValueError(f"invalid verdict: {verdict!r}, expected one of {VALID_VERDICTS}")
    header = read_header(path)
    labels = read_labels(path)
    labels[composite_key(judge_id, candidate_id)] = verdict
    write_labels_atomic(path, labels, header=header)
    return labels
