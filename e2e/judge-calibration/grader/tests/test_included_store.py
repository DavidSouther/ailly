import tempfile
import unittest
from pathlib import Path

from backend import included_store


class TestIncludedStore(unittest.TestCase):
    def setUp(self):
        self.tmpdir = tempfile.TemporaryDirectory()
        self.path = Path(self.tmpdir.name) / "included.json"

    def tearDown(self):
        self.tmpdir.cleanup()

    def test_missing_file_returns_empty(self):
        self.assertEqual(included_store.read_included(self.path), {})

    def test_set_included_true_then_read_back(self):
        included_store.set_included(self.path, "a", {"a", "b"}, True)
        self.assertEqual(included_store.read_included(self.path), {"a": True})

    def test_set_included_false_removes_entry(self):
        included_store.set_included(self.path, "a", {"a", "b"}, True)
        included_store.set_included(self.path, "a", {"a", "b"}, False)
        self.assertEqual(included_store.read_included(self.path), {})

    def test_rejects_unknown_id(self):
        with self.assertRaises(ValueError):
            included_store.set_included(self.path, "not-valid", {"a", "b"}, True)

    def test_corrupt_json_treated_as_empty(self):
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.path.write_text("{not valid json", encoding="utf-8")
        self.assertEqual(included_store.read_included(self.path), {})


if __name__ == "__main__":
    unittest.main()
