"""HTTP application for the judge-calibration grader (judge-first, v2).

Stdlib-only (http.server), bound to 127.0.0.1 exclusively by the caller in
server.py -- this app must never be reachable off the local machine, since
the mined data contains verbatim excerpts of the user's private/client
codebases.

Security note (judge-id AND candidate-id allowlist): every route that takes
a judge id and/or candidate id (``/api/judge``, ``/api/cell``,
``/api/grade``) validates the judge id against ``JudgeRegistry.is_known_id``
(built from the real ``evals/judges.yaml``) and the (judge id, candidate id)
pair against ``MatrixStore.is_known_pair`` (built from the real
``mined/matrix.jsonl``) *before* any filesystem access. A conversation
draft's path is only ever taken from the already-loaded, trusted matrix
record (``MatrixStore.resolve_conversation_draft_path``) -- never
reconstructed by concatenating raw request input into a path -- so a
path-traversal-shaped id has nothing to traverse with; it is rejected with
404 at the allowlist check, before ``resolve_conversation_draft_path`` (or
any other file read) ever runs.
"""
from __future__ import annotations

import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

from . import conversation_draft, labels_store
from .judges import JudgeRegistry
from .matrix import MatrixStore

STATIC_FILES = {
    "/": ("index.html", "text/html; charset=utf-8"),
    "/index.html": ("index.html", "text/html; charset=utf-8"),
    "/app.js": ("app.js", "application/javascript; charset=utf-8"),
    "/style.css": ("style.css", "text/css; charset=utf-8"),
}

VALID_VERDICT_INPUTS = ("pass", "fail")


class AppContext:
    """Shared, lock-guarded application state for one running server."""

    def __init__(self, judges: JudgeRegistry, matrix: MatrixStore, evals_dir: Path, static_dir: Path):
        self.judges = judges
        self.matrix = matrix
        self.evals_dir = Path(evals_dir)
        self.static_dir = Path(static_dir)
        self.labels_path = self.evals_dir / "labels.yaml"
        self.lock = threading.Lock()

    # -- read helpers -----------------------------------------------------
    def labels(self) -> dict[str, str]:
        return labels_store.read_labels(self.labels_path)

    # -- judge summaries ----------------------------------------------------
    def judge_summary(self, judge_id: str, labels: dict[str, str] | None = None) -> dict:
        judge = self.judges.get(judge_id)
        cells = self.matrix.cells_for_judge(judge_id)
        labels = self.labels() if labels is None else labels
        graded = sum(
            1 for c in cells if labels_store.composite_key(judge_id, c["candidate_id"]) in labels
        )
        total = len(cells)
        return {
            "judge_id": judge_id,
            "suite": judge["suite"],
            "case_name": judge["case_name"],
            "needs_human_review": judge["needs_human_review"],
            "total_cells": total,
            "graded_cells": graded,
            "remaining_cells": total - graded,
            "pickable": (total - graded) > 0,
        }

    def list_judges(self) -> list[dict]:
        """Every judge with at least one relevant cell in matrix.jsonl right
        now, each annotated with its grading progress. The UI shows
        ``pickable`` judges (>=1 ungraded cell) prominently and any
        fully-graded judge in a clearly-labeled "done" section, rather than
        having a judge silently vanish once its last cell is graded."""
        labels = self.labels()
        return [self.judge_summary(jid, labels) for jid in sorted(self.matrix.judge_ids)]

    # -- judge detail (prompt + its cells) ---------------------------------
    def judge_detail(self, judge_id: str) -> dict | None:
        if not self.judges.is_known_id(judge_id):
            return None
        judge = self.judges.get(judge_id)
        labels = self.labels()
        cells = []
        for cell in self.matrix.cells_for_judge(judge_id):
            key = labels_store.composite_key(judge_id, cell["candidate_id"])
            cells.append(
                {
                    "candidate_id": cell["candidate_id"],
                    "matched_keyword": cell.get("matched_keyword"),
                    "matched_tag": cell.get("matched_tag"),
                    "narrowing": cell.get("narrowing"),
                    "source": cell.get("source"),
                    "project_cwd": cell.get("project_cwd"),
                    "repaired": cell.get("repaired"),
                    "verdict": labels.get(key),
                }
            )
        return {
            "judge_id": judge_id,
            "suite": judge["suite"],
            "suite_file": judge["suite_file"],
            "case_name": judge["case_name"],
            "case_when": judge["case_when"],
            "prompt": judge["prompt"],
            "keywords": judge["keywords"],
            "needs_human_review": judge["needs_human_review"],
            "cells": cells,
        }

    # -- one matrix cell (the (user, assistant) pair to grade) -------------
    def cell_detail(self, judge_id: str, candidate_id: str) -> dict | None:
        if not self.judges.is_known_id(judge_id):
            return None
        cell = self.matrix.cell(judge_id, candidate_id)
        if cell is None:
            return None
        judge = self.judges.get(judge_id)
        draft_path = self.matrix.resolve_conversation_draft_path(cell)
        parsed = conversation_draft.load_conversation_draft(draft_path)
        user_text, assistant_text = conversation_draft.user_and_assistant_text(parsed)
        labels = self.labels()
        key = labels_store.composite_key(judge_id, candidate_id)
        return {
            "judge": {
                "judge_id": judge_id,
                "suite": judge["suite"],
                "case_name": judge["case_name"],
                "prompt": judge["prompt"],
            },
            "cell": {
                "candidate_id": candidate_id,
                "matched_keyword": cell.get("matched_keyword"),
                "matched_tag": cell.get("matched_tag"),
                "narrowing": cell.get("narrowing"),
                "source": cell.get("source"),
                "source_file": cell.get("source_file"),
                "project_cwd": cell.get("project_cwd"),
                "repaired": cell.get("repaired"),
            },
            "model": parsed.get("model"),
            "user": user_text,
            "assistant": assistant_text,
            "verdict": labels.get(key),
        }

    # -- mutation (lock-guarded + atomic file write) ------------------------
    def grade(self, judge_id: str, candidate_id: str, verdict_input: str) -> dict:
        if verdict_input not in VALID_VERDICT_INPUTS:
            raise ValueError(f"verdict must be one of {VALID_VERDICT_INPUTS}")
        if not self.judges.is_known_id(judge_id) or not self.matrix.is_known_pair(
            judge_id, candidate_id
        ):
            raise KeyError((judge_id, candidate_id))
        verdict = "Pass" if verdict_input == "pass" else "Fail"
        with self.lock:
            labels_store.set_verdict(self.labels_path, judge_id, candidate_id, verdict)
            summary = self.judge_summary(judge_id)
        return {"judge": summary, "verdict": verdict}


def _json_bytes(obj) -> bytes:
    return json.dumps(obj).encode("utf-8")


def make_handler(ctx: AppContext):
    class Handler(BaseHTTPRequestHandler):
        server_version = "JudgeCalibrationGrader/2.0"

        def log_message(self, fmt, *args):  # quieter default logging
            pass

        def _send_json(self, status: int, obj) -> None:
            body = _json_bytes(obj)
            self.send_response(status)
            self.send_header("Content-Type", "application/json; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def _send_text(self, status: int, text: str, content_type: str) -> None:
            body = text.encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", content_type)
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def _read_json_body(self) -> dict:
            length = int(self.headers.get("Content-Length", 0) or 0)
            if length == 0:
                return {}
            raw = self.rfile.read(length)
            try:
                return json.loads(raw.decode("utf-8"))
            except json.JSONDecodeError:
                return {}

        # -- routing ---------------------------------------------------
        def do_GET(self):
            parsed = urlparse(self.path)
            path = parsed.path
            qs = parse_qs(parsed.query)

            if path in STATIC_FILES:
                fname, ctype = STATIC_FILES[path]
                fpath = ctx.static_dir / fname
                if not fpath.exists():
                    self._send_text(404, "not found", "text/plain")
                    return
                self._send_text(200, fpath.read_text(encoding="utf-8"), ctype)
                return

            if path == "/api/judges":
                self._send_json(200, {"judges": ctx.list_judges()})
                return

            if path == "/api/judge":
                judge_id = (qs.get("judge_id") or [None])[0]
                detail = ctx.judge_detail(judge_id) if judge_id else None
                if detail is None:
                    self._send_json(404, {"error": "unknown judge_id"})
                    return
                self._send_json(200, detail)
                return

            if path == "/api/cell":
                judge_id = (qs.get("judge_id") or [None])[0]
                candidate_id = (qs.get("candidate_id") or [None])[0]
                detail = (
                    ctx.cell_detail(judge_id, candidate_id)
                    if judge_id and candidate_id
                    else None
                )
                if detail is None:
                    self._send_json(404, {"error": "unknown judge_id/candidate_id pair"})
                    return
                self._send_json(200, detail)
                return

            self._send_text(404, "not found", "text/plain")

        def do_POST(self):
            parsed = urlparse(self.path)
            path = parsed.path
            body = self._read_json_body()

            if path == "/api/grade":
                judge_id = body.get("judge_id")
                candidate_id = body.get("candidate_id")
                verdict = body.get("verdict")
                if (
                    not isinstance(judge_id, str)
                    or not isinstance(candidate_id, str)
                    or not ctx.judges.is_known_id(judge_id)
                    or not ctx.matrix.is_known_pair(judge_id, candidate_id)
                ):
                    self._send_json(404, {"error": "unknown judge_id/candidate_id pair"})
                    return
                try:
                    result = ctx.grade(judge_id, candidate_id, verdict)
                except ValueError as exc:
                    self._send_json(400, {"error": str(exc)})
                    return
                self._send_json(200, result)
                return

            self._send_text(404, "not found", "text/plain")

    return Handler


def build_server(ctx: AppContext, host: str, port: int) -> ThreadingHTTPServer:
    if host != "127.0.0.1" and host != "localhost":
        raise ValueError("refusing to bind to a non-loopback host: mined data must stay local-only")
    handler = make_handler(ctx)
    return ThreadingHTTPServer((host, port), handler)
