import tempfile
import unittest
from pathlib import Path

from backend.judges import JudgeRegistry, parse_judges_yaml

# A trimmed-but-faithful excerpt of the real evals/judges.yaml shape,
# including the two tricky cases: an empty keywords list + needs_human_review
# true, and a prompt containing an escaped '' (a literal apostrophe).
SAMPLE = """\
# header comment
# more header

judges:
- judge_id: clean-comments-review/invocation/clean-comments-review
  suite: clean-comments-review
  suite_file: e2e/clean-comments-review/evals/invocation.yaml
  case_name: clean-comments-review
  case_when: null
  prompt: 'The review is a critique document, not edits to the code. It

    classifies the comment on `className` as a public DocBlock addressed

    to an external reader.

    '
  keywords:
  - value: clean-comments-review
    polarity: include
    source: assembly-external-skill-path
  needs_human_review: false
- judge_id: delegate-52/corruption/case4
  suite: delegate-52
  suite_file: e2e/delegate-52/evals/corruption.yaml
  case_name: null
  case_when: null
  prompt: 'Report the per-provider corruption count.

    provider''s output must be checked.

    '
  keywords: []
  needs_human_review: true
- judge_id: patterns-eval/discovery/paired-add-propagator
  suite: patterns-eval
  suite_file: e2e/patterns-eval/evals/discovery.yaml
  case_name: paired-add-propagator
  case_when: null
  prompt: 'Selects patterns:configuring-logging.

    '
  keywords:
  - value: patterns:configuring-logging
    polarity: include
    source: sibling
  - value: patterns:emitting-logs
    polarity: exclude
    source: sibling
  needs_human_review: false
"""


class TestParseJudgesYaml(unittest.TestCase):
    def test_parses_all_entries(self):
        judges = parse_judges_yaml(SAMPLE)
        self.assertEqual(len(judges), 3)
        self.assertEqual(
            [j["judge_id"] for j in judges],
            [
                "clean-comments-review/invocation/clean-comments-review",
                "delegate-52/corruption/case4",
                "patterns-eval/discovery/paired-add-propagator",
            ],
        )

    def test_folded_prompt_joins_lines_with_newline(self):
        judges = parse_judges_yaml(SAMPLE)
        j0 = judges[0]
        self.assertEqual(
            j0["prompt"],
            "The review is a critique document, not edits to the code. It\n"
            "classifies the comment on `className` as a public DocBlock addressed\n"
            "to an external reader.\n",
        )

    def test_escaped_single_quote_decodes_to_literal_quote(self):
        judges = parse_judges_yaml(SAMPLE)
        j1 = judges[1]
        self.assertIn("provider's output", j1["prompt"])

    def test_null_case_name_decodes_to_none(self):
        judges = parse_judges_yaml(SAMPLE)
        j1 = judges[1]
        self.assertIsNone(j1["case_name"])

    def test_empty_keywords_list(self):
        judges = parse_judges_yaml(SAMPLE)
        j1 = judges[1]
        self.assertEqual(j1["keywords"], [])
        self.assertTrue(j1["needs_human_review"])

    def test_multi_item_keywords_block(self):
        judges = parse_judges_yaml(SAMPLE)
        j2 = judges[2]
        self.assertEqual(
            j2["keywords"],
            [
                {
                    "value": "patterns:configuring-logging",
                    "polarity": "include",
                    "source": "sibling",
                },
                {
                    "value": "patterns:emitting-logs",
                    "polarity": "exclude",
                    "source": "sibling",
                },
            ],
        )

    def test_needs_human_review_false_decodes_to_bool(self):
        judges = parse_judges_yaml(SAMPLE)
        self.assertIs(judges[0]["needs_human_review"], False)

    def test_no_judge_entries_raises(self):
        with self.assertRaises(ValueError):
            parse_judges_yaml("judges: []\n")


class TestJudgeRegistry(unittest.TestCase):
    def setUp(self):
        self.tmpdir = tempfile.TemporaryDirectory()
        self.path = Path(self.tmpdir.name) / "judges.yaml"
        self.path.write_text(SAMPLE, encoding="utf-8")
        self.registry = JudgeRegistry(self.path)

    def tearDown(self):
        self.tmpdir.cleanup()

    def test_len_and_order(self):
        self.assertEqual(len(self.registry), 3)
        self.assertEqual(
            self.registry.order[0],
            "clean-comments-review/invocation/clean-comments-review",
        )

    def test_is_known_id(self):
        self.assertTrue(
            self.registry.is_known_id("delegate-52/corruption/case4")
        )
        self.assertFalse(self.registry.is_known_id("../../etc/passwd"))
        self.assertFalse(self.registry.is_known_id("does-not-exist"))

    def test_get_returns_full_record(self):
        rec = self.registry.get("delegate-52/corruption/case4")
        self.assertEqual(rec["suite"], "delegate-52")
        self.assertTrue(rec["needs_human_review"])

    def test_get_unknown_returns_none(self):
        self.assertIsNone(self.registry.get("nope"))

    def test_duplicate_judge_id_rejected(self):
        text = SAMPLE.replace(
            "- judge_id: patterns-eval/discovery/paired-add-propagator",
            "- judge_id: clean-comments-review/invocation/clean-comments-review",
        )
        path = Path(self.tmpdir.name) / "dup.yaml"
        path.write_text(text, encoding="utf-8")
        with self.assertRaises(ValueError):
            JudgeRegistry(path)


if __name__ == "__main__":
    unittest.main()
