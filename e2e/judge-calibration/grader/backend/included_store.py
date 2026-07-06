"""Read/write for mined/included.json.

Tracks the human's explicit "include this graded candidate in the curated
calibration set" toggle. Deliberately kept separate from labels.yaml, which
stays ground-truth-grade-only, and separate from suggestions.json, which is
a disposable heuristic cache.
"""
from __future__ import annotations

import json
import tempfile
from pathlib import Path


def read_included(path: Path) -> dict[str, bool]:
    p = Path(path)
    if not p.exists():
        return {}
    try:
        data = json.loads(p.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return {}
    if not isinstance(data, dict):
        return {}
    return {k: bool(v) for k, v in data.items()}


def write_included_atomic(path: Path, included: dict[str, bool]) -> None:
    p = Path(path)
    p.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(included, indent=2, sort_keys=True) + "\n"
    fd, tmp_name = tempfile.mkstemp(dir=str(p.parent), prefix=".included-", suffix=".tmp")
    try:
        with open(fd, "w", encoding="utf-8") as f:
            f.write(text)
        Path(tmp_name).replace(p)
    finally:
        if Path(tmp_name).exists():
            Path(tmp_name).unlink(missing_ok=True)


def set_included(path: Path, candidate_id: str, valid_ids: set[str], included: bool) -> dict[str, bool]:
    if candidate_id not in valid_ids:
        raise ValueError(f"unknown candidate id: {candidate_id!r}")
    current = read_included(path)
    if included:
        current[candidate_id] = True
    else:
        current.pop(candidate_id, None)
    write_included_atomic(path, current)
    return current
