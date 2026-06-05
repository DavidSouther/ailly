#!/usr/bin/env python3
"""Corruption scorer for the `notation-music` domain.

Ported in spirit from microsoft/DELEGATE52: a notation fragment's load-bearing
facts are its time signature, its tempo, its pitches, its note durations, and
its dynamics markings. The paper's failure mode here is a pitch or duration that
shifts during a "clean up the notation" edit while the measure still scans.

Reads the candidate (the final assistant turn's document) from stdin and the
seed from `context/seeds/notation-music.md`, resolved relative to the
project-root working directory. `program` assertions take no arguments; the seed
path is hardcoded (see the README fidelity notes).

Extracts the seed's facts structurally at fixture scale: the time signature
(`N/N`), the tempo (`N BPM`), scientific-pitch tokens (`C4`, `E4`, `G4`), the
note durations that follow them (`quarter`, `half`), and the dynamics marking
after `Dynamics:`. Each must survive verbatim in the candidate. On the first
missing fact it prints a single-line reason to stdout and exits 1; if all
survive it exits 0. stderr is never written to.
"""

import re
import sys
from pathlib import Path

from _checker_utils import fail

SEED_PATH = Path("context/seeds/notation-music.md")

TIME_SIGNATURE = re.compile(r"\b\d+/\d+\b")
TEMPO = re.compile(r"\b\d+\s*BPM\b")
PITCH = re.compile(r"\b[A-G](?:#|b)?\d\b")
DURATION = re.compile(r"\b(?:whole|half|quarter|eighth|sixteenth)\b")
DYNAMICS = re.compile(r"\bDynamics:\s*([a-z]+)\b")


def seed_facts(seed: str) -> list[str]:
    """Ordered, de-duplicated load-bearing facts extracted from the seed."""
    facts: list[str] = []
    seen: set[str] = set()

    def add(fact: str) -> None:
        if fact and fact not in seen:
            seen.add(fact)
            facts.append(fact)

    for match in TIME_SIGNATURE.findall(seed):
        add(match)
    for match in TEMPO.findall(seed):
        add(match)
    for match in PITCH.findall(seed):
        add(match)
    for match in DURATION.findall(seed):
        add(match)
    dynamics = DYNAMICS.search(seed)
    if dynamics:
        add(dynamics.group(1))
    return facts


def main() -> int:
    candidate = sys.stdin.read()
    seed = Path(SEED_PATH).read_text(encoding="utf-8")

    for fact in seed_facts(seed):
        if fact not in candidate:
            return fail(
                f"notation-music: load-bearing fact dropped or altered: {fact!r}"
            )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
