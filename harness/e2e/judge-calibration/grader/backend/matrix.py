"""Load mined/matrix.jsonl: one line per relevant (judge, candidate) cell.

Unlike judges.yaml, this is plain JSON Lines (one ``json.loads`` per line --
no hand-rolled YAML needed), produced by a separate, already-run pipeline
stage (``build_relevance_matrix.py``, in a different worktree). The file is
read fresh at server start; nothing here hardcodes an expected row count --
today's 5-line file and a much larger future file (once the parallel
synthetic-dataset work lands) are both just "however many lines are in the
file right now."

The ``candidate_id`` set observed across all cells doubles as the
candidate-id half of the security allowlist (see ``backend/app.py``): a
``candidate_id`` is only ever resolved to a filesystem path via the
``conversation_draft`` field already present on a loaded cell record, never
by concatenating raw user input into a path.
"""
from __future__ import annotations

import json
from pathlib import Path
from typing import Any

# Cell records are written with this prefix on conversation_draft (relative
# to the repo root). We resolve it against our own local mined/ copy rather
# than reconstructing a full repo-root path, so a cell's file is only ever
# read from inside the mined/ directory this app already trusts.
_CONVERSATION_DRAFT_PREFIX = "e2e/judge-calibration/mined/"


class MatrixStore:
    """Loaded matrix.jsonl cells, indexed by judge_id and by (judge_id, candidate_id)."""

    def __init__(self, jsonl_path: Path, mined_dir: Path):
        self.jsonl_path = Path(jsonl_path)
        self.mined_dir = Path(mined_dir)
        self._cells: list[dict[str, Any]] = []
        self._by_judge: dict[str, list[dict[str, Any]]] = {}
        self._by_pair: dict[tuple[str, str], dict[str, Any]] = {}
        self.reload()

    def reload(self) -> None:
        cells: list[dict[str, Any]] = []
        by_judge: dict[str, list[dict[str, Any]]] = {}
        by_pair: dict[tuple[str, str], dict[str, Any]] = {}
        if self.jsonl_path.exists():
            with open(self.jsonl_path, "r", encoding="utf-8") as f:
                for line_no, line in enumerate(f, start=1):
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        record = json.loads(line)
                    except json.JSONDecodeError as exc:
                        raise ValueError(
                            f"malformed JSON on line {line_no} of {self.jsonl_path}"
                        ) from exc
                    judge_id = record.get("judge_id")
                    candidate_id = record.get("candidate_id")
                    if not judge_id or not candidate_id:
                        raise ValueError(
                            f"matrix cell on line {line_no} missing judge_id/candidate_id"
                        )
                    key = (judge_id, candidate_id)
                    if key in by_pair:
                        raise ValueError(f"duplicate matrix cell: {key!r}")
                    cells.append(record)
                    by_judge.setdefault(judge_id, []).append(record)
                    by_pair[key] = record
        self._cells = cells
        self._by_judge = by_judge
        self._by_pair = by_pair

    @property
    def judge_ids(self) -> set[str]:
        """Judge ids that have at least one relevant cell right now."""
        return set(self._by_judge.keys())

    @property
    def candidate_ids(self) -> set[str]:
        """The candidate-id allowlist -- every id that appears in some cell."""
        return {cid for (_jid, cid) in self._by_pair.keys()}

    def cells_for_judge(self, judge_id: str) -> list[dict[str, Any]]:
        return list(self._by_judge.get(judge_id, []))

    def all_cells(self) -> list[dict[str, Any]]:
        """Every cell, in matrix.jsonl line order (used by
        ``backend/precheck.py`` to walk the whole matrix once)."""
        return list(self._cells)

    def cell(self, judge_id: str, candidate_id: str) -> dict[str, Any] | None:
        return self._by_pair.get((judge_id, candidate_id))

    def is_known_pair(self, judge_id: str, candidate_id: str) -> bool:
        return (judge_id, candidate_id) in self._by_pair

    def resolve_conversation_draft_path(self, cell: dict[str, Any]) -> Path:
        """Resolve a loaded cell's ``conversation_draft`` field to a real
        path under this app's own ``mined_dir`` -- never built from raw
        candidate/judge id input, only from the already-loaded, trusted
        matrix.jsonl record.
        """
        draft = cell["conversation_draft"]
        if not draft.startswith(_CONVERSATION_DRAFT_PREFIX):
            raise ValueError(f"unexpected conversation_draft path shape: {draft!r}")
        relative = draft[len(_CONVERSATION_DRAFT_PREFIX) :]
        return self.mined_dir / relative

    def __len__(self) -> int:
        return len(self._cells)
