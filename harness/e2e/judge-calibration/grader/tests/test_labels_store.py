import tempfile
import unittest
from pathlib import Path

from backend import labels_store

SAMPLE = """\
# Judge-calibration ground truth
# some more header lines
# here too

clean-comments-review__invocation__clean-comments-review__cand-001: Pass
patterns-eval__baseline__newtype__cand-002: Fail
"""


class TestCompositeKey(unittest.TestCase):
    def test_composite_key_replaces_slashes_with_double_underscore(self):
        key = labels_store.composite_key("patterns-eval/baseline/newtype", "cand-001")
        self.assertEqual(key, "patterns-eval__baseline__newtype__cand-001")

    def test_composite_key_is_stable_for_repeated_calls(self):
        a = labels_store.composite_key("suite/x/y", "cand")
        b = labels_store.composite_key("suite/x/y", "cand")
        self.assertEqual(a, b)


class TestParsing(unittest.TestCase):
    def test_parse_labels_text(self):
        labels = labels_store.parse_labels_text(SAMPLE)
        self.assertEqual(
            labels,
            {
                "clean-comments-review__invocation__clean-comments-review__cand-001": "Pass",
                "patterns-eval__baseline__newtype__cand-002": "Fail",
            },
        )

    def test_parse_ignores_comments_and_blanks(self):
        text = "# comment\n\nfoo__1: Pass\n# another\nbar__2: Fail\n"
        labels = labels_store.parse_labels_text(text)
        self.assertEqual(labels, {"foo__1": "Pass", "bar__2": "Fail"})

    def test_parse_empty_text(self):
        self.assertEqual(labels_store.parse_labels_text(""), {})

    def test_parse_rejects_lowercase_value(self):
        # HumanVerdict's serde representation is capitalized (Pass/Fail); a
        # lowercase value here would silently fail to round-trip through a
        # Rust loader, so this parser treats it as malformed rather than
        # accepting it.
        with self.assertRaises(ValueError):
            labels_store.parse_labels_text("foo__1: pass\n")

    def test_parse_rejects_unrecognized_value(self):
        with self.assertRaises(ValueError):
            labels_store.parse_labels_text("foo__1: Maybe\n")


class TestRoundTrip(unittest.TestCase):
    def setUp(self):
        self.tmpdir = tempfile.TemporaryDirectory()
        self.path = Path(self.tmpdir.name) / "labels.yaml"
        self.path.write_text(SAMPLE, encoding="utf-8")

    def tearDown(self):
        self.tmpdir.cleanup()

    def test_read_matches_written_sample(self):
        labels = labels_store.read_labels(self.path)
        self.assertEqual(
            labels,
            {
                "clean-comments-review__invocation__clean-comments-review__cand-001": "Pass",
                "patterns-eval__baseline__newtype__cand-002": "Fail",
            },
        )

    def test_noop_roundtrip_is_byte_identical(self):
        labels = labels_store.read_labels(self.path)
        header = labels_store.read_header(self.path)
        rendered = labels_store.render_labels_text(labels, header=header)
        self.assertEqual(rendered, SAMPLE)

    def test_set_verdict_updates_only_target_entry(self):
        new_labels = labels_store.set_verdict(
            self.path, "patterns-eval/baseline/newtype", "cand-003", "Pass"
        )
        self.assertEqual(
            new_labels["patterns-eval__baseline__newtype__cand-003"], "Pass"
        )
        self.assertEqual(
            new_labels["clean-comments-review__invocation__clean-comments-review__cand-001"],
            "Pass",
        )
        self.assertEqual(new_labels["patterns-eval__baseline__newtype__cand-002"], "Fail")

        reread = labels_store.read_labels(self.path)
        self.assertEqual(reread["patterns-eval__baseline__newtype__cand-003"], "Pass")

    def test_set_verdict_overwrites_existing_entry(self):
        labels_store.set_verdict(
            self.path, "patterns-eval/baseline/newtype", "cand-002", "Pass"
        )
        reread = labels_store.read_labels(self.path)
        self.assertEqual(reread["patterns-eval__baseline__newtype__cand-002"], "Pass")

    def test_set_verdict_preserves_header(self):
        labels_store.set_verdict(
            self.path, "some/judge", "cand-x", "Fail"
        )
        text = self.path.read_text(encoding="utf-8")
        self.assertTrue(text.startswith("# Judge-calibration ground truth"))

    def test_set_verdict_rejects_bad_value(self):
        with self.assertRaises(ValueError):
            labels_store.set_verdict(self.path, "some/judge", "cand-x", "maybe")

    def test_read_missing_file_returns_empty(self):
        missing = Path(self.tmpdir.name) / "does-not-exist.yaml"
        self.assertEqual(labels_store.read_labels(missing), {})

    def test_write_creates_parent_dirs(self):
        dest = Path(self.tmpdir.name) / "nested" / "dir" / "labels.yaml"
        labels_store.write_labels_atomic(dest, {"x__1": "Pass"})
        self.assertTrue(dest.exists())
        self.assertEqual(labels_store.read_labels(dest), {"x__1": "Pass"})

    def test_render_sorts_keys_for_a_stable_diff(self):
        rendered = labels_store.render_labels_text({"zzz__1": "Pass", "aaa__1": "Fail"})
        self.assertLess(rendered.index("aaa__1"), rendered.index("zzz__1"))

    def test_ungraded_cell_is_absent_not_a_todo_placeholder(self):
        # Unlike v1's mined/-draft labels.yaml, there is no TODO sentinel:
        # a cell that has never been graded simply has no entry.
        labels = labels_store.read_labels(self.path)
        self.assertNotIn("no-such-key", labels)


if __name__ == "__main__":
    unittest.main()
