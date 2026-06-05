#!/usr/bin/env python3
"""Corruption scorer for the `prose-bio` domain.

Ported in spirit from microsoft/DELEGATE52: a biographical note's load-bearing
facts are its named entities, full dates, and standalone years. The paper's
"sparse but severe" failure mode is a date or name that quietly migrates across
a delegated edit while the prose still reads cleanly.

Reads the candidate (the final assistant turn's document) from stdin and the
seed from `context/seeds/prose-bio.md`, resolved relative to the subprocess
working directory, which `eval_run` sets to the project root (`e2e/delegate-52`).
`program` assertions take no arguments, so the seed path is hardcoded here, not
passed as argv (see the README fidelity notes).

Extracts the seed's dates and years structurally at fixture scale (full dates
`DD Month YYYY` and standalone four-digit years), and pairs them with a small
explicit list of named entities. A general proper-noun NER is deliberately
avoided: at fixture scale the named entities are few and known, and the explicit
list is guarded against drifting away from the seed by asserting each entity is
present in the seed before it is checked against the candidate. On the first
fact missing from the candidate verbatim it prints a single-line reason to
stdout and exits 1; if every fact survives it exits 0. stderr is never written
to, so the runner records Fail, not Errored.
"""

import re
import sys
from pathlib import Path

from _checker_utils import fail

SEED_PATH = Path("context/seeds/prose-bio.md")

FULL_DATE = re.compile(
    r"\b\d{1,2}\s+"
    r"(?:January|February|March|April|May|June|July|August|September|October|"
    r"November|December)\s+\d{4}\b"
)
YEAR = re.compile(r"\b\d{4}\b")

# The biography's load-bearing named entities. Each is verified present in the
# seed (so the list cannot silently diverge from the seed document) before it is
# required of the candidate. A general NER is overkill at fixture scale and
# fragile across line breaks and the markdown title.
NAMED_ENTITIES = ("Ada Lovelace", "London", "Charles Babbage", "Analytical Engine")


def seed_facts(seed: str) -> list[str]:
    """Ordered, de-duplicated load-bearing facts present in the seed.

    Full dates first (most corruption-prone), then the standalone publication
    year, then the named entities that the seed actually contains.
    """
    facts: list[str] = []
    seen: set[str] = set()

    def add(fact: str) -> None:
        if fact and fact not in seen:
            seen.add(fact)
            facts.append(fact)

    dates = FULL_DATE.findall(seed)
    for date in dates:
        add(date)
    # Years already inside a full date are covered; add the rest (e.g. 1843).
    date_blob = " ".join(dates)
    for year in YEAR.findall(seed):
        if year not in date_blob:
            add(year)
    # Whitespace-normalize the seed so a name split across a wrapped line still
    # counts as present.
    seed_flat = re.sub(r"\s+", " ", seed)
    for entity in NAMED_ENTITIES:
        if entity in seed_flat:
            add(entity)
    return facts


def main() -> int:
    candidate = re.sub(r"\s+", " ", sys.stdin.read())
    seed = SEED_PATH.read_text(encoding="utf-8")

    for fact in seed_facts(seed):
        if fact not in candidate:
            return fail(f"prose-bio: load-bearing fact dropped or altered: {fact!r}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
