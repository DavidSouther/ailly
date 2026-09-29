"""In-process integration tests: actually run the HTTP app (stdlib
ThreadingHTTPServer bound to 127.0.0.1 on an ephemeral port) and exercise it
over real HTTP via urllib, including the path-traversal / id-allowlist
security requirement (now covering BOTH judge_id and candidate_id).
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
from backend.judges import JudgeRegistry
from backend.matrix import MatrixStore
from backend.precheck import PrecheckStore
from backend import labels_store

JUDGES_YAML = """\
judges:
- judge_id: patterns-eval/baseline/newtype
  suite: patterns-eval
  suite_file: e2e/patterns-eval/evals/baseline.yaml
  case_name: newtype
  case_when: null
  prompt: 'The code introduces a UserId type that wraps a string.

    '
  keywords:
  - value: patterns:newtype
    polarity: include
    source: sibling-suite-case-name(assembly+suite-convention)
  needs_human_review: false
- judge_id: delegate-52/corruption/case4
  suite: delegate-52
  suite_file: e2e/delegate-52/evals/corruption.yaml
  case_name: null
  case_when: null
  prompt: 'Needs a human -- no keywords matched.

    '
  keywords: []
  needs_human_review: true
"""

CONVERSATION_DRAFT = (
    '---\nmodel: "claude-opus-4-8"  # mined narrow draft\n'
    "---\nrole: user\ncontent: |\n"
    "  Please add a UserId newtype.\n"
    "---\nrole: assistant\ncontent: |\n"
    "  Done -- UserId now wraps a String with validation in its constructor.\n"
)


class AppIntegrationTest(unittest.TestCase):
    def setUp(self):
        self.tmpdir = Path(tempfile.mkdtemp())
        self.mined_dir = self.tmpdir / "mined"
        self.evals_dir = self.tmpdir / "evals"
        self.static_dir = Path(__file__).resolve().parent.parent / "static"
        matrix_dir = self.mined_dir / "matrix" / "patterns-eval__baseline__newtype"
        matrix_dir.mkdir(parents=True)
        self.evals_dir.mkdir(parents=True)

        (matrix_dir / "cand-001.yaml").write_text(CONVERSATION_DRAFT, encoding="utf-8")

        matrix_lines = [
            {
                "judge_id": "patterns-eval/baseline/newtype",
                "candidate_id": "cand-001",
                "matched_keyword": "patterns:newtype",
                "matched_tag": "patterns:newtype",
                "narrowing": "user-prompt",
                "source": "claude-code",
                "source_file": "/tmp/x.jsonl",
                "project_cwd": "/tmp/proj",
                "conversation_draft": "e2e/judge-calibration/mined/matrix/patterns-eval__baseline__newtype/cand-001.yaml",
                "repaired": True,
            }
        ]
        matrix_path = self.mined_dir / "matrix.jsonl"
        with open(matrix_path, "w", encoding="utf-8") as f:
            for rec in matrix_lines:
                f.write(json.dumps(rec) + "\n")

        judges_path = self.evals_dir / "judges.yaml"
        judges_path.write_text(JUDGES_YAML, encoding="utf-8")

        precheck_path = self.mined_dir / "precheck_results.json"
        precheck_path.write_text(
            json.dumps(
                [
                    {
                        "judge_id": "patterns-eval/baseline/newtype",
                        "candidate_id": "cand-001",
                        "suite_file": "e2e/patterns-eval/evals/baseline.yaml",
                        "case_name": "newtype",
                        "content_shape": "scalar",
                        "flattening_applied_for_script_checks": False,
                        "checks": [
                            {
                                "type": "text_contains",
                                "outcome": "pass",
                                "reason": None,
                                "source": {"type": "text_contains", "value": "UserId"},
                            }
                        ],
                        "overall": "all_pass",
                    }
                ]
            ),
            encoding="utf-8",
        )

        judges = JudgeRegistry(judges_path)
        matrix = MatrixStore(matrix_path, mined_dir=self.mined_dir)
        precheck = PrecheckStore(precheck_path)
        self.ctx = AppContext(
            judges, matrix, evals_dir=self.evals_dir, static_dir=self.static_dir, precheck=precheck
        )
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
        req = urllib.request.Request(
            self._url(path), data=data, method="POST", headers={"Content-Type": "application/json"}
        )
        try:
            with urllib.request.urlopen(req) as resp:
                return resp.status, json.loads(resp.read())
        except urllib.error.HTTPError as e:
            return e.code, json.loads(e.read())

    # -- basic shape --------------------------------------------------
    def test_binds_loopback_only(self):
        self.assertEqual(self.httpd.server_address[0], "127.0.0.1")

    def test_static_index_served(self):
        with urllib.request.urlopen(self._url("/")) as resp:
            body = resp.read().decode("utf-8")
            self.assertEqual(resp.status, 200)
            self.assertIn("Judge Calibration Grader", body)

    def test_judges_list_only_includes_judges_with_relevant_cells(self):
        status, body = self._get_json("/api/judges")
        self.assertEqual(status, 200)
        ids = {j["judge_id"] for j in body["judges"]}
        # delegate-52/corruption/case4 has zero matrix cells (no keywords
        # matched anything) -- it must not appear at all.
        self.assertEqual(ids, {"patterns-eval/baseline/newtype"})

    def test_judge_starts_pickable_with_one_ungraded_cell(self):
        status, body = self._get_json("/api/judges")
        judge = body["judges"][0]
        self.assertTrue(judge["pickable"])
        self.assertEqual(judge["remaining_cells"], 1)
        self.assertEqual(judge["graded_cells"], 0)

    def test_judge_detail_includes_full_prompt_and_cells(self):
        status, detail = self._get_json(
            "/api/judge?judge_id=" + urllib.parse.quote("patterns-eval/baseline/newtype", safe="")
        )
        self.assertEqual(status, 200)
        self.assertIn("UserId type that wraps a string", detail["prompt"])
        self.assertEqual(len(detail["cells"]), 1)
        self.assertIsNone(detail["cells"][0]["verdict"])

    def test_cell_detail_surfaces_user_and_assistant_text(self):
        status, detail = self._get_json(
            "/api/cell?judge_id="
            + urllib.parse.quote("patterns-eval/baseline/newtype", safe="")
            + "&candidate_id=cand-001"
        )
        self.assertEqual(status, 200)
        self.assertIn("UserId newtype", detail["user"])
        self.assertIn("wraps a String", detail["assistant"])
        self.assertIsNone(detail["verdict"])

    # -- deterministic pre-check hint (informational only) -------------------
    def test_precheck_returns_cached_record_for_known_pair(self):
        status, body = self._get_json(
            "/api/precheck?judge_id="
            + urllib.parse.quote("patterns-eval/baseline/newtype", safe="")
            + "&candidate_id=cand-001"
        )
        self.assertEqual(status, 200)
        self.assertTrue(body["available"])
        self.assertEqual(body["precheck"]["overall"], "all_pass")
        self.assertEqual(body["precheck"]["checks"][0]["type"], "text_contains")

    def test_precheck_unknown_pair_is_404_same_as_cell(self):
        status, _ = self._get_json(
            "/api/precheck?judge_id="
            + urllib.parse.quote("delegate-52/corruption/case4", safe="")
            + "&candidate_id=cand-001"
        )
        self.assertEqual(status, 404)

    def test_precheck_missing_params_is_404(self):
        status, _ = self._get_json("/api/precheck?judge_id=patterns-eval%2Fbaseline%2Fnewtype")
        self.assertEqual(status, 404)

    def test_precheck_path_traversal_rejected_without_touching_filesystem(self):
        judge_qs = urllib.parse.quote("patterns-eval/baseline/newtype", safe="")
        url = self._url(
            f"/api/precheck?judge_id={judge_qs}&candidate_id="
            + urllib.parse.quote("../../../../etc/passwd", safe="")
        )
        try:
            with urllib.request.urlopen(url) as resp:
                status = resp.status
        except urllib.error.HTTPError as e:
            status = e.code
        self.assertEqual(status, 404)

    def test_grading_still_writes_labels_yaml_when_precheck_data_present(self):
        # The pre-check panel must never interfere with the direct
        # read-modify-write grading flow.
        status, body = self._post_json(
            "/api/grade",
            {
                "judge_id": "patterns-eval/baseline/newtype",
                "candidate_id": "cand-001",
                "verdict": "pass",
            },
        )
        self.assertEqual(status, 200)
        self.assertEqual(body["verdict"], "Pass")
        on_disk = labels_store.read_labels(self.evals_dir / "labels.yaml")
        self.assertEqual(on_disk["patterns-eval__baseline__newtype__cand-001"], "Pass")
        # And the pre-check hint is still independently available afterwards.
        status, precheck_body = self._get_json(
            "/api/precheck?judge_id="
            + urllib.parse.quote("patterns-eval/baseline/newtype", safe="")
            + "&candidate_id=cand-001"
        )
        self.assertEqual(status, 200)
        self.assertTrue(precheck_body["available"])

    # -- security: judge-id AND candidate-id allowlist ----------------------
    def test_unknown_judge_id_is_404(self):
        status, _ = self._get_json("/api/judge?judge_id=does-not-exist")
        self.assertEqual(status, 404)

    def test_known_judge_id_with_unrelated_candidate_id_is_404(self):
        # delegate-52/corruption/case4 is a real judge, cand-001 is a real
        # candidate id -- but they are not a real matrix cell pair.
        status, _ = self._get_json(
            "/api/cell?judge_id="
            + urllib.parse.quote("delegate-52/corruption/case4", safe="")
            + "&candidate_id=cand-001"
        )
        self.assertEqual(status, 404)

    def test_path_traversal_judge_id_rejected_without_touching_filesystem(self):
        traversal_ids = [
            "../../../../etc/passwd",
            "..%2F..%2Fetc%2Fpasswd",
            "....//....//etc/passwd",
        ]
        for tid in traversal_ids:
            with self.subTest(tid=tid):
                url = self._url("/api/judge?judge_id=" + urllib.parse.quote(tid, safe=""))
                try:
                    with urllib.request.urlopen(url) as resp:
                        status = resp.status
                except urllib.error.HTTPError as e:
                    status = e.code
                self.assertEqual(status, 404)

    def test_path_traversal_candidate_id_rejected_without_touching_filesystem(self):
        traversal_ids = ["../../../../etc/passwd", "a/../../secrets"]
        judge_qs = urllib.parse.quote("patterns-eval/baseline/newtype", safe="")
        for tid in traversal_ids:
            with self.subTest(tid=tid):
                url = self._url(
                    f"/api/cell?judge_id={judge_qs}&candidate_id="
                    + urllib.parse.quote(tid, safe="")
                )
                try:
                    with urllib.request.urlopen(url) as resp:
                        status = resp.status
                except urllib.error.HTTPError as e:
                    status = e.code
                self.assertEqual(status, 404)

    def test_grade_unknown_pair_rejected(self):
        status, body = self._post_json(
            "/api/grade", {"judge_id": "nope", "candidate_id": "nope", "verdict": "pass"}
        )
        self.assertEqual(status, 404)

    def test_grade_invalid_verdict_rejected(self):
        status, body = self._post_json(
            "/api/grade",
            {
                "judge_id": "patterns-eval/baseline/newtype",
                "candidate_id": "cand-001",
                "verdict": "maybe",
            },
        )
        self.assertEqual(status, 400)

    # -- grading writes the REAL evals/labels.yaml immediately ---------------
    def test_grade_pass_writes_labels_yaml_with_composite_key_and_capitalized_value(self):
        status, body = self._post_json(
            "/api/grade",
            {
                "judge_id": "patterns-eval/baseline/newtype",
                "candidate_id": "cand-001",
                "verdict": "pass",
            },
        )
        self.assertEqual(status, 200)
        self.assertEqual(body["verdict"], "Pass")

        on_disk = labels_store.read_labels(self.evals_dir / "labels.yaml")
        self.assertEqual(
            on_disk["patterns-eval__baseline__newtype__cand-001"], "Pass"
        )

        # The judge still appears in /api/judges (the UI groups pickable vs.
        # done judges from this one list), but is no longer pickable: its
        # only cell is now graded.
        status, judges = self._get_json("/api/judges")
        judge = judges["judges"][0]
        self.assertFalse(judge["pickable"])
        self.assertEqual(judge["remaining_cells"], 0)
        self.assertEqual(judge["graded_cells"], 1)

    def test_grade_fail_also_writes_immediately(self):
        status, body = self._post_json(
            "/api/grade",
            {
                "judge_id": "patterns-eval/baseline/newtype",
                "candidate_id": "cand-001",
                "verdict": "fail",
            },
        )
        self.assertEqual(status, 200)
        on_disk = labels_store.read_labels(self.evals_dir / "labels.yaml")
        self.assertEqual(
            on_disk["patterns-eval__baseline__newtype__cand-001"], "Fail"
        )

    def test_inconclusive_is_client_side_only_no_endpoint_needed(self):
        # There is no server-side "inconclusive" verdict and no API call for
        # it -- it is purely a client-side skip (see static/app.js). Confirm
        # the server rejects it as an invalid verdict if a client ever sent
        # one, so it can never accidentally get written as ground truth.
        status, _ = self._post_json(
            "/api/grade",
            {
                "judge_id": "patterns-eval/baseline/newtype",
                "candidate_id": "cand-001",
                "verdict": "inconclusive",
            },
        )
        self.assertEqual(status, 400)
        on_disk = labels_store.read_labels(self.evals_dir / "labels.yaml")
        self.assertEqual(on_disk, {})


class AppPrecheckUnavailableTest(unittest.TestCase):
    """A known (judge, candidate) pair with NO cached pre-check data at all
    (no --precheck-path given, matching how AppContext defaults it) must
    read as "no hint available" -- never as a misleading "0 checks pass"."""

    def setUp(self):
        self.tmpdir = Path(tempfile.mkdtemp())
        self.mined_dir = self.tmpdir / "mined"
        self.evals_dir = self.tmpdir / "evals"
        self.static_dir = Path(__file__).resolve().parent.parent / "static"
        matrix_dir = self.mined_dir / "matrix" / "patterns-eval__baseline__newtype"
        matrix_dir.mkdir(parents=True)
        self.evals_dir.mkdir(parents=True)
        (matrix_dir / "cand-001.yaml").write_text(CONVERSATION_DRAFT, encoding="utf-8")
        matrix_path = self.mined_dir / "matrix.jsonl"
        with open(matrix_path, "w", encoding="utf-8") as f:
            f.write(
                json.dumps(
                    {
                        "judge_id": "patterns-eval/baseline/newtype",
                        "candidate_id": "cand-001",
                        "conversation_draft": "e2e/judge-calibration/mined/matrix/patterns-eval__baseline__newtype/cand-001.yaml",
                    }
                )
                + "\n"
            )
        judges_path = self.evals_dir / "judges.yaml"
        judges_path.write_text(JUDGES_YAML, encoding="utf-8")

        judges = JudgeRegistry(judges_path)
        matrix = MatrixStore(matrix_path, mined_dir=self.mined_dir)
        # No precheck kwarg at all -- AppContext must fall back gracefully.
        self.ctx = AppContext(judges, matrix, evals_dir=self.evals_dir, static_dir=self.static_dir)
        self.httpd = build_server(self.ctx, "127.0.0.1", 0)
        self.port = self.httpd.server_address[1]
        self.thread = threading.Thread(target=self.httpd.serve_forever, daemon=True)
        self.thread.start()

    def tearDown(self):
        self.httpd.shutdown()
        self.httpd.server_close()
        self.thread.join(timeout=5)
        shutil.rmtree(self.tmpdir, ignore_errors=True)

    def test_known_pair_with_no_cache_reports_unavailable_not_404_not_pass(self):
        url = f"http://127.0.0.1:{self.port}/api/precheck?judge_id=" + urllib.parse.quote(
            "patterns-eval/baseline/newtype", safe=""
        ) + "&candidate_id=cand-001"
        with urllib.request.urlopen(url) as resp:
            status = resp.status
            body = json.loads(resp.read())
        self.assertEqual(status, 200)
        self.assertFalse(body["available"])
        self.assertNotIn("precheck", body)


if __name__ == "__main__":
    unittest.main()
