"""Parse a narrow conversation-schema draft yaml file (one (user, assistant)
turn pair), as produced by ``build_relevance_matrix.py``'s
``write_narrow_conversation``.

Fixed shape (a stream of small YAML documents separated by ``---`` lines)::

    ---
    model: "claude-sonnet-4-6"  # mined narrow draft -- ...
    ---
    role: user
    content: |
      <block-literal text, 2-space indented>
    ---
    role: assistant
    content: |
      <block-literal text, 2-space indented>

As with ``backend/judges.py``, this is a hand-rolled parser for exactly this
fixed, machine-generated shape rather than a general YAML parser (stdlib
only, no PyYAML).
"""
from __future__ import annotations

from pathlib import Path
from typing import Any

_CONTENT_INDENT = 2


def _dedent_block_literal(lines: list[str]) -> str:
    out = []
    for line in lines:
        if line == "":
            out.append("")
            continue
        if line[:_CONTENT_INDENT] != " " * _CONTENT_INDENT:
            raise ValueError(f"expected {_CONTENT_INDENT}-space indented content line: {line!r}")
        out.append(line[_CONTENT_INDENT:])
    return "\n".join(out)


def _unquote_plain(value: str) -> str:
    value = value.strip()
    if len(value) >= 2 and value[0] == '"' and value[-1] == '"':
        return value[1:-1]
    if len(value) >= 2 and value[0] == "'" and value[-1] == "'":
        return value[1:-1].replace("''", "'")
    # Strip a trailing "  # comment" the model field may carry.
    if "  #" in value:
        value = value.split("  #", 1)[0].rstrip()
        if len(value) >= 2 and value[0] == '"' and value[-1] == '"':
            return value[1:-1]
    return value


def parse_conversation_draft(text: str) -> dict[str, Any]:
    """Parse the narrow draft into ``{"model": str|None, "turns": [...]}``
    where each turn is ``{"role": str, "content": str}``."""
    if not text.startswith("---\n"):
        raise ValueError("conversation draft must start with a '---' document marker")
    docs = text[len("---\n") :].split("\n---\n")

    model: str | None = None
    turns: list[dict[str, str]] = []

    for doc in docs:
        lines = doc.split("\n")
        # Drop a single trailing blank line left by the split.
        if lines and lines[-1] == "":
            lines = lines[:-1]
        if not lines:
            continue
        first = lines[0]
        if first.startswith("model:"):
            model = _unquote_plain(first[len("model:") :])
            continue
        if first.startswith("role:"):
            role = _unquote_plain(first[len("role:") :])
            if len(lines) < 2 or lines[1].strip() != "content: |":
                raise ValueError(f"expected 'content: |' after role line, got: {lines[1:2]!r}")
            content = _dedent_block_literal(lines[2:])
            turns.append({"role": role, "content": content})
            continue
        raise ValueError(f"unrecognized conversation-draft document start: {first!r}")

    return {"model": model, "turns": turns}


def load_conversation_draft(path: Path) -> dict[str, Any]:
    return parse_conversation_draft(Path(path).read_text(encoding="utf-8"))


def user_and_assistant_text(parsed: dict[str, Any]) -> tuple[str, str]:
    """Pull out the (user, assistant) pair. The narrow drafts today are
    always exactly a preceding user turn and an assistant turn, but this
    tolerates additional/different-ordered turns by role rather than
    assuming positional order."""
    turns = parsed.get("turns", [])
    assistant = next((t["content"] for t in turns if t["role"] == "assistant"), "")
    user = next((t["content"] for t in turns if t["role"] != "assistant"), "")
    return user, assistant
