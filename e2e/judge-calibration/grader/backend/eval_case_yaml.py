"""Read one named ``Case``'s ``assertions:`` list straight out of a real
``e2e/<suite>/evals/<file>.yaml`` eval suite file (DESIGN.md's `evaluation`
schema).

This is used by ``backend/precheck.py`` to look up a judge's *sibling*
assertions (the deterministic ``text_contains``/``text_not_contains``/
``script`` assertions that live in the very same ``Case`` as the ``judge``
assertion `backend/judges.py`'s registry was built from) by reading the
actual source file, not a duplicated/hardcoded copy -- so this stays correct
if the suite ever changes.

Like ``backend/judges.py`` and ``backend/conversation_draft.py``, this is a
hand-rolled parser for exactly the fixed, human-authored shape these eval
suite files use (stdlib only, no PyYAML): a top-level ``cases:`` block
sequence, each case a mapping with a ``name:`` (or ``when:``) key and an
``assertions:`` block sequence, each assertion either a single-line flow
mapping (``{ type: text_contains, value: "..." }``) or a block-style mapping
whose first field is ``- type: <kind>`` followed by further-indented fields,
some of which may themselves be flow mappings (``script: { path: ... }``) or
block literals (``prompt: |``). It is not a general YAML parser: an
unrecognized shape raises ``ValueError`` rather than silently
misinterpreting the file.
"""
from __future__ import annotations

import re
from pathlib import Path
from typing import Any

_CASE_START_RE = re.compile(r"^( *)- name: (.*)$")
_KV_LINE_RE = re.compile(r"^( *)([A-Za-z_][A-Za-z0-9_]*): ?(.*)$")
_ITEM_FIRST_KV_RE = re.compile(r"^([A-Za-z_][A-Za-z0-9_]*): ?(.*)$")

_DOUBLE_QUOTE_ESCAPES = {"n": "\n", "t": "\t", "r": "\r", '"': '"', "\\": "\\"}
_ESCAPE_RE = re.compile(r"\\(u[0-9A-Fa-f]{4}|.)")


def _decode_double_quoted(inner: str) -> str:
    def repl(match: re.Match[str]) -> str:
        token = match.group(1)
        if token.startswith("u"):
            return chr(int(token[1:], 16))
        return _DOUBLE_QUOTE_ESCAPES.get(token, match.group(0))

    return _ESCAPE_RE.sub(repl, inner)


def _unquote_scalar(value: str) -> Any:
    value = value.strip()
    if value == "":
        return None
    if len(value) >= 2 and value[0] == '"' and value[-1] == '"':
        return _decode_double_quoted(value[1:-1])
    if len(value) >= 2 and value[0] == "'" and value[-1] == "'":
        return value[1:-1].replace("''", "'")
    if value == "true":
        return True
    if value == "false":
        return False
    if value == "null":
        return None
    if re.fullmatch(r"-?\d+", value):
        return int(value)
    if re.fullmatch(r"-?\d+\.\d+", value):
        return float(value)
    return value


def _split_flow_items(inner: str) -> list[str]:
    """Split the interior of a ``{ ... }`` flow mapping on top-level commas,
    respecting nested ``{}``/``[]`` and quoted strings."""
    items: list[str] = []
    depth = 0
    quote: str | None = None
    current: list[str] = []
    i = 0
    while i < len(inner):
        ch = inner[i]
        if quote is not None:
            current.append(ch)
            if ch == quote:
                quote = None
            i += 1
            continue
        if ch in "\"'":
            quote = ch
            current.append(ch)
            i += 1
            continue
        if ch in "{[":
            depth += 1
            current.append(ch)
            i += 1
            continue
        if ch in "}]":
            depth -= 1
            current.append(ch)
            i += 1
            continue
        if ch == "," and depth == 0:
            items.append("".join(current))
            current = []
            i += 1
            continue
        current.append(ch)
        i += 1
    if current:
        items.append("".join(current))
    return items


def _parse_flow_mapping(text: str) -> dict[str, Any]:
    text = text.strip()
    if not (text.startswith("{") and text.endswith("}")):
        raise ValueError(f"expected a flow mapping '{{ ... }}', got: {text!r}")
    inner = text[1:-1].strip()
    result: dict[str, Any] = {}
    if not inner:
        return result
    for raw_item in _split_flow_items(inner):
        item = raw_item.strip()
        if not item:
            continue
        key, sep, value = item.partition(":")
        if not sep:
            raise ValueError(f"malformed flow-mapping entry (no ':'): {item!r}")
        key = key.strip()
        value = value.strip()
        result[key] = _parse_flow_mapping(value) if value.startswith("{") else _unquote_scalar(value)
    return result


def _parse_block_literal(lines: list[str], idx: int, content_indent: int) -> tuple[str, int]:
    n = len(lines)
    content_lines: list[str] = []
    j = idx
    while j < n and (lines[j] == "" or lines[j][:content_indent] == " " * content_indent):
        content_lines.append(lines[j])
        j += 1
    while content_lines and content_lines[-1] == "":
        content_lines.pop()
        j -= 1
    out = []
    for line in content_lines:
        if line == "":
            out.append("")
            continue
        if line[:content_indent] != " " * content_indent:
            raise ValueError(f"expected {content_indent}-space indented content line: {line!r}")
        out.append(line[content_indent:])
    return "\n".join(out), j


def _parse_field_value(lines: list[str], idx: int, indent: int, rest: str) -> tuple[Any, int]:
    rest = rest.strip()
    if rest == "|":
        return _parse_block_literal(lines, idx, indent + 2)
    if rest.startswith("{"):
        return _parse_flow_mapping(rest), idx
    return _unquote_scalar(rest), idx


def _is_ignorable(line: str) -> bool:
    """Blank, or a full-line ``#`` comment -- these are legal between
    assertion items (and, in principle, between mapping fields) in a
    human-authored eval suite yaml file and must not be mistaken for the end
    of the enclosing block sequence/mapping."""
    stripped = line.strip()
    return stripped == "" or stripped.startswith("#")


def _parse_mapping(lines: list[str], idx: int, indent: int, end: int) -> tuple[dict[str, Any], int]:
    """Parse consecutive ``key: value`` lines at exactly ``indent`` spaces,
    stopping at ``end`` or the first non-blank, non-comment line that isn't
    at ``indent``."""
    result: dict[str, Any] = {}
    while idx < end:
        line = lines[idx]
        if _is_ignorable(line):
            idx += 1
            continue
        m = _KV_LINE_RE.match(line)
        if not m or len(m.group(1)) != indent:
            break
        key, rest = m.group(2), m.group(3)
        value, idx = _parse_field_value(lines, idx + 1, indent, rest)
        result[key] = value
    return result, idx


def _parse_assertion_sequence(lines: list[str], idx: int, indent: int, end: int) -> list[dict[str, Any]]:
    items: list[dict[str, Any]] = []
    marker = " " * indent + "- "
    while idx < end:
        line = lines[idx]
        if _is_ignorable(line):
            idx += 1
            continue
        if line[: len(marker)] != marker:
            break
        rest = line[len(marker) :]
        if rest.lstrip().startswith("{"):
            items.append(_parse_flow_mapping(rest.strip()))
            idx += 1
            continue
        m = _ITEM_FIRST_KV_RE.match(rest)
        if not m:
            raise ValueError(f"unrecognized assertion item shape: {line!r}")
        first_key, first_rest = m.group(1), m.group(2)
        value, idx = _parse_field_value(lines, idx + 1, indent + 2, first_rest)
        item: dict[str, Any] = {first_key: value}
        more, idx = _parse_mapping(lines, idx, indent + 2, end)
        item.update(more)
        items.append(item)
    return items


def _find_case_block(lines: list[str], case_name: str) -> tuple[int, int, int]:
    """Returns ``(start_index, end_index, case_indent)`` for the ``- name:
    <case_name>`` block sequence entry. ``start_index`` is the line with the
    ``- name:`` marker; ``end_index`` is the index of the next case at the
    same indent, or ``len(lines)``."""
    starts = []
    for i, line in enumerate(lines):
        m = _CASE_START_RE.match(line)
        if m:
            starts.append((i, len(m.group(1)), _unquote_scalar(m.group(2))))
    matches = [(i, indent) for i, indent, name in starts if name == case_name]
    if not matches:
        raise ValueError(f"case {case_name!r} not found (no '- name: {case_name}' block)")
    if len(matches) > 1:
        raise ValueError(f"case {case_name!r} matched multiple blocks -- ambiguous")
    start, indent = matches[0]
    end = len(lines)
    for i, ind in ((i, ind) for i, ind, _ in starts):
        if i > start and ind <= indent:
            end = i
            break
    return start, end, indent


def parse_case_assertions(text: str, case_name: str) -> list[dict[str, Any]]:
    """Return the ``assertions:`` list (each item a plain ``dict``, e.g.
    ``{"type": "text_contains", "value": "patterns:newtype"}``) for the case
    named ``case_name`` in this eval-suite yaml ``text``."""
    lines = text.split("\n")
    start, end, case_indent = _find_case_block(lines, case_name)
    assertions_indent = case_indent + 2
    assertions_re = re.compile(r"^ {%d}assertions:\s*$" % assertions_indent)
    assertions_start = None
    for i in range(start, end):
        if assertions_re.match(lines[i]):
            assertions_start = i + 1
            break
    if assertions_start is None:
        raise ValueError(f"case {case_name!r} has no 'assertions:' block")
    return _parse_assertion_sequence(lines, assertions_start, assertions_indent + 2, end)


def load_case_assertions(suite_file_path: Path, case_name: str) -> list[dict[str, Any]]:
    """Read and parse ``suite_file_path`` (an actual ``e2e/*/evals/*.yaml``
    file), returning ``case_name``'s assertions."""
    text = Path(suite_file_path).read_text(encoding="utf-8")
    return parse_case_assertions(text, case_name)
