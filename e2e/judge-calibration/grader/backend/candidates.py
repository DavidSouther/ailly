"""Load and index mined/candidates.jsonl.

The loaded id set doubles as the security allowlist: any endpoint that
accepts a candidate id as input must check membership here *before* doing
anything filesystem-related (see ``backend/app.py``'s use of
``CandidateStore.is_known_id``) so a crafted id can never be interpolated
into a path.
"""
from __future__ import annotations

import json
from pathlib import Path
from typing import Any


class CandidateStore:
    def __init__(self, jsonl_path: Path):
        self.jsonl_path = Path(jsonl_path)
        self._by_id: dict[str, dict[str, Any]] = {}
        self._order: list[str] = []
        self.reload()

    def reload(self) -> None:
        by_id: dict[str, dict[str, Any]] = {}
        order: list[str] = []
        with open(self.jsonl_path, "r", encoding="utf-8") as f:
            for line_no, line in enumerate(f, start=1):
                line = line.strip()
                if not line:
                    continue
                try:
                    record = json.loads(line)
                except json.JSONDecodeError as exc:
                    raise ValueError(f"malformed JSON on line {line_no} of {self.jsonl_path}") from exc
                cid = record.get("id")
                if not cid:
                    raise ValueError(f"candidate on line {line_no} missing 'id'")
                if cid in by_id:
                    raise ValueError(f"duplicate candidate id: {cid!r}")
                by_id[cid] = record
                order.append(cid)
        self._by_id = by_id
        self._order = order

    @property
    def order(self) -> list[str]:
        """Canonical candidate ordering, as they appear in candidates.jsonl."""
        return list(self._order)

    @property
    def ids(self) -> set[str]:
        """The id allowlist -- the only ids any endpoint may resolve to a file."""
        return set(self._by_id.keys())

    def is_known_id(self, candidate_id: str) -> bool:
        return candidate_id in self._by_id

    def get(self, candidate_id: str) -> dict[str, Any] | None:
        return self._by_id.get(candidate_id)

    def __len__(self) -> int:
        return len(self._order)

    def all(self) -> list[dict[str, Any]]:
        return [self._by_id[cid] for cid in self._order]

    def sources(self) -> list[str]:
        seen = []
        for cid in self._order:
            s = self._by_id[cid].get("source")
            if s not in seen:
                seen.append(s)
        return seen
