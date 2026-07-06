"""Read/write for mined/labels.yaml.

labels.yaml is intentionally a *flat* mapping (``id: pass|fail|TODO``) with a
comment header, one entry per line -- see the header text baked into
``DEFAULT_HEADER`` below (copied verbatim from the mined draft produced by
the mining script). Because the shape is this constrained we do not take a
PyYAML dependency (this app is stdlib-only); a tiny hand-rolled parser/writer
for exactly this shape is simpler and has zero install steps.

Round trip contract: reading then writing back with no changes reproduces
byte-identical file content (this is exercised by a unit test). Writing an
update preserves the header verbatim and preserves candidate order.
"""
from __future__ import annotations

import re
import tempfile
from pathlib import Path

VALID_LABELS = ("pass", "fail", "TODO")

_ENTRY_RE = re.compile(r"^([A-Za-z0-9_.-]+):\s*(\S+)\s*$")

DEFAULT_HEADER = """\
# DRAFT — mined candidate labels for e2e/judge-calibration.
#
# Fill each value with `pass` or `fail` after reviewing the matching
# conversation under mined/conversations/<id>.yaml (full provenance and any
# low-confidence human-implied hint live in mined/candidates.jsonl). This
# file is itself local-only scratch (see the e2e/judge-calibration/mined/
# .gitignore entry) until a human curates a final
# e2e/judge-calibration/evals/labels.yaml from a reviewed subset of these.
#
# Shape matches feature-e-judge-calibration/design.md's labels.yaml: a flat
# { id: pass|fail } map.
"""


def _split_header(text: str) -> tuple[str, list[str]]:
    """Split leading comment/blank lines (the header) from entry lines."""
    lines = text.splitlines()
    i = 0
    while i < len(lines) and (lines[i].strip() == "" or lines[i].lstrip().startswith("#")):
        i += 1
    header = "\n".join(lines[:i])
    if header:
        header += "\n"
    return header, lines[i:]


def parse_labels_text(text: str) -> dict[str, str]:
    """Parse the flat ``id: label`` entries out of labels.yaml text."""
    _, entry_lines = _split_header(text)
    labels: dict[str, str] = {}
    for line in entry_lines:
        if not line.strip():
            continue
        m = _ENTRY_RE.match(line.strip())
        if not m:
            continue
        labels[m.group(1)] = m.group(2)
    return labels


def read_labels(path: Path) -> dict[str, str]:
    """Read the id->label map from ``path``. Missing file -> empty map."""
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


def render_labels_text(labels: dict[str, str], order: list[str], header: str | None = None) -> str:
    """Render id->label map to labels.yaml text.

    ``order`` gives the canonical candidate ordering (from candidates.jsonl).
    Any id present in ``labels`` but absent from ``order`` is appended,
    sorted, at the end so no data is ever silently dropped.
    """
    header = header if header is not None else DEFAULT_HEADER
    if header and not header.endswith("\n"):
        header += "\n"
    ordered_ids = list(order)
    known = set(ordered_ids)
    extra = sorted(set(labels) - known)
    lines = [header.rstrip("\n"), ""]
    for cid in ordered_ids:
        value = labels.get(cid, "TODO")
        lines.append(f"{cid}: {value}")
    for cid in extra:
        lines.append(f"{cid}: {labels[cid]}")
    return "\n".join(lines) + "\n"


def write_labels_atomic(path: Path, labels: dict[str, str], order: list[str], header: str | None = None) -> None:
    """Atomically write labels.yaml (write-temp-then-rename)."""
    p = Path(path)
    p.parent.mkdir(parents=True, exist_ok=True)
    text = render_labels_text(labels, order, header=header)
    fd, tmp_name = tempfile.mkstemp(dir=str(p.parent), prefix=".labels-", suffix=".tmp")
    try:
        with open(fd, "w", encoding="utf-8") as f:
            f.write(text)
        Path(tmp_name).replace(p)
    finally:
        if Path(tmp_name).exists():
            Path(tmp_name).unlink(missing_ok=True)


def set_label(path: Path, order: list[str], candidate_id: str, label: str) -> dict[str, str]:
    """Read-modify-write a single label into labels.yaml. Returns the new map."""
    if label not in ("pass", "fail", "TODO"):
        raise ValueError(f"invalid label: {label!r}")
    if candidate_id not in order:
        raise ValueError(f"unknown candidate id: {candidate_id!r}")
    header = read_header(path)
    labels = read_labels(path)
    labels[candidate_id] = label
    write_labels_atomic(path, labels, order, header=header)
    return labels
