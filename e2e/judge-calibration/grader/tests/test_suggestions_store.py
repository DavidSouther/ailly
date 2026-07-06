import tempfile
import unittest
from pathlib import Path

from backend import suggestions_store


class TestCadence(unittest.TestCase):
    def test_no_recompute_below_cadence(self):
        labels = {f"id{i}": "pass" for i in range(10)}
        cache = {"computed_at_grade_count": 0}
        self.assertFalse(suggestions_store.should_recompute(labels, cache, cadence=20))

    def test_recompute_once_cadence_reached(self):
        labels = {f"id{i}": "pass" for i in range(20)}
        cache = {"computed_at_grade_count": 0}
        self.assertTrue(suggestions_store.should_recompute(labels, cache, cadence=20))

    def test_no_recompute_again_until_next_cadence_step(self):
        labels = {f"id{i}": "pass" for i in range(25)}
        cache = {"computed_at_grade_count": 20}
        self.assertFalse(suggestions_store.should_recompute(labels, cache, cadence=20))

    def test_recompute_at_next_cadence_step(self):
        labels = {f"id{i}": "pass" for i in range(40)}
        cache = {"computed_at_grade_count": 20}
        self.assertTrue(suggestions_store.should_recompute(labels, cache, cadence=20))

    def test_todo_labels_do_not_count(self):
        labels = {f"id{i}": "TODO" for i in range(30)}
        cache = {"computed_at_grade_count": 0}
        self.assertFalse(suggestions_store.should_recompute(labels, cache, cadence=20))


class TestPersistence(unittest.TestCase):
    def setUp(self):
        self.tmpdir = tempfile.TemporaryDirectory()
        self.path = Path(self.tmpdir.name) / "suggestions.json"

    def tearDown(self):
        self.tmpdir.cleanup()

    def test_missing_file_returns_defaults(self):
        cache = suggestions_store.read_suggestions_cache(self.path)
        self.assertEqual(cache["computed_at_grade_count"], 0)
        self.assertEqual(cache["suggestions"], {})

    def test_recompute_writes_and_is_readable(self):
        texts = {
            "graded-1": "refactor the authentication middleware for clarity and safety",
            "graded-2": "refactor authentication middleware cleanly and safely please",
            "ungraded-1": "refactor the authentication middleware please for clarity",
        }
        labels = {"graded-1": "pass", "graded-2": "pass"}
        cache = suggestions_store.recompute(texts, labels, self.path, k=2, sim_threshold=0.1)
        self.assertIn("ungraded-1", cache["suggestions"])
        reread = suggestions_store.read_suggestions_cache(self.path)
        self.assertEqual(reread["suggestions"], cache["suggestions"])

    def test_maybe_recompute_respects_cadence(self):
        labels = {f"id{i}": "pass" for i in range(5)}
        texts = {k: "some text words here" for k in labels}
        result = suggestions_store.maybe_recompute(texts, labels, self.path, cadence=20)
        self.assertIsNone(result)
        self.assertFalse(self.path.exists())


if __name__ == "__main__":
    unittest.main()
