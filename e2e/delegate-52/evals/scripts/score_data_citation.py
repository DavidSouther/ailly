#!/usr/bin/env python3
"""Corruption scorer for the `data-citation` domain.

Ported in spirit from microsoft/DELEGATE52: a citation's load-bearing facts are
its author surname, its year, its venue, its page range, and its DOI. The
paper's failure mode is a transposed page range or a DOI digit that flips during
a "standardise the formatting" edit while the citation still looks well-formed.

Reads the candidate (the final assistant turn's document) from stdin and the
seed from `context/seeds/data-citation.md`, resolved relative to the
project-root working directory. `program` assertions take no arguments; the seed
path is hardcoded (see the README fidelity notes).

Extracts the seed's facts structurally at fixture scale: the DOI (after the
`doi:` marker), the volume/issue/page span (`13(6), 377-387`), the standalone
year, and the leading author surname. Each must survive verbatim in the
candidate. On the first missing fact it prints a single-line reason to stdout
and exits 1; if all survive it exits 0. stderr is never written to.
"""

import re
import sys
from pathlib import Path

from _scorer_utils import check_facts, ordered_unique

SEED_PATH = Path("context/seeds/data-citation.md")

DOI = re.compile(r"\b10\.\d{4,9}/[-._;()/:A-Za-z0-9]+")
PAGE_SPAN = re.compile(r"\b\d+\(\d+\),\s*\d+-\d+")
YEAR = re.compile(r"\((\d{4})\)")
AUTHOR = re.compile(r"^([A-Z][a-z]+),", re.MULTILINE)


def seed_facts(seed: str) -> list[str]:
    """Ordered, de-duplicated load-bearing facts extracted from the seed.

    DOI first (most corruption-prone), then the page span, the year, and the
    leading author surname.
    """
    # The DOI char class admits `.`, so a sentence-ending period after the DOI
    # is captured; a DOI never ends in a bare period, so trim it.
    dois = [match.rstrip(".") for match in DOI.findall(seed)]
    page_spans = PAGE_SPAN.findall(seed)
    years = YEAR.findall(seed)
    author = AUTHOR.search(seed)
    authors = [author.group(1)] if author else []
    return ordered_unique([*dois, *page_spans, *years, *authors])


def main() -> int:
    candidate = sys.stdin.read()
    seed = SEED_PATH.read_text(encoding="utf-8")
    return check_facts("data-citation", candidate, seed_facts(seed))


if __name__ == "__main__":
    raise SystemExit(main())
