import json
import tempfile
import unittest
from pathlib import Path

from backend.matrix import MatrixStore

CELL_A = {
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
CELL_B = {
    "judge_id": "patterns-eval/baseline/newtype",
    "candidate_id": "cand-002",
    "matched_keyword": "patterns:newtype",
    "matched_tag": "patterns:newtype",
    "narrowing": "user-prompt",
    "source": "codex",
    "source_file": "/tmp/y.jsonl",
    "project_cwd": "/tmp/proj2",
    "conversation_draft": "e2e/judge-calibration/mined/matrix/patterns-eval__baseline__newtype/cand-002.yaml",
    "repaired": False,
}
CELL_C = {
    "judge_id": "clean-comments-review/invocation/clean-comments-review",
    "candidate_id": "cand-003",
    "matched_keyword": "clean-comments-review",
    "matched_tag": "developer:clean-comments-review",
    "narrowing": "user-prompt",
    "source": "claude-code",
    "source_file": "/tmp/z.jsonl",
    "project_cwd": "/tmp/proj3",
    "conversation_draft": "e2e/judge-calibration/mined/matrix/clean-comments-review__invocation__clean-comments-review/cand-003.yaml",
    "repaired": True,
}


class TestMatrixStore(unittest.TestCase):
    def setUp(self):
        self.tmpdir = tempfile.TemporaryDirectory()
        self.mined_dir = Path(self.tmpdir.name) / "mined"
        self.mined_dir.mkdir()
        self.jsonl_path = self.mined_dir / "matrix.jsonl"
        with open(self.jsonl_path, "w", encoding="utf-8") as f:
            for cell in (CELL_A, CELL_B, CELL_C):
                f.write(json.dumps(cell) + "\n")
        self.store = MatrixStore(self.jsonl_path, mined_dir=self.mined_dir)

    def tearDown(self):
        self.tmpdir.cleanup()

    def test_len(self):
        self.assertEqual(len(self.store), 3)

    def test_judge_ids_are_the_distinct_judges_with_cells(self):
        self.assertEqual(
            self.store.judge_ids,
            {
                "patterns-eval/baseline/newtype",
                "clean-comments-review/invocation/clean-comments-review",
            },
        )

    def test_candidate_ids_allowlist(self):
        self.assertEqual(self.store.candidate_ids, {"cand-001", "cand-002", "cand-003"})

    def test_cells_for_judge_returns_only_that_judges_cells(self):
        cells = self.store.cells_for_judge("patterns-eval/baseline/newtype")
        self.assertEqual({c["candidate_id"] for c in cells}, {"cand-001", "cand-002"})

    def test_cells_for_unknown_judge_is_empty(self):
        self.assertEqual(self.store.cells_for_judge("no-such-judge"), [])

    def test_all_cells_returns_every_cell_in_file_order(self):
        self.assertEqual(
            [c["candidate_id"] for c in self.store.all_cells()],
            ["cand-001", "cand-002", "cand-003"],
        )

    def test_cell_lookup_by_pair(self):
        cell = self.store.cell("patterns-eval/baseline/newtype", "cand-001")
        self.assertEqual(cell["matched_keyword"], "patterns:newtype")

    def test_is_known_pair_rejects_valid_ids_in_wrong_combination(self):
        # cand-003 is real, but only relevant to the clean-comments-review
        # judge -- pairing it with an unrelated judge must not validate.
        self.assertFalse(self.store.is_known_pair("patterns-eval/baseline/newtype", "cand-003"))
        self.assertTrue(
            self.store.is_known_pair(
                "clean-comments-review/invocation/clean-comments-review", "cand-003"
            )
        )

    def test_is_known_pair_rejects_unknown_ids(self):
        self.assertFalse(self.store.is_known_pair("nope", "cand-001"))
        self.assertFalse(self.store.is_known_pair("patterns-eval/baseline/newtype", "nope"))

    def test_resolve_conversation_draft_path_stays_under_mined_dir(self):
        cell = self.store.cell("patterns-eval/baseline/newtype", "cand-001")
        resolved = self.store.resolve_conversation_draft_path(cell)
        self.assertEqual(
            resolved,
            self.mined_dir / "matrix" / "patterns-eval__baseline__newtype" / "cand-001.yaml",
        )

    def test_missing_matrix_file_is_treated_as_zero_cells(self):
        missing_path = self.mined_dir / "does-not-exist.jsonl"
        store = MatrixStore(missing_path, mined_dir=self.mined_dir)
        self.assertEqual(len(store), 0)
        self.assertEqual(store.judge_ids, set())

    def test_duplicate_cell_is_rejected(self):
        with open(self.jsonl_path, "a", encoding="utf-8") as f:
            f.write(json.dumps(CELL_A) + "\n")
        with self.assertRaises(ValueError):
            MatrixStore(self.jsonl_path, mined_dir=self.mined_dir)

    def test_reload_picks_up_a_newly_appended_cell(self):
        # Growing matrix.jsonl (e.g. the parallel synthetic-dataset work
        # landing more rows) must not require any code change to be picked
        # up on the next reload -- this pins that a larger file "just works".
        self.assertEqual(len(self.store), 3)
        with open(self.jsonl_path, "a", encoding="utf-8") as f:
            new_cell = dict(CELL_A, candidate_id="cand-004")
            f.write(json.dumps(new_cell) + "\n")
        self.store.reload()
        self.assertEqual(len(self.store), 4)
        self.assertIn("cand-004", self.store.candidate_ids)


if __name__ == "__main__":
    unittest.main()
