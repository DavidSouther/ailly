import tempfile
import unittest
from pathlib import Path

from backend import export, labels_store


class TestCurate(unittest.TestCase):
    def test_only_graded_and_included_survive(self):
        order = ["a", "b", "c", "d"]
        labels = {"a": "pass", "b": "TODO", "c": "fail", "d": "pass"}
        included = {"a": True, "c": True, "d": False}
        curated = export.curate(order, labels, included)
        self.assertEqual(curated, {"a": "pass", "c": "fail"})

    def test_ungraded_never_included_even_if_marked(self):
        order = ["a"]
        labels = {"a": "TODO"}
        included = {"a": True}
        curated = export.curate(order, labels, included)
        self.assertEqual(curated, {})

    def test_empty_included_map_exports_nothing(self):
        order = ["a", "b"]
        labels = {"a": "pass", "b": "fail"}
        curated = export.curate(order, labels, {})
        self.assertEqual(curated, {})


class TestExportCuratedLabels(unittest.TestCase):
    def setUp(self):
        self.tmpdir = tempfile.TemporaryDirectory()
        self.dest = Path(self.tmpdir.name) / "evals" / "labels.yaml"

    def tearDown(self):
        self.tmpdir.cleanup()

    def test_writes_only_curated_subset(self):
        order = ["a", "b", "c"]
        labels = {"a": "pass", "b": "fail", "c": "TODO"}
        included = {"a": True, "b": False}
        curated = export.export_curated_labels(order, labels, included, self.dest)
        self.assertEqual(curated, {"a": "pass"})
        self.assertTrue(self.dest.exists())
        on_disk = labels_store.read_labels(self.dest)
        self.assertEqual(on_disk, {"a": "pass"})

    def test_output_header_marks_it_curated(self):
        order = ["a"]
        labels = {"a": "pass"}
        included = {"a": True}
        export.export_curated_labels(order, labels, included, self.dest)
        text = self.dest.read_text(encoding="utf-8")
        self.assertIn("CURATED", text)

    def test_second_export_overwrites_cleanly(self):
        order = ["a", "b"]
        export.export_curated_labels(order, {"a": "pass"}, {"a": True}, self.dest)
        export.export_curated_labels(order, {"a": "pass", "b": "fail"}, {"a": True, "b": True}, self.dest)
        on_disk = labels_store.read_labels(self.dest)
        self.assertEqual(on_disk, {"a": "pass", "b": "fail"})


if __name__ == "__main__":
    unittest.main()
