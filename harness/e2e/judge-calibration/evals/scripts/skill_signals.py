#!/usr/bin/env python3
"""Shared skill/reference-identifier signal detection for judge calibration.

Three scripts in this directory need the *same* deterministic, regex-based
notion of "what skill or reference does this text/JSON blob mention":

- `discover_judges.py` (step 1) mines it from eval-suite prompt/rubric text
  and sibling `text_contains`/`text_not_contains` assertion values, to learn
  what a `judge` assertion is actually checking for.
- `mine_calibration_candidates.py` (step 2) mines it from raw mined-session
  transcript JSON, to tag each candidate with the skills/references its real
  conversation touched.
- `build_relevance_matrix.py` (steps 3-5) uses the *same* normalization to
  decide whether a judge's topic keywords and a candidate's tags refer to the
  same thing, so a change here changes matching everywhere consistently.

Everything here is pure pattern matching over text — no LLM calls, no
subjective judgment — so re-running any of the three scripts against the same
input always yields the same tags.
"""

from __future__ import annotations

import json
import re
from typing import Any, Iterable

# Recognised skill-plugin namespaces. Kept as a small closed set (rather than
# a bare `\w+:` prefix) so we don't accidentally treat every `key: value`
# pair in a transcript (e.g. YAML-ish text a model quoted) as a skill token.
# Extend this set if a new plugin namespace is introduced.
PLUGIN_PREFIXES = ("patterns", "developer", "domain", "research", "general")

# `patterns:configuring-logging`, `developer:ailly`, ... — a plugin prefix,
# a literal colon with NO intervening space (so YAML `domain: prose-bio`
# mapping pairs, which always have a space after the colon, never match),
# followed by a skill-name-shaped token (word chars/hyphens).
SKILL_TOKEN_RE = re.compile(
    r"\b(?:" + "|".join(PLUGIN_PREFIXES) + r"):[A-Za-z][\w-]*\b"
)

# A `Skill` tool invocation's `input.skill` field, e.g.
# `"input": {"skill": "developer:git-workflow", ...}` — captured directly out
# of the raw JSON text rather than requiring a full JSON walk, since every
# candidate object is available to us as (or trivially re-serializable to)
# a JSON string already.
SKILL_TOOL_FIELD_RE = re.compile(r'"skill"\s*:\s*"([^"]+)"')

# `<command-name>/developer:ailly</command-name>` style slash-command tags,
# as Claude Code embeds them in user-turn text.
COMMAND_NAME_RE = re.compile(r"<command-name>\s*([^<]+?)\s*</command-name>", re.IGNORECASE)

# Explicit reference-file path mentions: a `references/*.md` path (pattern
# skills' bundled reference docs) or any `SKILL.md` path.
REFERENCE_PATH_RE = re.compile(r"[\w./-]*\breferences/[\w.-]+\.md\b")
SKILL_MD_PATH_RE = re.compile(r"[\w./-]*\bSKILL\.md\b")


def extract_skill_tags(text: str) -> set[str]:
    """Pull every skill/reference identifier out of one blob of text.

    `text` is typically `json.dumps(raw_transcript_object)` — dumping to JSON
    first (rather than trying to walk the object's structure) lets one set of
    regexes cover tool_use inputs, message text, and tag-shaped strings alike,
    and is exactly as deterministic since `json.dumps` output is stable for a
    given object.
    """
    tags: set[str] = set()

    for m in SKILL_TOOL_FIELD_RE.finditer(text):
        val = m.group(1)
        if SKILL_TOKEN_RE.fullmatch(val):
            tags.add(val)

    for m in SKILL_TOKEN_RE.finditer(text):
        tags.add(m.group(0))

    for m in COMMAND_NAME_RE.finditer(text):
        val = m.group(1).strip().lstrip("/")
        if SKILL_TOKEN_RE.fullmatch(val):
            tags.add(val)

    for m in REFERENCE_PATH_RE.finditer(text):
        tags.add(m.group(0))

    for m in SKILL_MD_PATH_RE.finditer(text):
        tags.add(m.group(0))

    return tags


def extract_skill_tags_from_objs(objs: Iterable[Any]) -> set[str]:
    """Convenience: union of `extract_skill_tags` over several raw JSON objects."""
    tags: set[str] = set()
    for obj in objs:
        try:
            blob = json.dumps(obj, ensure_ascii=False)
        except (TypeError, ValueError):
            blob = str(obj)
        tags |= extract_skill_tags(blob)
    return tags


def normalize_tag(tag: str) -> tuple[str, str]:
    """`(full_lower, short_lower)` — `short` is the part after the last ':'.

    `patterns:configuring-logging` -> `("patterns:configuring-logging",
    "configuring-logging")`. A bare tag with no colon (e.g. a case name we
    could not confidently prefix with a plugin, or a `SKILL.md` path)
    normalizes to the same value for both.
    """
    full = tag.strip().lower()
    short = full.rsplit(":", 1)[-1]
    # A reference-path tag's "short" form (last path segment) is rarely a
    # useful match key on its own (e.g. every skill ships a `SKILL.md`), so
    # only shorten bare-word/colon-token tags, not paths.
    if "/" in full:
        short = full
    return full, short


def tags_match(judge_keyword: str, candidate_tag: str) -> bool:
    """Decide whether a judge's topic keyword and a candidate's tag refer to
    the same skill/reference.

    Two rules, both exact string comparisons (no fuzzy/edit-distance
    matching):
      1. Full-value equality, case-insensitive — the common case, e.g. both
         sides are exactly `patterns:configuring-logging`.
      2. Short-name equality when at least one side is a recognised
         `plugin:name` token — lets a judge keyword that could only be
         resolved to a bare short name (e.g. `clean-comments-review`, for a
         suite whose own files never spell out its plugin prefix) still
         match a candidate tag that *did* capture the full `developer:` form
         (a `Skill` tool call, which always records the full id), and vice
         versa. Bare-tag-to-bare-tag comparisons only ever use rule 1 to
         avoid accidental short-word collisions (e.g. two unrelated
         `SKILL.md` reference-path mentions).
    """
    jf, js = normalize_tag(judge_keyword)
    cf, cs = normalize_tag(candidate_tag)
    if jf == cf:
        return True
    j_is_token = SKILL_TOKEN_RE.fullmatch(judge_keyword.strip()) is not None
    c_is_token = SKILL_TOKEN_RE.fullmatch(candidate_tag.strip()) is not None
    if (j_is_token or c_is_token) and js == cs and len(js) >= 4:
        return True
    return False
