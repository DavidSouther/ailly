#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["pyyaml"]
# ///
"""Local-only: confirm no corpus command was copied out of a real transcript.

    uv run --script shell-access/corpus/check_not_copied.py

This is a maintainer utility, deliberately **not** a CI job. It reads local
agent session files, which exist on a maintainer's machine and on no build
machine, so a CI run would always trivially pass and would only be a way to
believe the check happened. Run it after editing the corpus.

It fails when a corpus `command` is identical, after whitespace normalization,
to a command some local session recorded. The corpus is meant to be written from the
*shapes* those sessions show, on synthetic paths; an exact match means an
example was pasted rather than rewritten.

Nothing recorded is printed. A failure names the corpus case, because that is
the thing to rewrite.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import yaml

CORPUS = Path(__file__).resolve().parent

SESSION_ROOTS = (
    Path.home() / ".claude" / "projects",
    Path.home() / ".codex" / "sessions",
    Path.home() / ".pi" / "agent" / "sessions",
)

#: Keys every harness uses for the text it handed a shell.
COMMAND_KEYS = ("command", "cmd")


def normalized(command: str) -> str:
    return " ".join(command.split())


def corpus_commands() -> dict[str, str]:
    """Normalized command text keyed by the case id that holds it."""
    by_command = {}
    for path in sorted(CORPUS.glob("*.yaml")):
        documents = list(yaml.safe_load_all(path.read_text(encoding="utf-8")))
        cases = documents[-1] if documents else {}
        for case_id, case in (cases or {}).items():
            command = (case or {}).get("command")
            if isinstance(command, str):
                by_command[normalized(command)] = f"{path.name}:{case_id}"
    return by_command


def recorded_commands(payload: object, into: set[str]) -> None:
    """Every command string anywhere in one transcript record."""
    if isinstance(payload, dict):
        for key, value in payload.items():
            if key in COMMAND_KEYS:
                if isinstance(value, str):
                    into.add(normalized(value))
                elif isinstance(value, list) and all(isinstance(v, str) for v in value):
                    into.add(normalized(" ".join(value)))
            recorded_commands(value, into)
    elif isinstance(payload, list):
        for item in payload:
            recorded_commands(item, into)


def main() -> int:
    roots = [root for root in SESSION_ROOTS if root.is_dir()]
    if not roots:
        print("no local sessions to compare against; nothing to check")
        return 0

    recorded: set[str] = set()
    transcripts = 0
    for root in roots:
        for path in root.rglob("*.jsonl"):
            transcripts += 1
            with path.open(encoding="utf-8", errors="replace") as handle:
                for line in handle:
                    line = line.strip()
                    if not line:
                        continue
                    try:
                        recorded_commands(json.loads(line), recorded)
                    except json.JSONDecodeError:
                        continue

    copied = [
        (case, command)
        for command, case in corpus_commands().items()
        if command in recorded
    ]
    for case, command in sorted(copied):
        print(
            f"{case}: command matches a recorded one once whitespace is normalized; rewrite it",
            file=sys.stderr,
        )
    if copied:
        return 1
    print(f"no corpus command matches any of {len(recorded)} commands in {transcripts} transcripts")
    return 0


if __name__ == "__main__":
    sys.exit(main())
