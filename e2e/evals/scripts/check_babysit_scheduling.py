#!/usr/bin/env python3
"""Check waiting, confirmation, and denial principles; no live scheduler calls."""

import re
import sys

from _checker_utils import AILLY_DIR, fail, read


def main() -> int:
    reference = AILLY_DIR / "references" / "shapes" / "babysit.md"
    if not reference.is_file():
        return fail("Babysit scheduling: shape reference is missing")
    text = read(reference)
    contracts = (
        ((
            r"(?:waiting|polling)[^.\n]*read.only",
            r"(?:not restrict|does not restrict|must not restrict)[^.\n]*Watch Loop",
            r"fixes[^.\n]*re.arm[^.\n]*cancellation[^.\n]*(?:approval|permission)",
        ), "keep waiting read-only without restricting approved session work"),
        ((
            r"confirm[^.\n]*(?:started|running|success)",
            r"(?:record|persist)[^.\n]*handle[^.\n]*babysit-state\.md",
            r"before[^.\n]*(?:claim|report)[^.\n]*armed",
            r"re.arm[^.\n]*pending",
        ), "confirm and record a poll before claiming armed; re-arm while pending"),
        ((
            r"denial[^.\n]*(?:blocks|applies)[^.\n]*outcome",
            r"(?:record|persist)[^.\n]*(?:denial|denied)[^.\n]*reason",
            r"(?:continue|continuing)[^.\n]*independent authorized work",
            r"(?:do not|never)[^.\n]*change permissions[^.\n]*disguise[^.\n]*denied",
            r"unresolved denials",
            r"incomplete[^.\n]*no (?:timer|poll) running",
        ), "preserve denied outcomes, continue independent work, and report unarmed waits truthfully"),
    )
    for patterns, reason in contracts:
        if not all(re.search(pattern, text, re.IGNORECASE) for pattern in patterns):
            return fail(f"Babysit scheduling: {reason}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
