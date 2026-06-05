"""Shared utilities for the per-domain corruption scorers.

Each delegate-52 scorer extracts its seed's load-bearing facts in a
domain-specific way, then checks every fact survived verbatim in the candidate
(the final assistant turn's document, read from stdin). The extraction differs
per domain; the de-duplication and the survive-or-fail loop do not. Those two
common pieces live here.

This module is delegate-52-specific. It sits alongside `_checker_utils.py`,
which is copied verbatim from patterns-eval and provides `fail` /
`extract_code`; keeping the two separate preserves that file's provenance.
"""

from collections.abc import Iterable

from _checker_utils import fail


def ordered_unique(items: Iterable[str]) -> list[str]:
    """Return the items in first-seen order with later duplicates removed.

    Replaces each scorer's hand-rolled seen-set closure so the fact list stays
    ordered (most corruption-prone facts checked first) without repeats.
    """
    seen: set[str] = set()
    result: list[str] = []
    for item in items:
        if item and item not in seen:
            seen.add(item)
            result.append(item)
    return result


def check_facts(domain: str, candidate: str, facts: Iterable[str]) -> int:
    """Confirm every fact survives verbatim in the candidate.

    On the first fact missing from the candidate, write a single-line reason to
    stdout via `fail` and return 1; if every fact survives, return 0. stderr is
    never touched, so the eval runner records Fail, not Errored.
    """
    for fact in facts:
        if fact not in candidate:
            return fail(f"{domain}: load-bearing fact dropped or altered: {fact!r}")
    return 0
