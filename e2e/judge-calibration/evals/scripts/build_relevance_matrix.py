#!/usr/bin/env python3
"""Steps 3-5 of the judge-calibration relevance-matrix pipeline.

Given the judge registry (`discover_judges.py`'s `evals/judges.yaml`) and the
mined candidate pool (`mine_calibration_candidates.py`'s
`mined/candidates.jsonl`, each already tagged with `skill_tags`), this script:

3. **Relevance matrix** — for every judge with >=1 topic keyword, finds every
   candidate whose `skill_tags` overlap that judge's keywords
   (`skill_signals.tags_match`). Judges with an empty keyword list
   (`needs_human_review: true`) are skipped entirely — no automated matching
   is attempted for them, by design (see `discover_judges.py`).
4. **Narrow excerpt extraction** — for each relevant (judge, candidate) pair,
   re-opens that candidate's RAW source transcript and locates the message
   pair nearest to where the matched tag actually appears: the immediately
   preceding `AskUserQuestion` tool call (question + options) or plain user
   prompt — whichever actually precedes it — paired with the assistant's
   resulting message. This is narrower than the original mining pass, which
   captured the whole top-level turn. Writes one draft conversation-schema
   YAML file per matrix cell under
   `mined/matrix/<judge_id-safe>/<candidate_id>.yaml`.
5. **Output** — writes `mined/matrix.jsonl`, one line per relevant
   (judge_id, candidate_id) cell, and prints summary counts.

## Narrowing algorithm (step 4 detail)

Each candidate's turn boundary is re-derived from its RAW source file (not
re-parsed from the flattened `question`/`response` fields) by:

1. Reading every raw JSONL object in `source_file` in order.
2. Locating the object that started this turn — a real user turn (per
   `mine_calibration_candidates._is_real_claude_user_turn`) whose timestamp
   matches `turn_started_at` — as `start_idx`, and the next real user turn
   (or EOF) as the exclusive end boundary `end_idx`.
3. Walking `[start_idx, end_idx)` in order, tracking the most recent
   `AskUserQuestion` tool_use encountered (its `question`/`options`) as a
   candidate "preceding" element, falling back to the turn's own initiating
   user prompt if no `AskUserQuestion` occurred before the hit.
4. The "hit" is the first object in the window whose raw JSON contains the
   literal matched tag string; the "resulting assistant message" is that
   object's own assistant text if it has any, else the nearest following
   assistant object's text within the same window.

Codex transcripts don't have Claude Code's `Skill`/`AskUserQuestion`
tool-call shapes, so narrowing there only ever falls back to (initiating user
prompt, first assistant text after the hit) — still narrower than nothing,
but there is no clarifying-question exchange to discover. If no hit can be
located at all (the matched tag lived only in metadata this script doesn't
re-derive), the whole original `question`/`response` pair is used verbatim
and the cell is marked `narrowing: "fallback-full-turn"` so a human reviewer
knows not to expect a tight excerpt.

## Usage

    uv run --with pyyaml python3 build_relevance_matrix.py \\
        [--judges PATH] [--candidates PATH] [--mined-dir PATH]

Requires PyYAML (see `discover_judges.py`). Defaults: `--judges` is
`evals/judges.yaml` next to this script's `evals/` dir; `--candidates` is
`mined/candidates.jsonl`; `--mined-dir` is `mined/` (all gitignored except
`judges.yaml` itself, which lives under `evals/`).
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path
from typing import Any, Optional

import yaml

sys.path.insert(0, str(Path(__file__).resolve().parent))
from skill_signals import tags_match  # noqa: E402
import mine_calibration_candidates as mc  # noqa: E402


# --------------------------------------------------------------------------
# Loading
# --------------------------------------------------------------------------


def load_judges(path: Path) -> list[dict[str, Any]]:
    with path.open("r", encoding="utf-8") as fh:
        doc = yaml.safe_load(fh)
    return doc.get("judges", []) if doc else []


def load_candidates(path: Path) -> list[dict[str, Any]]:
    candidates = []
    with path.open("r", encoding="utf-8") as fh:
        for line in fh:
            line = line.strip()
            if line:
                candidates.append(json.loads(line))
    return candidates


def judge_id_safe(judge_id: str) -> str:
    return judge_id.replace("/", "__")


# --------------------------------------------------------------------------
# Step 3: relevance matrix
# --------------------------------------------------------------------------


def matching_candidates(
    judge: dict[str, Any], candidates: list[dict[str, Any]]
) -> list[tuple[dict[str, Any], str, str]]:
    """`(candidate, matched_keyword, matched_tag)` for every candidate whose
    `skill_tags` overlap this judge's keywords. A candidate can match on more
    than one keyword/tag pair; only the first is kept (one matrix cell per
    (judge, candidate), per the design brief)."""
    out = []
    keywords = [kw["value"] for kw in judge.get("keywords", [])]
    if not keywords:
        return out
    for cand in candidates:
        for tag in cand.get("skill_tags", []):
            hit = next((kw for kw in keywords if tags_match(kw, tag)), None)
            if hit is not None:
                out.append((cand, hit, tag))
                break
    return out


# --------------------------------------------------------------------------
# Step 4: narrow excerpt extraction
# --------------------------------------------------------------------------


def _load_raw_objects(source_file: Path, log: mc.MiningLog) -> list[dict[str, Any]]:
    return list(mc.read_jsonl(source_file, log))


def _ask_user_question_text(tool_use: dict[str, Any]) -> str:
    input_ = tool_use.get("input", {}) or {}
    questions = input_.get("questions") or []
    parts = []
    for q in questions:
        header = q.get("header", "")
        question = q.get("question", "")
        options = q.get("options") or []
        opt_lines = "\n".join(
            f"  - {o.get('label', '')}: {o.get('description', '')}" for o in options if isinstance(o, dict)
        )
        block = f"[{header}] {question}"
        if opt_lines:
            block += "\n" + opt_lines
        parts.append(block)
    return "\n\n".join(parts)


def _assistant_text(obj: dict[str, Any]) -> str:
    return "\n\n".join(t.strip() for t in mc._claude_assistant_text_blocks(obj) if t.strip())


def _find_turn_window(
    raw: list[dict[str, Any]], is_subagent_file: bool, turn_started_at: Optional[str]
) -> tuple[int, int]:
    """`(start_idx, end_idx)` bounding the candidate's turn, re-derived from
    the raw transcript by timestamp match against a real user turn (see
    module docstring). Falls back to `(0, len(raw))` if no match is found —
    callers treat that as "narrowing unavailable" and fall back to the
    original broad question/response pair."""
    start_idx = None
    for i, obj in enumerate(raw):
        if mc._is_real_claude_user_turn(obj, allow_sidechain=is_subagent_file) and obj.get("timestamp") == turn_started_at:
            start_idx = i
            break
    if start_idx is None:
        return 0, len(raw)
    end_idx = len(raw)
    for i in range(start_idx + 1, len(raw)):
        if mc._is_real_claude_user_turn(raw[i], allow_sidechain=is_subagent_file):
            end_idx = i
            break
    return start_idx, end_idx


def narrow_claude_excerpt(cand: dict[str, Any], matched_tag: str, log: mc.MiningLog) -> dict[str, Any]:
    """Returns `{preceding_role, preceding_content, assistant_content, narrowing}`."""
    source_file = Path(cand["source_file"])
    is_subagent_file = "subagents" in source_file.parts
    raw = _load_raw_objects(source_file, log)
    start_idx, end_idx = _find_turn_window(raw, is_subagent_file, cand.get("turn_started_at"))

    fallback = {
        "preceding_role": "user",
        "preceding_content": cand["question"],
        "assistant_content": cand["response"],
        "narrowing": "fallback-full-turn",
    }
    if end_idx <= start_idx:
        return fallback

    preceding_text = mc._claude_user_text(raw[start_idx]) or cand["question"]
    preceding_role = "user"
    idx_hit = None

    for i in range(start_idx, end_idx):
        obj = raw[i]
        if obj.get("type") != "assistant":
            continue
        content = obj.get("message", {}).get("content")
        if isinstance(content, list):
            for block in content:
                if isinstance(block, dict) and block.get("type") == "tool_use" and block.get("name") == "AskUserQuestion":
                    text = _ask_user_question_text(block)
                    if text:
                        preceding_text = text
                        preceding_role = "user"  # rendered as the prompting stimulus, per DESIGN.md's role set
        try:
            blob = json.dumps(obj, ensure_ascii=False)
        except (TypeError, ValueError):
            blob = str(obj)
        if idx_hit is None and matched_tag in blob:
            idx_hit = i

    if idx_hit is None:
        return fallback

    assistant_text = _assistant_text(raw[idx_hit])
    if not assistant_text:
        for i in range(idx_hit, end_idx):
            if raw[i].get("type") == "assistant":
                assistant_text = _assistant_text(raw[i])
                if assistant_text:
                    break
    if not assistant_text:
        return fallback

    return {
        "preceding_role": preceding_role,
        "preceding_content": preceding_text,
        "assistant_content": assistant_text,
        "narrowing": "ask-user-question" if preceding_text != cand["question"] else "user-prompt",
    }


def narrow_codex_excerpt(cand: dict[str, Any], matched_tag: str, log: mc.MiningLog) -> dict[str, Any]:
    """Codex has no `AskUserQuestion`/`Skill` tool shape (see module
    docstring) — narrowing only ever locates the nearest `agent_message`
    after the hit within the turn, still paired with the turn's own
    initiating user prompt."""
    source_file = Path(cand["source_file"])
    raw = _load_raw_objects(source_file, log)

    start_idx = None
    for i, obj in enumerate(raw):
        if obj.get("type") == "event_msg" and obj.get("payload", {}).get("type") == "user_message" and obj.get("timestamp") == cand.get("turn_started_at"):
            start_idx = i
            break
    fallback = {
        "preceding_role": "user",
        "preceding_content": cand["question"],
        "assistant_content": cand["response"],
        "narrowing": "fallback-full-turn",
    }
    if start_idx is None:
        return fallback
    end_idx = len(raw)
    for i in range(start_idx + 1, len(raw)):
        obj = raw[i]
        if obj.get("type") == "event_msg" and obj.get("payload", {}).get("type") == "user_message":
            end_idx = i
            break

    idx_hit = None
    for i in range(start_idx, end_idx):
        try:
            blob = json.dumps(raw[i], ensure_ascii=False)
        except (TypeError, ValueError):
            blob = str(raw[i])
        if matched_tag in blob:
            idx_hit = i
            break
    if idx_hit is None:
        return fallback

    assistant_text = ""
    for i in range(idx_hit, end_idx):
        payload = raw[i].get("payload", {})
        if raw[i].get("type") == "event_msg" and payload.get("type") == "agent_message":
            text = (payload.get("message") or "").strip()
            if text:
                assistant_text = text
                break
    if not assistant_text:
        return fallback

    return {
        "preceding_role": "user",
        "preceding_content": cand["question"],
        "assistant_content": assistant_text,
        "narrowing": "user-prompt",
    }


def write_narrow_conversation(excerpt: dict[str, Any], cand: dict[str, Any], path: Path) -> None:
    model = cand.get("model") or "unknown"
    path.parent.mkdir(parents=True, exist_ok=True)
    header = (
        f"---\n"
        f"model: {mc._yaml_double_quoted(model)}  # mined narrow draft — verify against a real ailly ModelId before use\n"
        f"---\n"
        f"role: {excerpt['preceding_role']}\n"
        f"content: |\n"
        f"{mc._yaml_block_literal(excerpt['preceding_content'])}\n"
        f"---\n"
        f"role: assistant\n"
        f"content: |\n"
        f"{mc._yaml_block_literal(excerpt['assistant_content'])}\n"
    )
    path.write_text(header)


# --------------------------------------------------------------------------
# Steps 3-5 orchestration
# --------------------------------------------------------------------------


def build(
    judges: list[dict[str, Any]],
    candidates: list[dict[str, Any]],
    mined_dir: Path,
    log: mc.MiningLog,
    repo_root: Path,
) -> dict[str, Any]:
    matrix_dir = mined_dir / "matrix"
    matrix_lines: list[str] = []
    judges_with_matches = 0
    matched_candidate_ids: set[str] = set()
    unmatched_judges: list[str] = []

    for judge in judges:
        if judge.get("needs_human_review"):
            unmatched_judges.append(judge["judge_id"])
            continue
        pairs = matching_candidates(judge, candidates)
        if not pairs:
            continue
        judges_with_matches += 1
        safe = judge_id_safe(judge["judge_id"])
        for cand, matched_keyword, matched_tag in pairs:
            matched_candidate_ids.add(cand["id"])
            if cand["source"] == "claude-code":
                excerpt = narrow_claude_excerpt(cand, matched_tag, log)
            else:
                excerpt = narrow_codex_excerpt(cand, matched_tag, log)

            draft_path = matrix_dir / safe / f"{cand['id']}.yaml"
            write_narrow_conversation(excerpt, cand, draft_path)

            matrix_lines.append(
                json.dumps(
                    {
                        "judge_id": judge["judge_id"],
                        "candidate_id": cand["id"],
                        "matched_keyword": matched_keyword,
                        "matched_tag": matched_tag,
                        "narrowing": excerpt["narrowing"],
                        "source": cand["source"],
                        "source_file": cand["source_file"],
                        "project_cwd": cand.get("project_cwd"),
                        "conversation_draft": str(draft_path.relative_to(repo_root)),
                    },
                    ensure_ascii=False,
                )
            )

    matrix_path = mined_dir / "matrix.jsonl"
    matrix_path.write_text("\n".join(matrix_lines) + ("\n" if matrix_lines else ""))

    return {
        "total_judges": len(judges),
        "judges_with_matches": judges_with_matches,
        "candidates_matched": len(matched_candidate_ids),
        "total_cells": len(matrix_lines),
        "unmatched_judges": unmatched_judges,
        "matrix_path": matrix_path,
    }


def main(argv: Optional[list[str]] = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--judges", type=Path, default=None)
    parser.add_argument("--candidates", type=Path, default=None)
    parser.add_argument("--mined-dir", type=Path, default=None)
    args = parser.parse_args(argv)

    scripts_dir = Path(__file__).resolve().parent
    evals_dir = scripts_dir.parent
    judge_calibration_dir = evals_dir.parent
    e2e_dir = judge_calibration_dir.parent
    repo_root = e2e_dir.parent

    judges_path = args.judges or (evals_dir / "judges.yaml")
    candidates_path = args.candidates or (judge_calibration_dir / "mined" / "candidates.jsonl")
    mined_dir = args.mined_dir or (judge_calibration_dir / "mined")

    judges = load_judges(judges_path)
    candidates = load_candidates(candidates_path)
    log = mc.MiningLog()

    summary = build(judges, candidates, mined_dir, log, repo_root)

    print(f"[build_relevance_matrix] {summary['total_judges']} judges loaded from {judges_path}")
    print(f"[build_relevance_matrix] {len(candidates)} candidates loaded from {candidates_path}")
    print(
        f"[build_relevance_matrix] {summary['judges_with_matches']}/{summary['total_judges']} "
        "judges found >=1 relevant candidate"
    )
    print(
        f"[build_relevance_matrix] {summary['candidates_matched']}/{len(candidates)} "
        "candidates matched >=1 judge"
    )
    print(f"[build_relevance_matrix] {summary['total_cells']} total matrix cells")
    if summary["unmatched_judges"]:
        print(f"[build_relevance_matrix] {len(summary['unmatched_judges'])} judges need human review (no keywords):")
        for jid in summary["unmatched_judges"]:
            print(f"  - {jid}")
    print(f"[build_relevance_matrix] wrote {summary['matrix_path']}")
    log.write(mined_dir / "matrix_build_log.txt", "build_relevance_matrix run")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
