"""In-process integration tests: actually run the HTTP app (stdlib
ThreadingHTTPServer bound to 127.0.0.1 on an ephemeral port) and exercise it
over real HTTP via urllib, including the path-traversal / id-allowlist
security requirement.
"""
import json
import shutil
import tempfile
import threading
import unittest
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

from backend.app import AppContext, build_server
from backend.candidates import CandidateStore
from backend import labels_store

FIXTURE_RECORDS = [
    {
        "id": "claude-code-fixture-aaa-001",
        "source": "claude-code",
        "source_file": "/tmp/x.jsonl",
        "session_id": "s1",
        "project_cwd": "/Users/x/proj",
        "turn_started_at": "2026-01-01T00:00:00Z",
        "invocation_signal": "test",
        "model": "claude-opus-4-8",
        "question": "Please refactor the auth middleware for clarity.",
        "response": "Sure, I refactored the auth middleware for clarity and added tests.",
        "agent_id": None,
        "candidate_label_human_implied": None,
        "label": "TODO",
    },
    {
        "id": "codex-fixture-bbb-002",
        "source": "codex",
        "source_file": "/tmp/y.jsonl",
        "session_id": "s2",
        "project_cwd": "/Users/x/proj2",
        "turn_started_at": "2026-01-02T00:00:00Z",
        "invocation_signal": "test",
        "model": "gpt-5-codex",
        "question": "Looks good, ship it.",
        "response": "Great, merging now.",
        "agent_id": None,
        "candidate_label_human_implied": {
            "verdict": "pass",
            "evidence_phrase": "looks good",
            "confidence": "low",
            "note": "heuristic hint, not confirmed",
        },
        "label": "TODO",
    },
]

LABELS_YAML_TEXT = (
    "# DRAFT — fixture labels\n\n"
    "claude-code-fixture-aaa-001: TODO\n"
    "codex-fixture-bbb-002: TODO\n"
)


class AppIntegrationTest(unittest.TestCase):
    def setUp(self):
        self.tmpdir = Path(tempfile.mkdtemp())
        self.mined_dir = self.tmpdir / "mined"
        self.evals_dir = self.tmpdir / "evals"
        self.static_dir = Path(__file__).resolve().parent.parent / "static"
        self.mined_dir.mkdir(parents=True)
        (self.mined_dir / "conversations").mkdir()

        candidates_path = self.mined_dir / "candidates.jsonl"
        with open(candidates_path, "w", encoding="utf-8") as f:
            for r in FIXTURE_RECORDS:
                f.write(json.dumps(r) + "\n")
        (self.mined_dir / "labels.yaml").write_text(LABELS_YAML_TEXT, encoding="utf-8")
        (self.mined_dir / "conversations" / "claude-code-fixture-aaa-001.yaml").write_text(
            "---\nrole: user\ncontent: fixture\n", encoding="utf-8"
        )

        store = CandidateStore(candidates_path)
        self.ctx = AppContext(store, mined_dir=self.mined_dir, evals_dir=self.evals_dir, static_dir=self.static_dir)
        self.httpd = build_server(self.ctx, "127.0.0.1", 0)
        self.port = self.httpd.server_address[1]
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)
        self.thread.start()

    def tearDown(self):
        self.httpd.shutdown()
        self.httpd.server_close()
        self.thread.join(timeout=5)
        shutil.rmtree(self.tmpdir, ignore_errors=True)

    def _url(self, path):
        return f"http://127.0.0.1:{self.port}{path}"

    def _get_json(self, path):
        try:
            with urllib.request.urlopen(self._url(path)) as resp:
                return resp.status, json.loads(resp.read())
        except urllib.error.HTTPError as e:
            body = e.read()
            return e.code, (json.loads(body) if body else None)

    def _post_json(self, path, payload):
        data = json.dumps(payload).encode("utf-8")
        req = urllib.request.Request(self._url(path), data=data, method="POST", headers={"Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(req) as resp:
                return resp.status, json.loads(resp.read())
        except urllib.error.HTTPError as e:
            return e.code, json.loads(e.read())

    # -- basic API shape --------------------------------------------------
    def test_binds_loopback_only(self):
        self.assertEqual(self.httpd.server_address[0], "127.0.0.1")

    def test_state_reflects_fixture(self):
        status, state = self._get_json("/api/state")
        self.assertEqual(status, 200)
        self.assertEqual(state["total"], 2)
        self.assertEqual(state["graded"], 0)
        self.assertEqual(state["remaining"], 2)

    def test_candidates_list_contains_both_fixtures(self):
        status, body = self._get_json("/api/candidates")
        self.assertEqual(status, 200)
        ids = {c["id"] for c in body["candidates"]}
        self.assertEqual(ids, {"claude-code-fixture-aaa-001", "codex-fixture-bbb-002"})

    def test_candidates_list_source_filter(self):
        _, body = self._get_json("/api/candidates?source=codex")
        ids = {c["id"] for c in body["candidates"]}
        self.assertEqual(ids, {"codex-fixture-bbb-002"})

    def test_implied_hint_surfaces_in_detail(self):
        status, detail = self._get_json("/api/candidate/codex-fixture-bbb-002")
        self.assertEqual(status, 200)
        self.assertEqual(detail["candidate_label_human_implied"]["verdict"], "pass")

    def test_static_index_served(self):
        with urllib.request.urlopen(self._url("/")) as resp:
            body = resp.read().decode("utf-8")
            self.assertEqual(resp.status, 200)
            self.assertIn("Judge Calibration Grader", body)

    # -- security: id allowlist / no path traversal ------------------------
    def test_unknown_candidate_id_is_404(self):
        status, _ = self._get_json("/api/candidate/does-not-exist")
        self.assertEqual(status, 404)

    def test_path_traversal_id_rejected_without_touching_filesystem(self):
        traversal_ids = [
            "../../../../etc/passwd",
            "..%2F..%2Fetc%2Fpasswd",
            "....//....//etc/passwd",
            "a/../../secrets",
        ]
        for tid in traversal_ids:
            with self.subTest(tid=tid):
                url = self._url(f"/api/candidate/{urllib.parse.quote(tid, safe='')}/conversation")
                try:
                    with urllib.request.urlopen(url) as resp:
                        status = resp.status
                except urllib.error.HTTPError as e:
                    status = e.code
                self.assertEqual(status, 404)

    def test_grade_unknown_id_rejected(self):
        status, body = self._post_json("/api/grade", {"id": "nope", "label": "pass"})
        self.assertEqual(status, 404)

    def test_grade_invalid_label_rejected(self):
        status, body = self._post_json("/api/grade", {"id": "claude-code-fixture-aaa-001", "label": "maybe"})
        self.assertEqual(status, 400)

    # -- grading writes labels.yaml immediately ----------------------------
    def test_grade_writes_labels_yaml(self):
        status, body = self._post_json("/api/grade", {"id": "claude-code-fixture-aaa-001", "label": "pass"})
        self.assertEqual(status, 200)
        on_disk = labels_store.read_labels(self.mined_dir / "labels.yaml")
        self.assertEqual(on_disk["claude-code-fixture-aaa-001"], "pass")

        status, state = self._get_json("/api/state")
        self.assertEqual(state["graded"], 1)
        self.assertEqual(state["remaining"], 1)

    def test_include_toggle_persists(self):
        self._post_json("/api/grade", {"id": "claude-code-fixture-aaa-001", "label": "pass"})
        status, _ = self._post_json("/api/include", {"id": "claude-code-fixture-aaa-001", "included": True})
        self.assertEqual(status, 200)
        included = json.loads((self.mined_dir / "included.json").read_text(encoding="utf-8"))
        self.assertTrue(included["claude-code-fixture-aaa-001"])

    def test_export_only_writes_graded_and_included(self):
        self._post_json("/api/grade", {"id": "claude-code-fixture-aaa-001", "label": "pass"})
        self._post_json("/api/grade", {"id": "codex-fixture-bbb-002", "label": "fail"})
        self._post_json("/api/include", {"id": "claude-code-fixture-aaa-001", "included": True})
        # codex-fixture-bbb-002 graded but NOT marked included -> must be excluded.
        status, result = self._post_json("/api/export", {})
        self.assertEqual(status, 200)
        self.assertEqual(result["exported_count"], 1)
        exported = labels_store.read_labels(self.evals_dir / "labels.yaml")
        self.assertEqual(exported, {"claude-code-fixture-aaa-001": "pass"})


if __name__ == "__main__":
    unittest.main()
