import tempfile
import unittest
from pathlib import Path

from backend.conversation_draft import (
    flatten_for_checker,
    load_conversation_draft,
    parse_conversation_draft,
    text_only,
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

BLOCKS_SAMPLE = (
    '---\nmodel: "claude-opus-4-8"  # mined narrow draft -- verify against a real ailly ModelId before use\n'
    "---\nrole: user\ncontent: |\n"
    "  Add a UserId newtype.\n"
    "---\nrole: assistant\ncontent:\n"
    "  - type: text\n"
    "    text: |\n"
    "      Here is the change.\n"
    "  - type: tool_use\n"
    '    id: "toolu_01"\n'
    '    name: "Write"\n'
    "    input:\n"
    '      file_path: "/repo/src/user_id.ts"\n'
    "      content: |\n"
    "        export type UserId = string & { readonly __brand: unique symbol };\n"
    "  - type: tool_use\n"
    '    id: "toolu_02"\n'
    '    name: "Edit"\n'
    "    input:\n"
    '      file_path: "/repo/README.md"\n'
    '      old_string: "old"\n'
    '      new_string: "You\\u2019re welcome"\n'
    "      replace_all: false\n"
    "  - type: tool_use\n"
    '    id: "toolu_03"\n'
    '    name: "Task"\n'
    "    input:\n"
    '      prompt: "irrelevant, not code-carrying"\n'
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


class TestBlockSequenceContent(unittest.TestCase):
    """A mined turn whose content is a ContentBlock list (text + tool_use),
    as produced for a session that included Edit/Write/MultiEdit/other tool
    calls, rather than the plain block-literal scalar shape."""

    def test_content_parses_as_a_list_of_typed_blocks(self):
        parsed = parse_conversation_draft(BLOCKS_SAMPLE)
        content = parsed["turns"][1]["content"]
        self.assertIsInstance(content, list)
        self.assertEqual([b["type"] for b in content], ["text", "tool_use", "tool_use", "tool_use"])

    def test_tool_use_fields_parsed_including_nested_input(self):
        parsed = parse_conversation_draft(BLOCKS_SAMPLE)
        write_block = parsed["turns"][1]["content"][1]
        self.assertEqual(write_block["id"], "toolu_01")
        self.assertEqual(write_block["name"], "Write")
        self.assertEqual(write_block["input"]["file_path"], "/repo/src/user_id.ts")
        self.assertIn("UserId", write_block["input"]["content"])

    def test_double_quoted_unicode_escape_is_decoded(self):
        parsed = parse_conversation_draft(BLOCKS_SAMPLE)
        edit_block = parsed["turns"][1]["content"][2]
        self.assertEqual(edit_block["input"]["new_string"], "You’re welcome")

    def test_does_not_crash_load_conversation_draft(self):
        # Regression: the parser used to raise ValueError on a bare
        # "content:" line (no "|"), which is exactly the shape a mined
        # tool-call-carrying turn uses.
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "draft.yaml"
            path.write_text(BLOCKS_SAMPLE, encoding="utf-8")
            parsed = load_conversation_draft(path)
            self.assertEqual(parsed["model"], "claude-opus-4-8")


class TestTextOnly(unittest.TestCase):
    def test_scalar_content_is_returned_unchanged(self):
        self.assertEqual(text_only("hello"), "hello")

    def test_block_list_joins_only_text_blocks(self):
        parsed = parse_conversation_draft(BLOCKS_SAMPLE)
        assistant_raw = parsed["turns"][1]["content"]
        self.assertEqual(text_only(assistant_raw), "Here is the change.")

    def test_user_and_assistant_text_does_not_crash_on_block_list(self):
        parsed = parse_conversation_draft(BLOCKS_SAMPLE)
        user, assistant = user_and_assistant_text(parsed)
        self.assertEqual(user, "Add a UserId newtype.")
        self.assertEqual(assistant, "Here is the change.")


class TestFlattenForChecker(unittest.TestCase):
    def test_scalar_content_is_returned_unchanged(self):
        self.assertEqual(flatten_for_checker("already ```code```"), "already ```code```")

    def test_write_and_edit_are_flattened_into_fenced_code_by_extension(self):
        parsed = parse_conversation_draft(BLOCKS_SAMPLE)
        assistant_raw = parsed["turns"][1]["content"]
        flat = flatten_for_checker(assistant_raw)
        self.assertIn("Here is the change.", flat)
        self.assertIn("```typescript\nexport type UserId", flat)
        self.assertIn("```markdown\nYou’re welcome\n```", flat)

    def test_non_code_carrying_tool_use_contributes_nothing(self):
        parsed = parse_conversation_draft(BLOCKS_SAMPLE)
        assistant_raw = parsed["turns"][1]["content"]
        flat = flatten_for_checker(assistant_raw)
        self.assertNotIn("irrelevant, not code-carrying", flat)


if __name__ == "__main__":
    unittest.main()
