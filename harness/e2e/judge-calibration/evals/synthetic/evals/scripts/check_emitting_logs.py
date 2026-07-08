#!/usr/bin/env python3
"""Structural checker for `patterns:emitting-logs` invocation cases.

Reads a TypeScript candidate from stdin and applies an ordered list of rules,
each tracing 1:1 to a bullet in the emitting-logs SKILL.md "Common Mistakes"
section. On the first violated rule it prints a single-line reason to stdout and
exits 1; if every rule holds it exits 0. stderr is left untouched so the eval
runner records a genuine Fail, never an `Errored` broken checker.

Rules:
- R1 stable message body ("String interpolation in the message body").
- R2 event name set ("Skipping `EventName` on a business outcome").
- R3 semantic-convention keys ("Ad-hoc field names").
"""

import re
import sys

from _checker_utils import extract_code, fail, strip_comments


def main() -> int:
    src = strip_comments(extract_code(sys.stdin.read()))

    # R1 — stable message body.
    if re.search(r"`[^`]*\$\{", src):
        return fail(
            "R1 stable message body: an interpolated `${...}` template literal "
            "flattens the structured event into free text; keep the body stable and "
            "move values to fields"
        )

    # R2 — event name set (eventName / EventName / event_name).
    if not re.search(r"\bevent[_]?name\b", src, re.IGNORECASE):
        return fail(
            "R2 event name set: no `eventName` field; without it the backend has a "
            "body but no name to count the business outcome by"
        )

    # R3 — semantic-convention keys.
    missing = [
        key
        for key in ("order.id", "user.id", "http.response.status_code")
        if key not in src
    ]
    if missing:
        return fail(
            f"R3 semantic-convention keys: missing {', '.join(missing)}; use the "
            "OpenTelemetry semantic-convention keys, not ad-hoc field names"
        )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
