import tempfile
import unittest
from pathlib import Path

from backend.conversation_draft import (
    load_conversation_draft,
    parse_conversation_draft,
    user_and_assistant_text,
)

SAMPLE = (
    '---\nmodel: "claude-sonnet-4-6"  # mined narrow draft -- verify against a real ailly ModelId before use\n'
    "---\nrole: user\ncontent: |\n"
    "  Please review this file.\n"
    "\n"
    "  It has two paragraphs.\n"
    "---\nrole: assistant\ncontent: |\n"
    "  Sure, here's the review:\n"
    "\n"
    "  - point one\n"
    "  - point two\n"
)


class TestParseConversationDraft(unittest.TestCase):
    def test_model_field_strips_quotes_and_comment(self):
        parsed = parse_conversation_draft(SAMPLE)
        self.assertEqual(parsed["model"], "claude-sonnet-4-6")

    def test_two_turns_in_order(self):
        parsed = parse_conversation_draft(SAMPLE)
        self.assertEqual(len(parsed["turns"]), 2)
        self.assertEqual(parsed["turns"][0]["role"], "user")
        self.assertEqual(parsed["turns"][1]["role"], "assistant")

    def test_block_literal_dedented_and_blank_lines_preserved(self):
        parsed = parse_conversation_draft(SAMPLE)
        self.assertEqual(
            parsed["turns"][0]["content"],
            "Please review this file.\n\nIt has two paragraphs.",
        )

    def test_assistant_content_preserves_list_markup_for_markdown_rendering(self):
        parsed = parse_conversation_draft(SAMPLE)
        self.assertIn("- point one", parsed["turns"][1]["content"])

    def test_missing_leading_document_marker_raises(self):
        with self.assertRaises(ValueError):
            parse_conversation_draft("role: user\ncontent: |\n  hi\n")

    def test_missing_content_marker_raises(self):
        bad = '---\nmodel: "x"\n---\nrole: user\nnotcontent: |\n  hi\n'
        with self.assertRaises(ValueError):
            parse_conversation_draft(bad)


class TestUserAndAssistantText(unittest.TestCase):
    def test_pulls_out_the_pair(self):
        parsed = parse_conversation_draft(SAMPLE)
        user, assistant = user_and_assistant_text(parsed)
        self.assertIn("two paragraphs", user)
        self.assertIn("point two", assistant)

    def test_missing_turn_returns_empty_string(self):
        user, assistant = user_and_assistant_text({"turns": [{"role": "user", "content": "hi"}]})
        self.assertEqual(user, "hi")
        self.assertEqual(assistant, "")


class TestLoadConversationDraft(unittest.TestCase):
    def test_reads_from_disk(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "draft.yaml"
            path.write_text(SAMPLE, encoding="utf-8")
            parsed = load_conversation_draft(path)
            self.assertEqual(parsed["model"], "claude-sonnet-4-6")


if __name__ == "__main__":
    unittest.main()
