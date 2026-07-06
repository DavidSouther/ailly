#!/usr/bin/env python3
"""Mine judge-calibration candidate examples from real agent-session history.

Feature-step E ("Judge calibration") of the `ailly-evals` project needs 20-50
human-labeled `(question, candidate response)` examples to measure how well
`ailly_two`'s `judge` assertion agrees with a human labeler (see
`.ailly/developer/2026-07-06-A-ailly-evals/feature-e-judge-calibration/design.md`,
Open Artifact Decision #5, in the `domain-driven-design` repo). Rather than
hand-author those examples from scratch, this script mines *candidates* for
them out of the operator's own past Claude Code and Codex agent-session
transcripts, wherever those sessions actually invoked Ailly (the
`developer:ailly`-family skills, or the `ailly`/`ailly_two` tooling itself).

This script only produces a DRAFT. It never assigns a pass/fail verdict
itself — that is exactly the human judgment this calibration set exists to
check the judge against. Where a transcript's very next human message makes
an unambiguous approval/complaint about the prior response (e.g. "that's
wrong" or "looks good"), the script records that as a low-confidence
`candidate_label_human_implied` hint, kept structurally separate from the
`label` field a human still has to fill in.

## CONFIDENTIALITY — read before running

Session transcripts span many different client/employer codebases as well as
personal projects. The mined output necessarily contains verbatim excerpts of
real conversations from all of them. This script's output directory
(`e2e/judge-calibration/mined/` by default) is local-only: it is listed in
this repo's `.gitignore` and MUST NEVER be committed. Only this script (code)
is meant to be checked in. A human must review the mined candidates and
explicitly choose what, if anything, is safe to promote into the real,
checked-in `e2e/judge-calibration/evals/labels.yaml` and
`e2e/judge-calibration/runs/<run-id>/*.yaml` (see DESIGN.md's `conversation`
and `evaluation` schemas, and `feature-e-judge-calibration/design.md`'s
"Suite/labels shape" section) before any of that content leaves this
machine.

## Where transcripts live

- Claude Code: `~/.claude/projects/<url-encoded-project-path>/<session-uuid>.jsonl`
  — one directory per project checkout, one JSONL file per session, one JSON
  object per line (`type: "user" | "assistant" | ...`).
- Codex: `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` — one JSONL file per
  session/thread, JSON objects of `type: "session_meta" | "turn_context" |
  "event_msg" | "response_item"`.

Both roots are overridable (see `--claude-projects-dir` / `--codex-sessions-dir`)
so a different user on a different machine can point this at their own
history. Neither tool is assumed to be installed: a missing root is skipped
with a message on stderr, not an error.

## Detection heuristic (why not just grep every line for "ailly")

Every Claude Code session's system reminders and every Codex session's
`<skills_instructions>` block list *available* skills by name — including a
skill literally named "ailly" / "developer:ailly" — regardless of whether it
was ever used. A naive full-text search for "ailly" over raw message bodies
would treat nearly every session as a match. Instead:

- **Claude Code**: an assistant turn counts as Ailly-invoked when its
  `attributionSkill` or `attributionPlugin` field (populated by Claude Code
  itself only while a skill is actively driving the turn) contains "ailly".
  A user turn counts when its text contains a `<command-name>` tag whose
  value contains "ailly" (a real slash-command invocation, e.g.
  `/developer:ailly`), or a bare `/...ailly...`-shaped token.
- **Codex**: there is no per-turn attribution field, so the heuristic matches
  the *clean* `event_msg` user-message text (already stripped of the
  AGENTS.md/skills-catalog/environment-context boilerplate that
  `response_item` entries carry) against a small set of patterns that are
  specific to actually *doing* Ailly work: `developer:ailly`, a slash-style
  `/...ailly...` token, a `$ailly` token, an "Ailly ... phase" instruction
  (this operator's own Ailly-OODA harness phrases task handoffs this way),
  a `.ailly/` project-path reference, or an `ailly-`/`ailly_`-prefixed
  identifier (`ailly_two`, `ailly-skill-eval`, ...).

Both heuristics are necessarily approximate. False negatives (a real Ailly
turn phrased in some other way) are expected and fine — this is a mining
pass over a large corpus, not an exhaustive audit. False positives are
mitigated by human review at the labeling step.

## Turn segmentation

A "turn" is bounded by consecutive real human messages (Claude Code:
`type: "user"` entries that are not tool-result forwarding, not a synthetic
`isMeta` injection, and — for a top-level session file — not an inline
sidechain mirror; Codex: `event_msg` entries of type `user_message`). The
"candidate response" is the concatenation of the assistant's plain text
output during that turn (Claude Code: `text`-type content blocks off
assistant entries; Codex: `agent_message` events, preferring
`phase: "final_answer"` over `phase: "commentary"`), ignoring thinking
blocks and tool-call machinery — this mirrors what `check_judge` itself
grades (the final assistant turn's text).

Both tools give Task/Agent-tool sub-invocations their own dedicated
transcript: Codex writes a separate `rollout-*.jsonl` per spawned thread;
Claude Code writes `<project>/<session-uuid>/subagents/agent-*.jsonl`
alongside the parent session (in addition to mirroring the same content
inline in the parent, marked `isSidechain: true`, purely for the parent's
own rendering). This script walks both projects' directories recursively and
mines each sub-agent file as its own independent session — so a `Task`/
`Agent`-tool sub-invocation that itself drove Ailly is captured — while
skipping the inline sidechain mirror inside the parent file, since it is a
duplicate of what the dedicated sub-agent file already provides.

**Known scope limits** (flagged for human follow-up, not hidden):
- The human-implied-verdict keyword list is a small, hand-picked set of
  common approval/complaint phrasings in English. It will miss most real
  human judgments, which is why it is a *hint*, not a label.
- Codex subagent threads whose `model` is `codex-auto-review` (the "guardian"
  role's internal allow/deny gate over a proposed action) are excluded
  outright: their output is a fixed-shape verdict, never prose a `judge`
  assertion's rubric would meaningfully grade, and their input is often a
  raw diff that can incidentally mention an `.ailly/` path.

## Usage

    python3 mine_calibration_candidates.py \\
        [--claude-projects-dir PATH] [--codex-sessions-dir PATH] \\
        [--output-dir PATH] [--limit-per-source N]

Defaults: `~/.claude/projects`, `~/.codex/sessions`, and
`e2e/judge-calibration/mined/` next to this script. `--limit-per-source`
caps the number of *candidates* kept per source, for a quick smoke run.

## Output

Everything is written under `--output-dir` (gitignored):

- `candidates.jsonl` — one JSON object per mined candidate: id, source,
  provenance (source file/session/project cwd), timestamp, which detection
  signal fired, model metadata (or `null` if the transcript never recorded
  one), the question/response text, and an optional
  `candidate_label_human_implied` hint.
- `labels.yaml` — a draft flat `{id: TODO}` map in exactly the shape
  `feature-e-judge-calibration/design.md` specifies for
  `e2e/judge-calibration/evals/labels.yaml`, one entry per candidate, value
  always `TODO` (never pre-filled, including for human-implied hints — see
  module docstring above).
- `conversations/<id>.yaml` — one draft per candidate in `DESIGN.md`'s
  `conversation` multi-document YAML shape (a `model:`/`assembly:`/`binding:`
  header document followed by one document per message), ready to be
  reviewed and copied into a real `runs/<run-id>/` directory once curated.
- `mining_log.txt` — every skipped file/line and why, plus the same summary
  counts printed to stdout.

Re-running the script overwrites these files; it does not merge with a
prior run's manual edits, so curate into the real `evals/`/`runs/` tree
before re-running.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from dataclasses import asdict, dataclass
from pathlib import Path
from typing import Any, Iterator, Optional


# --------------------------------------------------------------------------
# Shared: implied-verdict keyword heuristic (used by both sources)
# --------------------------------------------------------------------------

_IMPLIED_NOTE = (
    "heuristic keyword match on the human's next message; NOT a confirmed "
    "label — a human must still confirm it"
)

NEGATIVE_IMPLIED_PATTERNS = [
    re.compile(p, re.IGNORECASE)
    for p in [
        r"\bthat'?s wrong\b",
        r"\bthat is wrong\b",
        r"\bthis is wrong\b",
        r"\bnot correct\b",
        r"\bincorrect\b",
        r"\bthat'?s not right\b",
        r"\bno,? that'?s not\b",
        r"\byou got it wrong\b",
        r"\bstill wrong\b",
        r"\bdoesn'?t work\b",
        r"\bdidn'?t work\b",
        r"\bthat failed\b",
        r"\brevert that\b",
        r"\bundo that\b",
        r"\bthat'?s a bug\b",
        r"\bthat broke\b",
    ]
]

POSITIVE_IMPLIED_PATTERNS = [
    re.compile(p, re.IGNORECASE)
    for p in [
        r"\bthat'?s correct\b",
        r"\bthat is correct\b",
        r"\bcorrect,? thanks\b",
        r"\blooks good\b",
        r"\blgtm\b",
        r"\bthat works\b",
        r"\bperfect\b",
        r"\bexactly right\b",
        r"\bnailed it\b",
        r"\byes,? that'?s right\b",
        r"\bgreat,? that'?s right\b",
        r"\bworks (?:great|perfectly|as expected)\b",
    ]
]


def detect_implied_verdict(next_human_text: Optional[str]) -> Optional[dict[str, str]]:
    """Look for an unambiguous human verdict in the *next* human message.

    Checks negative phrasings first so an accidental substring overlap never
    reports "pass" for a message that was actually a complaint.
    """
    if not next_human_text:
        return None
    for pat in NEGATIVE_IMPLIED_PATTERNS:
        m = pat.search(next_human_text)
        if m:
            return {
                "verdict": "fail",
                "evidence_phrase": m.group(0),
                "confidence": "low",
                "note": _IMPLIED_NOTE,
            }
    for pat in POSITIVE_IMPLIED_PATTERNS:
        m = pat.search(next_human_text)
        if m:
            return {
                "verdict": "pass",
                "evidence_phrase": m.group(0),
                "confidence": "low",
                "note": _IMPLIED_NOTE,
            }
    return None


# --------------------------------------------------------------------------
# Shared: candidate record + id assignment
# --------------------------------------------------------------------------


@dataclass
class Candidate:
    id: str
    source: str  # "claude-code" | "codex"
    source_file: str
    session_id: Optional[str]
    project_cwd: Optional[str]
    turn_started_at: Optional[str]
    invocation_signal: str
    model: Optional[str]
    question: str
    response: str
    agent_id: Optional[str] = None
    candidate_label_human_implied: Optional[dict[str, str]] = None
    label: str = "TODO"


def slugify(text: Optional[str], maxlen: int = 24) -> str:
    if not text:
        return "unknown"
    slug = re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")
    return (slug[:maxlen] or "unknown").strip("-") or "unknown"


def short_hash(text: str, length: int = 10) -> str:
    return hashlib.sha1(text.encode("utf-8")).hexdigest()[:length]


def file_slug(path: Path) -> str:
    """A short, collision-resistant slug identifying one transcript file.

    Deliberately hashes the full file path rather than truncating a session
    UUID: Codex mints its ids in a time-ordered scheme, so sessions started
    within the same short window can share an 8-hex-char prefix, and a
    Claude Code top-level session file can transiently observe a sidechain
    mirror's `agentId` that has nothing to do with the top-level thread's own
    identity. Both were confirmed, in an early run of this script, to
    produce real `id` collisions across distinct files. Hashing the
    (filesystem-unique) path sidesteps both failure modes entirely.
    """
    return short_hash(str(path))


def make_id(source: str, project_cwd: Optional[str], slug: str, seq: int) -> str:
    project_slug = slugify(Path(project_cwd).name if project_cwd else None)
    return f"{source}-{project_slug}-{slug}-{seq:03d}"


# --------------------------------------------------------------------------
# Robustness: a small logger that never lets a bad file/line crash the run
# --------------------------------------------------------------------------


class MiningLog:
    def __init__(self) -> None:
        self.lines: list[str] = []
        self.files_scanned = 0
        self.files_skipped = 0
        self.lines_skipped = 0

    def note(self, msg: str) -> None:
        self.lines.append(msg)

    def skip_file(self, path: Path, reason: str) -> None:
        self.files_skipped += 1
        self.note(f"SKIP FILE {path}: {reason}")

    def skip_line(self, path: Path, lineno: int, reason: str) -> None:
        self.lines_skipped += 1
        # Keep the log readable: only note the first few per file explicitly,
        # the running total per source is reported in the summary instead.
        if self.lines_skipped <= 200:
            self.note(f"SKIP LINE {path}:{lineno}: {reason}")

    def write(self, path: Path, extra_summary: str) -> None:
        path.write_text(extra_summary + "\n\n" + "\n".join(self.lines) + "\n")


def read_jsonl(path: Path, log: MiningLog) -> Iterator[dict[str, Any]]:
    """Yield parsed JSON objects from a JSONL file, skipping bad lines."""
    try:
        fh = path.open("r", encoding="utf-8", errors="replace")
    except OSError as exc:
        log.skip_file(path, f"could not open: {exc}")
        return
    log.files_scanned += 1
    with fh:
        for lineno, raw in enumerate(fh, start=1):
            raw = raw.strip()
            if not raw:
                continue
            try:
                obj = json.loads(raw)
            except json.JSONDecodeError as exc:
                log.skip_line(path, lineno, f"invalid JSON: {exc}")
                continue
            if not isinstance(obj, dict):
                log.skip_line(path, lineno, "top-level JSON value was not an object")
                continue
            yield obj


# --------------------------------------------------------------------------
# Claude Code mining
# --------------------------------------------------------------------------

_CLAUDE_COMMAND_NAME_RE = re.compile(r"<command-name>\s*([^<]+?)\s*</command-name>", re.IGNORECASE)
_CLAUDE_BARE_SLASH_RE = re.compile(r"(?<!\w)/[\w:.-]*ailly[\w:.-]*", re.IGNORECASE)


def _claude_user_signal(text: str) -> Optional[str]:
    for m in _CLAUDE_COMMAND_NAME_RE.finditer(text):
        if "ailly" in m.group(1).lower():
            return f"command-name tag: {m.group(1)!r}"
    m = _CLAUDE_BARE_SLASH_RE.search(text)
    if m:
        return f"bare slash token: {m.group(0)!r}"
    return None


def _claude_attribution_signal(obj: dict[str, Any]) -> Optional[str]:
    for field_name in ("attributionSkill", "attributionPlugin"):
        val = obj.get(field_name)
        if isinstance(val, str) and "ailly" in val.lower():
            return f"{field_name}={val!r}"
    return None


def _is_real_claude_user_turn(obj: dict[str, Any], allow_sidechain: bool = False) -> bool:
    if obj.get("type") != "user":
        return False
    if obj.get("isSidechain") and not allow_sidechain:
        return False
    if obj.get("isMeta"):
        return False
    if obj.get("toolUseResult") not in (None, ""):
        return False
    content = obj.get("message", {}).get("content")
    if isinstance(content, str):
        return True
    if isinstance(content, list):
        return not any(isinstance(b, dict) and b.get("type") == "tool_result" for b in content)
    return False


def _claude_user_text(obj: dict[str, Any]) -> str:
    content = obj.get("message", {}).get("content")
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        parts = [b.get("text", "") for b in content if isinstance(b, dict) and b.get("type") == "text"]
        return "\n".join(p for p in parts if p)
    return ""


def _claude_assistant_text_blocks(obj: dict[str, Any]) -> list[str]:
    content = obj.get("message", {}).get("content")
    if not isinstance(content, list):
        return []
    return [b.get("text", "") for b in content if isinstance(b, dict) and b.get("type") == "text" and b.get("text")]


def mine_claude_session_file(path: Path, log: MiningLog) -> list[Candidate]:
    # Claude Code stores each Task/Agent-tool sub-invocation's own transcript
    # as its own file under `<project>/<session-uuid>/subagents/agent-*.jsonl`
    # (in addition to mirroring it inline in the parent transcript, marked
    # `isSidechain: true`, purely for the parent's own rendering). Mining the
    # dedicated subagent file directly is simpler and more complete than
    # reconstructing nested sidechains from the parent's interleaved stream,
    # so: skip `isSidechain` entries when reading a top-level session file
    # (they are a redundant mirror handled by the subagent's own file), but
    # treat them as the primary thread when reading a `subagents/*.jsonl`
    # file directly (there, `isSidechain: true` is simply how every entry in
    # that file is marked, relative to its parent session).
    is_subagent_file = "subagents" in path.parts
    slug = file_slug(path)

    candidates: list[Candidate] = []
    session_id: Optional[str] = None
    agent_id: Optional[str] = None
    project_cwd: Optional[str] = None

    pending: Optional[dict[str, Any]] = None
    collected_texts: list[str] = []
    collected_signal: Optional[str] = None
    collected_models: set[str] = set()
    seq = 0

    def flush(next_human_text: Optional[str]) -> None:
        nonlocal seq
        if pending is None:
            return
        if collected_signal is None:
            return
        response_text = "\n\n".join(t.strip() for t in collected_texts if t.strip()).strip()
        if not response_text:
            return
        seq += 1
        model = sorted(collected_models)[0] if len(collected_models) == 1 else (
            "; ".join(sorted(collected_models)) if collected_models else None
        )
        cand = Candidate(
            id=make_id("claude-code", project_cwd, slug, seq),
            source="claude-code",
            source_file=str(path),
            session_id=session_id,
            project_cwd=project_cwd,
            turn_started_at=pending.get("timestamp"),
            invocation_signal=collected_signal,
            model=model,
            question=pending["text"],
            response=response_text,
            agent_id=agent_id,
        )
        cand.candidate_label_human_implied = detect_implied_verdict(next_human_text)
        candidates.append(cand)

    for obj in read_jsonl(path, log):
        session_id = session_id or obj.get("sessionId")
        project_cwd = project_cwd or obj.get("cwd")
        # Only trust `agentId` when this file *is* a dedicated subagent
        # transcript — a top-level file can carry inline sidechain mirrors
        # that have their own (unrelated) `agentId`, which must not be
        # attributed to the top-level thread's own identity.
        if is_subagent_file:
            agent_id = agent_id or obj.get("agentId")

        if _is_real_claude_user_turn(obj, allow_sidechain=is_subagent_file):
            text = _claude_user_text(obj)
            flush(next_human_text=text)
            pending = {"text": text, "timestamp": obj.get("timestamp")}
            collected_texts = []
            collected_signal = _claude_user_signal(text)
            collected_models = set()
            continue

        if obj.get("type") == "assistant" and (is_subagent_file or not obj.get("isSidechain")) and pending is not None:
            collected_texts.extend(_claude_assistant_text_blocks(obj))
            sig = _claude_attribution_signal(obj)
            if sig and collected_signal is None:
                collected_signal = sig
            model = obj.get("message", {}).get("model")
            if isinstance(model, str) and model:
                collected_models.add(model)

    flush(next_human_text=None)
    return candidates


def iter_claude_session_files(root: Path) -> Iterator[Path]:
    # `rglob` (not `glob`) because each session also has its own
    # `<session-uuid>/subagents/agent-*.jsonl` files for Task/Agent-tool
    # sub-invocations — real, independent agent-session transcripts in their
    # own right, and exactly the kind of session this script's brief asks it
    # to cover (this very analysis task, running as a `general-purpose`
    # subagent, would itself show up as one such file).
    for project_dir in sorted(p for p in root.iterdir() if p.is_dir()):
        yield from sorted(project_dir.rglob("*.jsonl"))


def mine_claude_projects(root: Path, log: MiningLog, limit: Optional[int]) -> list[Candidate]:
    if not root.exists():
        print(f"[mine] Claude Code projects dir not found, skipping: {root}", file=sys.stderr)
        return []
    all_candidates: list[Candidate] = []
    for session_file in iter_claude_session_files(root):
        all_candidates.extend(mine_claude_session_file(session_file, log))
        if limit is not None and len(all_candidates) >= limit:
            return all_candidates[:limit]
    return all_candidates


# --------------------------------------------------------------------------
# Codex mining
# --------------------------------------------------------------------------

# Internal Codex subagent models whose turns are never substantive answers to
# a human question (e.g. `codex-auto-review`, the "guardian" role's fixed
# allow/deny gate over a proposed action) — excluded outright regardless of
# whether their input text happens to match an invocation signal.
_CODEX_EXCLUDED_MODELS = {"codex-auto-review"}

_CODEX_SIGNAL_PATTERNS: list[tuple[str, re.Pattern[str]]] = [
    ("developer:ailly reference", re.compile(r"developer:ailly", re.IGNORECASE)),
    ("slash-style ailly command", re.compile(r"(?<!\w)/[\w:.-]*ailly[\w:.-]*", re.IGNORECASE)),
    ("$ailly token", re.compile(r"\$ailly\b", re.IGNORECASE)),
    ("ailly ... phase instruction", re.compile(r"\bailly\b[^.\n]{0,40}\bphase\b", re.IGNORECASE)),
    (".ailly project-path reference", re.compile(r"(?<![\w.])\.ailly/", re.IGNORECASE)),
    ("ailly-prefixed identifier", re.compile(r"\bailly[-_][a-z][\w-]*\b", re.IGNORECASE)),
]


def _codex_signal(text: str) -> Optional[str]:
    for label, pat in _CODEX_SIGNAL_PATTERNS:
        m = pat.search(text)
        if m:
            return f"{label}: {m.group(0)!r}"
    return None


def _codex_session_identity(obj: dict[str, Any]) -> tuple[Optional[str], Optional[str]]:
    """`(session_id, cwd)` from a `session_meta` entry.

    Prefers `payload.id` (this rollout file's own unique id) over
    `payload.session_id` (the shared parent-thread id across resumes/forks
    and spawned sub-agent threads) — see `file_slug`'s docstring for why a
    shared, time-ordered id is not safe to use for id assignment on its own.
    """
    payload = obj.get("payload", {})
    return payload.get("id") or payload.get("session_id"), payload.get("cwd")


def mine_codex_session_file(path: Path, log: MiningLog) -> list[Candidate]:
    slug = file_slug(path)
    candidates: list[Candidate] = []
    session_id: Optional[str] = None
    project_cwd: Optional[str] = None
    current_model: Optional[str] = None

    pending: Optional[dict[str, Any]] = None
    collected_final: list[str] = []
    collected_commentary: list[str] = []
    seq = 0

    def flush(next_human_text: Optional[str]) -> None:
        nonlocal seq
        if pending is None:
            return
        if pending.get("model") in _CODEX_EXCLUDED_MODELS:
            # `codex-auto-review` ("guardian" role) is Codex's own internal
            # approval gate over a proposed action, not a substantive answer
            # to a question — its "final_answer" is a fixed-shape verdict
            # like `{"outcome":"allow"}`, never natural-language prose a
            # `judge` assertion's rubric would meaningfully grade. Its input
            # is often a raw diff/context blob that can incidentally mention
            # an `.ailly/` path without the turn actually being Ailly work.
            return
        signal = _codex_signal(pending["text"])
        if signal is None:
            return
        response_text = "\n\n".join(t.strip() for t in (collected_final or collected_commentary) if t.strip()).strip()
        if not response_text:
            return
        seq += 1
        cand = Candidate(
            id=make_id("codex", project_cwd, slug, seq),
            source="codex",
            source_file=str(path),
            session_id=session_id,
            project_cwd=project_cwd,
            turn_started_at=pending.get("timestamp"),
            invocation_signal=signal,
            model=pending.get("model"),
            question=pending["text"],
            response=response_text,
        )
        cand.candidate_label_human_implied = detect_implied_verdict(next_human_text)
        candidates.append(cand)

    for obj in read_jsonl(path, log):
        t = obj.get("type")

        if t == "session_meta":
            found_session_id, found_cwd = _codex_session_identity(obj)
            session_id = session_id or found_session_id
            project_cwd = project_cwd or found_cwd
            continue

        if t == "turn_context":
            payload = obj.get("payload", {})
            model = payload.get("model")
            if isinstance(model, str) and model:
                current_model = model
            project_cwd = project_cwd or payload.get("cwd")
            continue

        if t != "event_msg":
            continue

        payload = obj.get("payload", {})
        ptype = payload.get("type")

        if ptype == "user_message":
            text = payload.get("message", "") or ""
            flush(next_human_text=text)
            pending = {"text": text, "timestamp": obj.get("timestamp"), "model": current_model}
            collected_final = []
            collected_commentary = []
            continue

        if ptype == "agent_message" and pending is not None:
            text = payload.get("message", "") or ""
            if not text:
                continue
            if payload.get("phase") == "final_answer":
                collected_final.append(text)
            else:
                collected_commentary.append(text)
            continue

    flush(next_human_text=None)
    return candidates


def iter_codex_session_files(root: Path) -> Iterator[Path]:
    yield from sorted(root.rglob("rollout-*.jsonl"))


def mine_codex_sessions(root: Path, log: MiningLog, limit: Optional[int]) -> list[Candidate]:
    if not root.exists():
        print(f"[mine] Codex sessions dir not found, skipping: {root}", file=sys.stderr)
        return []
    all_candidates: list[Candidate] = []
    for session_file in iter_codex_session_files(root):
        all_candidates.extend(mine_codex_session_file(session_file, log))
        if limit is not None and len(all_candidates) >= limit:
            return all_candidates[:limit]
    return all_candidates


# --------------------------------------------------------------------------
# Output
# --------------------------------------------------------------------------


def _yaml_double_quoted(text: str) -> str:
    """A JSON string literal is also a valid YAML double-quoted scalar."""
    return json.dumps(text)


def _yaml_block_literal(text: str, indent: int = 2) -> str:
    pad = " " * indent
    lines = text.replace("\r\n", "\n").replace("\r", "\n").split("\n")
    out = []
    for ln in lines:
        ln = ln.rstrip()
        out.append(pad + ln if ln else "")
    return "\n".join(out)


def write_conversation_draft(cand: Candidate, path: Path) -> None:
    model = cand.model or "unknown"
    header = (
        f"---\n"
        f"model: {_yaml_double_quoted(model)}  # mined draft — verify against a real ailly ModelId before use\n"
        f"---\n"
        f"role: user\n"
        f"content: |\n"
        f"{_yaml_block_literal(cand.question)}\n"
        f"---\n"
        f"role: assistant\n"
        f"content: |\n"
        f"{_yaml_block_literal(cand.response)}\n"
    )
    path.write_text(header)


def write_outputs(candidates: list[Candidate], output_dir: Path, log: MiningLog) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    conversations_dir = output_dir / "conversations"
    conversations_dir.mkdir(parents=True, exist_ok=True)

    with (output_dir / "candidates.jsonl").open("w", encoding="utf-8") as fh:
        for cand in candidates:
            fh.write(json.dumps(asdict(cand), ensure_ascii=False) + "\n")

    labels_lines = [
        "# DRAFT — mined candidate labels for e2e/judge-calibration.",
        "#",
        "# Fill each value with `pass` or `fail` after reviewing the matching",
        "# conversation under mined/conversations/<id>.yaml (full provenance and any",
        "# low-confidence human-implied hint live in mined/candidates.jsonl). This",
        "# file is itself local-only scratch (see the e2e/judge-calibration/mined/",
        "# .gitignore entry) until a human curates a final",
        "# e2e/judge-calibration/evals/labels.yaml from a reviewed subset of these.",
        "#",
        "# Shape matches feature-e-judge-calibration/design.md's labels.yaml: a flat",
        "# { id: pass|fail } map.",
        "",
    ]
    for cand in candidates:
        labels_lines.append(f"{cand.id}: TODO")
    (output_dir / "labels.yaml").write_text("\n".join(labels_lines) + "\n")

    for cand in candidates:
        write_conversation_draft(cand, conversations_dir / f"{cand.id}.yaml")

    by_source: dict[str, int] = {}
    with_model = 0
    with_implied = 0
    for cand in candidates:
        by_source[cand.source] = by_source.get(cand.source, 0) + 1
        if cand.model:
            with_model += 1
        if cand.candidate_label_human_implied:
            with_implied += 1

    summary_lines = [
        "Judge-calibration candidate mining run summary",
        "===============================================",
        f"Total candidates: {len(candidates)}",
    ]
    for source, count in sorted(by_source.items()):
        summary_lines.append(f"  {source}: {count}")
    summary_lines += [
        f"Model metadata present: {with_model}/{len(candidates)}",
        f"Low-confidence human-implied verdict hints: {with_implied}/{len(candidates)}",
        f"Files scanned: {log.files_scanned}",
        f"Files skipped (unreadable): {log.files_skipped}",
        f"Lines skipped (malformed JSON): {log.lines_skipped}",
    ]
    summary = "\n".join(summary_lines)
    print(summary)
    log.write(output_dir / "mining_log.txt", summary)


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------


def default_output_dir() -> Path:
    # scripts/ -> evals/ -> judge-calibration/ -> mined/
    return Path(__file__).resolve().parents[2] / "mined"


def parse_args(argv: Optional[list[str]] = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0], formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument(
        "--claude-projects-dir",
        type=Path,
        default=Path.home() / ".claude" / "projects",
        help="Root of Claude Code's per-project session transcripts (default: ~/.claude/projects)",
    )
    parser.add_argument(
        "--codex-sessions-dir",
        type=Path,
        default=Path.home() / ".codex" / "sessions",
        help="Root of Codex's dated session rollouts (default: ~/.codex/sessions)",
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=None,
        help="Where to write mined output (default: e2e/judge-calibration/mined/ next to this script)",
    )
    parser.add_argument(
        "--limit-per-source",
        type=int,
        default=None,
        help="Cap the number of candidates kept per source, for a quick smoke run",
    )
    return parser.parse_args(argv)


def main(argv: Optional[list[str]] = None) -> int:
    args = parse_args(argv)
    output_dir = args.output_dir or default_output_dir()

    log = MiningLog()
    claude_candidates = mine_claude_projects(args.claude_projects_dir, log, args.limit_per_source)
    codex_candidates = mine_codex_sessions(args.codex_sessions_dir, log, args.limit_per_source)

    all_candidates = claude_candidates + codex_candidates
    if not all_candidates:
        print(
            "[mine] No candidates found. Either neither source directory exists, or no "
            "session in either source matched the Ailly-invocation heuristics.",
            file=sys.stderr,
        )
    write_outputs(all_candidates, output_dir, log)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
