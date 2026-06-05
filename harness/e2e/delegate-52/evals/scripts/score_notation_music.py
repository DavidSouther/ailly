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

from _scorer_utils import check_facts, ordered_unique

SEED_PATH = Path("context/seeds/notation-music.md")

TIME_SIGNATURE = re.compile(r"\b\d+/\d+\b")
TEMPO = re.compile(r"\b\d+\s*BPM\b")
PITCH = re.compile(r"\b[A-G](?:#|b)?\d\b")
DURATION = re.compile(r"\b(?:whole|half|quarter|eighth|sixteenth)\b")
DYNAMICS = re.compile(r"\bDynamics:\s*([a-z]+)\b")


def seed_facts(seed: str) -> list[str]:
    """Ordered, de-duplicated load-bearing facts extracted from the seed."""
    time_signatures = TIME_SIGNATURE.findall(seed)
    tempi = TEMPO.findall(seed)
    pitches = PITCH.findall(seed)
    durations = DURATION.findall(seed)
    dynamics = DYNAMICS.search(seed)
    markings = [dynamics.group(1)] if dynamics else []
    return ordered_unique([*time_signatures, *tempi, *pitches, *durations, *markings])


def main() -> int:
    candidate = sys.stdin.read()
    seed = SEED_PATH.read_text(encoding="utf-8")
    return check_facts("notation-music", candidate, seed_facts(seed))


if __name__ == "__main__":
    raise SystemExit(main())
