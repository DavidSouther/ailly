import json
import tempfile
import unittest
from pathlib import Path

from backend.candidates import CandidateStore


def write_jsonl(path, records):
    with open(path, "w", encoding="utf-8") as f:
        for r in records:
            f.write(json.dumps(r) + "\n")


class TestCandidateStore(unittest.TestCase):
    def setUp(self):
        self.tmpdir = tempfile.TemporaryDirectory()
        self.path = Path(self.tmpdir.name) / "candidates.jsonl"

    def tearDown(self):
        self.tmpdir.cleanup()

    def test_loads_ids_in_order(self):
        write_jsonl(self.path, [{"id": "a", "source": "claude-code"}, {"id": "b", "source": "codex"}])
        store = CandidateStore(self.path)
        self.assertEqual(store.order, ["a", "b"])
        self.assertEqual(len(store), 2)

    def test_allowlist_membership(self):
        write_jsonl(self.path, [{"id": "known-1", "source": "codex"}])
        store = CandidateStore(self.path)
        self.assertTrue(store.is_known_id("known-1"))
        self.assertFalse(store.is_known_id("../../etc/passwd"))
        self.assertFalse(store.is_known_id("unknown-2"))
        self.assertFalse(store.is_known_id(""))

    def test_get_returns_full_record(self):
        write_jsonl(self.path, [{"id": "a", "source": "codex", "response": "hi"}])
        store = CandidateStore(self.path)
        self.assertEqual(store.get("a")["response"], "hi")
        self.assertIsNone(store.get("nope"))

    def test_rejects_duplicate_ids(self):
        write_jsonl(self.path, [{"id": "dup"}, {"id": "dup"}])
        with self.assertRaises(ValueError):
            CandidateStore(self.path)

    def test_rejects_missing_id_field(self):
        write_jsonl(self.path, [{"source": "codex"}])
        with self.assertRaises(ValueError):
            CandidateStore(self.path)

    def test_rejects_malformed_json_line(self):
        self.path.write_text('{"id": "a"}\nnot json at all\n', encoding="utf-8")
        with self.assertRaises(ValueError):
            CandidateStore(self.path)

    def test_skips_blank_lines(self):
        self.path.write_text('{"id": "a"}\n\n\n{"id": "b"}\n', encoding="utf-8")
        store = CandidateStore(self.path)
        self.assertEqual(store.order, ["a", "b"])

    def test_sources_preserves_first_seen_order(self):
        write_jsonl(
            self.path,
            [{"id": "a", "source": "codex"}, {"id": "b", "source": "claude-code"}, {"id": "c", "source": "codex"}],
        )
        store = CandidateStore(self.path)
        self.assertEqual(store.sources(), ["codex", "claude-code"])


if __name__ == "__main__":
    unittest.main()
