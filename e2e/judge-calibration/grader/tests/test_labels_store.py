import tempfile
import unittest
from pathlib import Path

from backend import labels_store

SAMPLE = """\
# DRAFT — mined candidate labels for e2e/judge-calibration.
#
# some more header lines
# here too

alpha-001: TODO
beta-002: pass
gamma-003: fail
"""


class TestParsing(unittest.TestCase):
    def test_parse_labels_text(self):
        labels = labels_store.parse_labels_text(SAMPLE)
        self.assertEqual(labels, {"alpha-001": "TODO", "beta-002": "pass", "gamma-003": "fail"})

    def test_parse_ignores_comments_and_blanks(self):
        text = "# comment\n\nfoo-1: pass\n# another\nbar-2: TODO\n"
        labels = labels_store.parse_labels_text(text)
        self.assertEqual(labels, {"foo-1": "pass", "bar-2": "TODO"})

    def test_parse_empty_text(self):
        self.assertEqual(labels_store.parse_labels_text(""), {})


class TestRoundTrip(unittest.TestCase):
    def setUp(self):
        self.tmpdir = tempfile.TemporaryDirectory()
        self.path = Path(self.tmpdir.name) / "labels.yaml"
        self.path.write_text(SAMPLE, encoding="utf-8")

    def tearDown(self):
        self.tmpdir.cleanup()

    def test_read_matches_written_sample(self):
        labels = labels_store.read_labels(self.path)
        self.assertEqual(labels, {"alpha-001": "TODO", "beta-002": "pass", "gamma-003": "fail"})

    def test_noop_roundtrip_is_byte_identical(self):
        labels = labels_store.read_labels(self.path)
        order = ["alpha-001", "beta-002", "gamma-003"]
        header = labels_store.read_header(self.path)
        rendered = labels_store.render_labels_text(labels, order, header=header)
        self.assertEqual(rendered, SAMPLE)

    def test_set_label_updates_only_target_entry(self):
        order = ["alpha-001", "beta-002", "gamma-003"]
        new_labels = labels_store.set_label(self.path, order, "alpha-001", "pass")
        self.assertEqual(new_labels["alpha-001"], "pass")
        self.assertEqual(new_labels["beta-002"], "pass")
        self.assertEqual(new_labels["gamma-003"], "fail")

        # And it's actually persisted to disk.
        reread = labels_store.read_labels(self.path)
        self.assertEqual(reread["alpha-001"], "pass")

    def test_set_label_preserves_header(self):
        order = ["alpha-001", "beta-002", "gamma-003"]
        labels_store.set_label(self.path, order, "alpha-001", "fail")
        text = self.path.read_text(encoding="utf-8")
        self.assertTrue(text.startswith("# DRAFT"))

    def test_set_label_preserves_candidate_order(self):
        order = ["alpha-001", "beta-002", "gamma-003"]
        labels_store.set_label(self.path, order, "gamma-003", "pass")
        text = self.path.read_text(encoding="utf-8")
        idx_alpha = text.index("alpha-001")
        idx_beta = text.index("beta-002")
        idx_gamma = text.index("gamma-003")
        self.assertTrue(idx_alpha < idx_beta < idx_gamma)

    def test_set_label_rejects_unknown_id(self):
        order = ["alpha-001", "beta-002", "gamma-003"]
        with self.assertRaises(ValueError):
            labels_store.set_label(self.path, order, "does-not-exist", "pass")

    def test_set_label_rejects_bad_value(self):
        order = ["alpha-001", "beta-002", "gamma-003"]
        with self.assertRaises(ValueError):
            labels_store.set_label(self.path, order, "alpha-001", "maybe")

    def test_read_missing_file_returns_empty(self):
        missing = Path(self.tmpdir.name) / "does-not-exist.yaml"
        self.assertEqual(labels_store.read_labels(missing), {})

    def test_write_creates_parent_dirs(self):
        dest = Path(self.tmpdir.name) / "nested" / "dir" / "labels.yaml"
        labels_store.write_labels_atomic(dest, {"x-1": "pass"}, ["x-1"])
        self.assertTrue(dest.exists())
        self.assertEqual(labels_store.read_labels(dest), {"x-1": "pass"})

    def test_extra_ids_not_in_order_are_preserved_sorted(self):
        labels = {"alpha-001": "pass", "zzz-orphan": "fail"}
        order = ["alpha-001"]
        rendered = labels_store.render_labels_text(labels, order)
        self.assertIn("zzz-orphan: fail", rendered)


if __name__ == "__main__":
    unittest.main()
