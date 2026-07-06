"""HTTP application for the judge-calibration grader.

Stdlib-only (http.server), bound to 127.0.0.1 exclusively by the caller in
server.py -- this app must never be reachable off the local machine, since
the mined data contains verbatim excerpts of the user's private/client
codebases.

Security note (candidate-id allowlist): every route that takes a candidate
id (``/api/candidate/<id>``, its ``/conversation`` sub-route, ``/api/grade``,
``/api/include``) validates the id against ``CandidateStore.is_known_id``
-- built from the actual ids present in candidates.jsonl at load time --
*before* it is ever used to build a filesystem path. An id that isn't in
that allowlist is rejected with 404 and never touches the filesystem.
"""
from __future__ import annotations

import json
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import parse_qs, urlparse

from . import export, included_store, labels_store, suggestions_store

STATIC_FILES = {
    "/": ("index.html", "text/html; charset=utf-8"),
    "/index.html": ("index.html", "text/html; charset=utf-8"),
    "/app.js": ("app.js", "application/javascript; charset=utf-8"),
    "/style.css": ("style.css", "text/css; charset=utf-8"),
}


class AppContext:
    """Shared, lock-guarded application state for one running server."""

    def __init__(self, candidate_store, mined_dir: Path, evals_dir: Path, static_dir: Path):
        self.store = candidate_store
        self.mined_dir = Path(mined_dir)
        self.evals_dir = Path(evals_dir)
        self.static_dir = Path(static_dir)
        self.labels_path = self.mined_dir / "labels.yaml"
        self.included_path = self.mined_dir / "included.json"
        self.suggestions_path = self.mined_dir / "suggestions.json"
        self.export_path = self.evals_dir / "labels.yaml"
        self.lock = threading.Lock()

    # -- read helpers -----------------------------------------------------
    def labels(self) -> dict[str, str]:
        return labels_store.read_labels(self.labels_path)

    def included(self) -> dict[str, bool]:
        return included_store.read_included(self.included_path)

    def suggestions_cache(self) -> dict:
        return suggestions_store.read_suggestions_cache(self.suggestions_path)

    def texts_by_id(self) -> dict[str, str]:
        return {
            cid: (self.store.get(cid) or {}).get("response", "")
            for cid in self.store.order
        }

    # -- aggregate state ----------------------------------------------------
    def state(self) -> dict:
        labels = self.labels()
        total = len(self.store)
        graded = sum(1 for v in labels.values() if v in ("pass", "fail"))
        passed = sum(1 for v in labels.values() if v == "pass")
        failed = sum(1 for v in labels.values() if v == "fail")
        cache = self.suggestions_cache()
        included = self.included()
        by_source: dict[str, dict] = {}
        for cid in self.store.order:
            rec = self.store.get(cid)
            src = rec.get("source")
            bucket = by_source.setdefault(src, {"total": 0, "graded": 0})
            bucket["total"] += 1
            if labels.get(cid) in ("pass", "fail"):
                bucket["graded"] += 1
        return {
            "total": total,
            "graded": graded,
            "remaining": total - graded,
            "passed": passed,
            "failed": failed,
            "included_count": sum(1 for v in included.values() if v),
            "by_source": by_source,
            "suggestions_computed_at_grade_count": cache.get("computed_at_grade_count", 0),
            "suggestions_generated_at": cache.get("generated_at"),
            "suggestions_count": len(cache.get("suggestions", {})),
            "next_suggestion_recompute_at": (
                (cache.get("computed_at_grade_count", 0) + suggestions_store.CADENCE)
                if graded >= suggestions_store.CADENCE or cache.get("computed_at_grade_count", 0) > 0
                else suggestions_store.CADENCE
            ),
        }

    # -- listing --------------------------------------------------------
    def list_candidates(self, source: str | None, status: str | None, q: str | None) -> list[dict]:
        labels = self.labels()
        included = self.included()
        cache = self.suggestions_cache().get("suggestions", {})
        out = []
        for cid in self.store.order:
            rec = self.store.get(cid)
            label = labels.get(cid, "TODO")
            if source and rec.get("source") != source:
                continue
            if status == "graded" and label not in ("pass", "fail"):
                continue
            if status == "ungraded" and label in ("pass", "fail"):
                continue
            if q and q.lower() not in cid.lower():
                continue
            out.append({
                "id": cid,
                "source": rec.get("source"),
                "model": rec.get("model"),
                "project_cwd": rec.get("project_cwd"),
                "turn_started_at": rec.get("turn_started_at"),
                "label": label,
                "included": bool(included.get(cid)),
                "has_implied": rec.get("candidate_label_human_implied") is not None,
                "suggestion": cache.get(cid),
            })
        return out

    def candidate_detail(self, cid: str) -> dict | None:
        if not self.store.is_known_id(cid):
            return None
        rec = self.store.get(cid)
        labels = self.labels()
        included = self.included()
        cache = self.suggestions_cache().get("suggestions", {})
        detail = dict(rec)
        detail["label"] = labels.get(cid, "TODO")
        detail["included"] = bool(included.get(cid))
        detail["suggestion"] = cache.get(cid)
        return detail

    def conversation_text(self, cid: str) -> str | None:
        if not self.store.is_known_id(cid):
            return None
        # cid is now proven to be a member of the id allowlist derived from
        # candidates.jsonl -- safe to use in a path join.
        path = self.mined_dir / "conversations" / f"{cid}.yaml"
        if not path.exists():
            return ""
        return path.read_text(encoding="utf-8")

    # -- mutations (all lock-guarded + atomic file writes) -----------------
    def grade(self, cid: str, label: str) -> dict:
        if label not in ("pass", "fail"):
            raise ValueError("label must be 'pass' or 'fail'")
        if not self.store.is_known_id(cid):
            raise KeyError(cid)
        with self.lock:
            labels_store.set_label(self.labels_path, self.store.order, cid, label)
            new_labels = self.labels()
            cache = suggestions_store.maybe_recompute(
                self.texts_by_id(), new_labels, self.suggestions_path
            )
        return {"state": self.state(), "suggestions_recomputed": cache is not None}

    def set_included(self, cid: str, included_flag: bool) -> dict:
        if not self.store.is_known_id(cid):
            raise KeyError(cid)
        with self.lock:
            included_store.set_included(self.included_path, cid, self.store.ids, included_flag)
        return self.state()

    def force_recompute(self) -> dict:
        with self.lock:
            cache = suggestions_store.recompute(self.texts_by_id(), self.labels(), self.suggestions_path)
        return cache

    def export(self) -> dict:
        with self.lock:
            curated = export.export_curated_labels(
                self.store.order, self.labels(), self.included(), self.export_path
            )
        return {"exported_count": len(curated), "path": str(self.export_path), "ids": sorted(curated)}


def _json_bytes(obj) -> bytes:
    return json.dumps(obj).encode("utf-8")


def make_handler(ctx: AppContext):
    class Handler(BaseHTTPRequestHandler):
        server_version = "JudgeCalibrationGrader/1.0"

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

            if path == "/api/state":
                self._send_json(200, ctx.state())
                return

            if path == "/api/candidates":
                source = (qs.get("source") or [None])[0]
                status = (qs.get("status") or [None])[0]
                q = (qs.get("q") or [None])[0]
                self._send_json(200, {"candidates": ctx.list_candidates(source, status, q)})
                return

            if path.startswith("/api/candidate/"):
                rest = path[len("/api/candidate/"):]
                if rest.endswith("/conversation"):
                    cid = rest[: -len("/conversation")]
                    text = ctx.conversation_text(cid)
                    if text is None:
                        self._send_json(404, {"error": "unknown candidate id"})
                        return
                    self._send_text(200, text, "text/plain; charset=utf-8")
                    return
                cid = rest
                detail = ctx.candidate_detail(cid)
                if detail is None:
                    self._send_json(404, {"error": "unknown candidate id"})
                    return
                self._send_json(200, detail)
                return

            self._send_text(404, "not found", "text/plain")

        def do_POST(self):
            parsed = urlparse(self.path)
            path = parsed.path
            body = self._read_json_body()

            if path == "/api/grade":
                cid = body.get("id")
                label = body.get("label")
                if not isinstance(cid, str) or not ctx.store.is_known_id(cid):
                    self._send_json(404, {"error": "unknown candidate id"})
                    return
                try:
                    result = ctx.grade(cid, label)
                except ValueError as exc:
                    self._send_json(400, {"error": str(exc)})
                    return
                self._send_json(200, result)
                return

            if path == "/api/include":
                cid = body.get("id")
                included_flag = bool(body.get("included"))
                if not isinstance(cid, str) or not ctx.store.is_known_id(cid):
                    self._send_json(404, {"error": "unknown candidate id"})
                    return
                state = ctx.set_included(cid, included_flag)
                self._send_json(200, state)
                return

            if path == "/api/recompute-suggestions":
                cache = ctx.force_recompute()
                self._send_json(200, cache)
                return

            if path == "/api/export":
                result = ctx.export()
                self._send_json(200, result)
                return

            self._send_text(404, "not found", "text/plain")

    return Handler


def build_server(ctx: AppContext, host: str, port: int) -> ThreadingHTTPServer:
    if host != "127.0.0.1" and host != "localhost":
        raise ValueError("refusing to bind to a non-loopback host: mined data must stay local-only")
    handler = make_handler(ctx)
    return ThreadingHTTPServer((host, port), handler)
