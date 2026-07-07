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

A turn's ``content`` can also be a block sequence of ``ContentBlock``
mappings instead of a plain block-literal scalar -- this is how a mined
session that included tool calls (Edit/Write/MultiEdit/...) alongside prose
is represented, mirroring the real conversation schema's ``Content::Blocks``
shape::

    ---
    role: assistant
    content:
      - type: text
        text: |
          <block-literal text>
      - type: tool_use
        id: "toolu_..."
        name: "Write"
        input:
          file_path: "..."
          content: |
            <block-literal text>

In that shape, a turn's ``content`` field is a ``list[dict]`` (each dict at
least has a ``type`` key) rather than a ``str``. ``text_only`` and
``flatten_for_checker`` below both accept either shape.

As with ``backend/judges.py``, this is a hand-rolled parser for exactly this
fixed, machine-generated shape rather than a general YAML parser (stdlib
only, no PyYAML). The block-sequence content parser is a small recursive
mapping/sequence parser (still narrow -- no flow style, no anchors/aliases,
2-space nesting steps only) rather than a general YAML parser, for the same
reason.
"""
from __future__ import annotations

import re
from pathlib import Path
from typing import Any

_CONTENT_INDENT = 2

_KV_RE = re.compile(r"^( *)([A-Za-z_][A-Za-z0-9_]*): ?(.*)$")
_ITEM_FIRST_KV_RE = re.compile(r"^([A-Za-z_][A-Za-z0-9_]*): ?(.*)$")


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


def _dedent_lines(lines: list[str], content_indent: int) -> str:
    out = []
    for line in lines:
        if line == "":
            out.append("")
            continue
        if line[:content_indent] != " " * content_indent:
            raise ValueError(f"expected {content_indent}-space indented content line: {line!r}")
        out.append(line[content_indent:])
    return "\n".join(out)


def _parse_block_literal_value(lines: list[str], idx: int, content_indent: int) -> tuple[str, int]:
    """Parse a ``key: |`` block literal whose content starts at ``lines[idx]``,
    indented ``content_indent`` spaces. Returns ``(dedented_value, next_idx)``.

    Trailing blank lines immediately before a dedent (the next sibling field,
    the next list item, or end of input) are dropped -- matching YAML's
    default "clip" chomping for a ``|`` block scalar -- rather than kept as
    trailing blank paragraphs that were never really part of the text.
    """
    n = len(lines)
    content_lines: list[str] = []
    j = idx
    while j < n and (lines[j] == "" or lines[j][:content_indent] == " " * content_indent):
        content_lines.append(lines[j])
        j += 1
    while content_lines and content_lines[-1] == "":
        content_lines.pop()
        j -= 1
    return _dedent_lines(content_lines, content_indent), j


def _parse_mapping(lines: list[str], idx: int, indent: int) -> tuple[dict[str, Any], int]:
    """Parse consecutive ``key: value`` lines at exactly ``indent`` spaces,
    starting at ``lines[idx]``. Stops at the first non-blank line that is not
    indented exactly ``indent`` spaces (a dedent to a sibling/enclosing
    field, or a list-item marker at a shallower indent). Returns
    ``(mapping, next_idx)``.
    """
    result: dict[str, Any] = {}
    n = len(lines)
    while idx < n:
        line = lines[idx]
        if line.strip() == "":
            idx += 1
            continue
        m = _KV_RE.match(line)
        if not m or len(m.group(1)) != indent:
            break
        key, rest = m.group(2), m.group(3)
        value, idx = _parse_field_value(lines, idx + 1, indent, rest)
        result[key] = value
    return result, idx


def _parse_field_value(lines: list[str], idx: int, indent: int, rest: str) -> tuple[Any, int]:
    """Parse the value half of one ``key: rest`` line already consumed at
    ``indent``. ``idx`` is the index of the line *after* that key line."""
    if rest == "|":
        return _parse_block_literal_value(lines, idx, indent + 2)
    if rest == "":
        # A nested mapping or list sequence, one step deeper.
        k = idx
        n = len(lines)
        while k < n and lines[k].strip() == "":
            k += 1
        if k >= n:
            return {}, k
        nested_indent = len(lines[k]) - len(lines[k].lstrip(" "))
        if nested_indent <= indent:
            # Nothing nested here after all (an empty/null field) -- leave
            # the line for the caller.
            return None, idx
        if lines[k].lstrip(" ").startswith("- "):
            return _parse_sequence(lines, k, nested_indent)
        return _parse_mapping(lines, k, nested_indent)
    return _unquote_plain(rest), idx


def _parse_sequence(lines: list[str], idx: int, indent: int) -> tuple[list[Any], int]:
    """Parse a block sequence of mappings (``- key: value`` items, each
    possibly carrying further-indented continuation fields), starting at
    ``lines[idx]`` whose ``-`` marker sits at ``indent`` spaces."""
    items: list[dict[str, Any]] = []
    n = len(lines)
    marker = " " * indent + "- "
    while idx < n:
        line = lines[idx]
        if line.strip() == "":
            idx += 1
            continue
        if line[: len(marker)] != marker:
            break
        first_rest = line[len(marker) :]
        m = _ITEM_FIRST_KV_RE.match(first_rest)
        if not m:
            raise ValueError(f"unrecognized list item shape: {line!r}")
        first_key, first_val_rest = m.group(1), m.group(2)
        item: dict[str, Any] = {}
        value, idx = _parse_field_value(lines, idx + 1, indent + 2, first_val_rest)
        item[first_key] = value
        rest_fields, idx = _parse_mapping(lines, idx, indent + 2)
        item.update(rest_fields)
        items.append(item)
    return items, idx


_DOUBLE_QUOTE_ESCAPES = {
    "n": "\n",
    "t": "\t",
    "r": "\r",
    '"': '"',
    "\\": "\\",
}

_ESCAPE_RE = re.compile(r"\\(u[0-9A-Fa-f]{4}|.)")


def _decode_double_quoted(inner: str) -> str:
    """Decode YAML double-quoted-scalar backslash escapes (``\\n``, ``\\t``,
    ``\\"``, ``\\\\``, ``\\uXXXX``). A mined tool_use field (e.g. an Edit's
    ``old_string``/``new_string`` containing a literal apostrophe) is
    sometimes emitted as a double-quoted scalar with a Unicode escape rather
    than a block literal; leaving these undecoded would hand a checker
    script literal backslash-u sequences instead of the real character.
    Unrecognized escapes are left as-is (backslash and following char kept
    verbatim) rather than guessed at.
    """

    def repl(match: re.Match[str]) -> str:
        token = match.group(1)
        if token.startswith("u"):
            return chr(int(token[1:], 16))
        return _DOUBLE_QUOTE_ESCAPES.get(token, match.group(0))

    return _ESCAPE_RE.sub(repl, inner)


def _unquote_plain(value: str) -> str:
    value = value.strip()
    if len(value) >= 2 and value[0] == '"' and value[-1] == '"':
        return _decode_double_quoted(value[1:-1])
    if len(value) >= 2 and value[0] == "'" and value[-1] == "'":
        return value[1:-1].replace("''", "'")
    # Strip a trailing "  # comment" the model field may carry.
    if "  #" in value:
        value = value.split("  #", 1)[0].rstrip()
        if len(value) >= 2 and value[0] == '"' and value[-1] == '"':
            return _decode_double_quoted(value[1:-1])
    return value


def parse_conversation_draft(text: str) -> dict[str, Any]:
    """Parse the narrow draft into ``{"model": str|None, "turns": [...]}``
    where each turn is ``{"role": str, "content": str | list[dict]}`` -- a
    ``str`` for the plain block-literal shape, or a ``list[dict]`` of
    ``ContentBlock`` mappings for a mined turn that carried tool calls (see
    module docstring)."""
    if not text.startswith("---\n"):
        raise ValueError("conversation draft must start with a '---' document marker")
    docs = text[len("---\n") :].split("\n---\n")

    model: str | None = None
    turns: list[dict[str, Any]] = []

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
            if len(lines) < 2:
                raise ValueError(f"expected a 'content:' line after role line, got: {lines[1:2]!r}")
            content_line = lines[1].strip()
            if content_line == "content: |":
                content: Any = _dedent_block_literal(lines[2:])
            elif content_line == "content:":
                content, _next = _parse_sequence(lines, 2, indent=2)
            else:
                raise ValueError(f"expected 'content: |' or 'content:' after role line, got: {lines[1:2]!r}")
            turns.append({"role": role, "content": content})
            continue
        raise ValueError(f"unrecognized conversation-draft document start: {first!r}")

    return {"model": model, "turns": turns}


def load_conversation_draft(path: Path) -> dict[str, Any]:
    return parse_conversation_draft(Path(path).read_text(encoding="utf-8"))


# File-extension -> fenced-code-block language tag, for flattening a
# Write/Edit/MultiEdit tool_use block's real file content into a fence a
# checker script's fenced-code extraction can find. Best-effort only: an
# unrecognized extension gets an untagged fence rather than a guess.
_LANGUAGE_BY_EXTENSION = {
    ".ts": "typescript",
    ".tsx": "typescript",
    ".js": "javascript",
    ".jsx": "javascript",
    ".py": "python",
    ".rs": "rust",
    ".md": "markdown",
    ".yaml": "yaml",
    ".yml": "yaml",
    ".json": "json",
    ".sh": "bash",
    ".toml": "toml",
}

# tool_use block names whose ``input`` carries real file content/diff text
# worth flattening into a fenced code block for a checker to see. Other tool
# calls (Task, Bash, Read, Grep, ...) don't carry candidate-authored file
# content in a form that would be honest to present as "the code", so they
# are left out entirely -- same as production's blindness to *any* tool_use
# block, just narrower than "everything is invisible".
_CODE_CARRYING_TOOLS = {"Write", "Edit", "MultiEdit"}


def _language_for_path(file_path: str) -> str:
    for ext, lang in _LANGUAGE_BY_EXTENSION.items():
        if file_path.endswith(ext):
            return lang
    return ""


def _tool_use_code_fragments(block: dict[str, Any]) -> list[str]:
    """Real file content/diff text carried by one Write/Edit/MultiEdit
    tool_use block's ``input``, as a list of fenced-code-block strings (one
    fragment per edit for MultiEdit). Returns ``[]`` for a tool name this
    module doesn't know how to flatten, or a block missing the fields it
    expects -- never fabricated placeholder text."""
    name = block.get("name")
    tool_input = block.get("input")
    if name not in _CODE_CARRYING_TOOLS or not isinstance(tool_input, dict):
        return []
    file_path = tool_input.get("file_path", "") or ""
    lang = _language_for_path(file_path)

    def fence(code: str) -> str:
        return f"```{lang}\n{code}\n```"

    if name == "Write":
        content = tool_input.get("content")
        return [fence(content)] if isinstance(content, str) else []
    if name == "Edit":
        new_string = tool_input.get("new_string")
        return [fence(new_string)] if isinstance(new_string, str) else []
    if name == "MultiEdit":
        edits = tool_input.get("edits")
        if not isinstance(edits, list):
            return []
        fragments = []
        for edit in edits:
            if isinstance(edit, dict) and isinstance(edit.get("new_string"), str):
                fragments.append(fence(edit["new_string"]))
        return fragments
    return []


def text_only(content: Any) -> str:
    """The plain-text projection of a turn's content, matching production's
    ``final_assistant_text``/``final_user_text``: for a scalar (str) turn,
    the string itself; for a structured (ContentBlock list) turn, every
    ``type: text`` block's text, in declaration order, joined by ``"\\n"``.
    ``tool_use``/``tool_result`` blocks contribute nothing here -- this is
    the "what a real text_contains/text_matches/text_equals assertion, or a
    judge assertion, actually sees today" projection, deliberately including
    no tool-call content. See ``flatten_for_checker`` for the more generous
    projection used for script-assertion pre-checks."""
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        return "\n".join(
            block.get("text", "")
            for block in content
            if isinstance(block, dict) and block.get("type") == "text"
        )
    return ""


def flatten_for_checker(content: Any) -> str:
    """The text handed to a ``script`` assertion's checker stdin for a
    pre-check run: for a scalar (str) turn, the string itself unchanged
    (already plain narration text, occasionally already containing fenced
    code -- the 15 synthetic-live cells all look like this). For a
    structured (ContentBlock list) turn, every ``text`` block's text
    (declaration order) PLUS, after it, a fenced code block for every
    Write/Edit/MultiEdit ``tool_use`` block's real file content (language
    inferred from the file extension).

    This is deliberately MORE than what the real, live `script`/`judge`
    assertion reads today (final-assistant-text-only, see
    ``backend/precheck.py``'s module docstring and DESIGN.md) -- it exists
    so a checker that greps for fenced code has a genuine chance to find
    code a mined session actually wrote via a tool call instead of typing it
    directly into its reply. It never fabricates code that was not really
    in the tool call's input.
    """
    if isinstance(content, str):
        return content
    if not isinstance(content, list):
        return ""
    parts: list[str] = []
    for block in content:
        if not isinstance(block, dict):
            continue
        block_type = block.get("type")
        if block_type == "text":
            text = block.get("text", "")
            if text:
                parts.append(text)
        elif block_type == "tool_use":
            parts.extend(_tool_use_code_fragments(block))
    return "\n\n".join(parts)


def raw_user_and_assistant(parsed: dict[str, Any]) -> tuple[Any, Any]:
    """Pull out the (user, assistant) pair's RAW content -- ``str`` for a
    scalar turn, ``list[dict]`` of ContentBlocks for a structured turn --
    without projecting it down to text. Used by ``backend/precheck.py``,
    which needs the raw tool_use blocks (via ``flatten_for_checker``) for a
    script-assertion pre-check, not just the narration (``text_only``)."""
    turns = parsed.get("turns", [])
    assistant = next((t["content"] for t in turns if t["role"] == "assistant"), "")
    user = next((t["content"] for t in turns if t["role"] != "assistant"), "")
    return user, assistant


def user_and_assistant_text(parsed: dict[str, Any]) -> tuple[str, str]:
    """Pull out the (user, assistant) pair as plain text for human display,
    via ``text_only`` -- a structured (tool-call-carrying) turn is projected
    down to its narration text only, same as what a real text/judge
    assertion sees (see ``text_only``'s docstring). The narrow drafts today
    are always exactly a preceding user turn and an assistant turn, but this
    tolerates additional/different-ordered turns by role rather than
    assuming positional order."""
    user, assistant = raw_user_and_assistant(parsed)
    return text_only(user), text_only(assistant)
