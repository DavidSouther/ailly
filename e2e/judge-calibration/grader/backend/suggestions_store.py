"""Cache + cadence logic for the heuristic pre-fill suggestions.

Persisted to mined/suggestions.json (gitignored, since it lives under
mined/) so suggestions survive a server restart without re-deriving them
every boot. Kept entirely separate from labels.yaml, which stays
ground-truth-only.
"""
from __future__ import annotations

import json
import tempfile
import time
from pathlib import Path

from . import similarity

CADENCE = 20  # recompute after every ~20 new real (pass/fail) grades


def read_suggestions_cache(path: Path) -> dict:
    p = Path(path)
    if not p.exists():
        return {"computed_at_grade_count": 0, "generated_at": None, "suggestions": {}}
    try:
        data = json.loads(p.read_text(encoding="utf-8"))
    except json.JSONDecodeError:
        return {"computed_at_grade_count": 0, "generated_at": None, "suggestions": {}}
    data.setdefault("computed_at_grade_count", 0)
    data.setdefault("generated_at", None)
    data.setdefault("suggestions", {})
    return data


def write_suggestions_cache_atomic(path: Path, cache: dict) -> None:
    p = Path(path)
    p.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(cache, indent=2, sort_keys=True) + "\n"
    fd, tmp_name = tempfile.mkstemp(dir=str(p.parent), prefix=".suggestions-", suffix=".tmp")
    try:
        with open(fd, "w", encoding="utf-8") as f:
            f.write(text)
        Path(tmp_name).replace(p)
    finally:
        if Path(tmp_name).exists():
            Path(tmp_name).unlink(missing_ok=True)


def graded_count(labels: dict[str, str]) -> int:
    return sum(1 for v in labels.values() if v in ("pass", "fail"))


def should_recompute(labels: dict[str, str], cache: dict, cadence: int = CADENCE) -> bool:
    """True once ~20 real grades have accrued since the last computed cache."""
    n = graded_count(labels)
    if n < cadence:
        return False
    return (n - cache.get("computed_at_grade_count", 0)) >= cadence


def recompute(
    texts: dict[str, str],
    labels: dict[str, str],
    path: Path,
    k: int = 5,
    sim_threshold: float = 0.12,
    majority_threshold: float = 0.6,
) -> dict:
    """Recompute suggestions unconditionally and persist to ``path``."""
    suggestions = similarity.compute_suggestions(
        texts, labels, k=k, sim_threshold=sim_threshold, majority_threshold=majority_threshold
    )
    cache = {
        "computed_at_grade_count": graded_count(labels),
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "suggestions": suggestions,
    }
    write_suggestions_cache_atomic(path, cache)
    return cache


def maybe_recompute(
    texts: dict[str, str],
    labels: dict[str, str],
    path: Path,
    cadence: int = CADENCE,
    **kwargs,
) -> dict | None:
    """Recompute-if-due. Returns the new cache dict, or None if not due yet."""
    cache = read_suggestions_cache(path)
    if not should_recompute(labels, cache, cadence=cadence):
        return None
    return recompute(texts, labels, path, **kwargs)
