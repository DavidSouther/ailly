#!/usr/bin/env python3
"""Structural checker for a Clean Comments Review critique.

Reads the assistant critique from stdin and applies three ordered rules, each
the projection of one bullet in the clean-comments-review SKILL.md "Common
Mistakes" section. On the first violated rule it prints a single-line reason to
stdout and exits 1; if every rule holds it exits 0. stderr is left untouched so
the eval runner records a genuine Fail, never an `Errored` broken checker.

The material under review is a critique document (prose), not code, so this
checker keys on the critique text directly rather than extracting fenced code.
It discriminates the skilled, audience-aware critique (passes) from a baseline
"the comments look thorough, no changes needed" response (fails R1).
"""

import re
import sys

from _checker_utils import fail


def check(text: str) -> int:
    lower = text.lower()

    # R1 (audience) <- "Ignoring the comment's audience". The critique
    # classifies the comment by audience: it names the public DocBlock and its
    # external reader. The baseline ("the comments are thorough ... no changes
    # needed") names neither.
    names_docblock = re.search(r"doc\s*block", lower) is not None
    names_reader = re.search(r"external reader|\baudience\b", lower) is not None
    if not (names_docblock and names_reader):
        return fail(
            "R1: critique does not classify the comment's audience "
            "(public DocBlock / external reader)"
        )

    # R2 (usage-enumeration rot) <- "Endorsing rot-prone usage enumeration".
    # The critique ties enumerating current usage or call sites to rot/drift. A
    # rot term is required, so the baseline's bare "how the prop is used today"
    # does not satisfy it.
    rot_term = re.search(r"\brots?\b|\bdrift|\bstale\b|out[- ]?dated|out of date", lower) is not None
    usage_ref = re.search(r"call ?sites?|\bcallers?\b|\busage\b|\bused\b", lower) is not None
    if not (rot_term and usage_ref):
        return fail(
            "R2: critique does not flag that enumerating current usage / "
            "call sites will rot"
        )

    # R3 (reduction, not endorsement) <- "Recommending no change when reduction
    # is warranted". The critique recommends cutting / removing to intent and
    # does not conclude that no change is needed. The baseline endorses ("no
    # changes are needed").
    endorses = re.search(
        r"no changes?\s+(are\s+|is\s+)?(needed|necessary|required)"
        r"|looks? (complete|thorough|fine|good)"
        r"|leave (it|them|as)",
        lower,
    ) is not None
    recommends_reduction = re.search(r"\bcut\b|\bremove\b|\btrim\b|\breduce\b|\bdelete\b", lower) is not None
    if endorses or not recommends_reduction:
        return fail(
            "R3: critique endorses the over-documentation instead of "
            "recommending reduction to intent"
        )

    return 0


def main() -> int:
    return check(sys.stdin.read())


if __name__ == "__main__":
    sys.exit(main())
