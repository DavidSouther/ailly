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

R3 checks the SHAPE of the attached field keys (a dotted, namespaced
identifier -- OpenTelemetry semantic-convention style, e.g. `order.id`,
`refund.amount`, `http.response.status_code`), not a fixed field-name list.
The one canonical example in the skill (`order.placed` / `order.id` /
`user.id` / `http.response.status_code`) is illustrative, not a whitelist:
a refund handler legitimately has no `order.id`, and a non-HTTP handler
legitimately has no `http.response.status_code`. A candidate that invents
flat, unnamespaced keys (`orderId`, `httpStatusCode`) fails R3 regardless of
which domain event it is answering.
"""

import re
import sys

from _checker_utils import extract_code, fail, strip_comments

# A dotted, namespaced field key: two or more lowercase-leading segments
# joined by `.` (e.g. `order.id`, `carrier.tracking.status_code`). This is
# the SHAPE OpenTelemetry semantic-convention keys share; it is deliberately
# name-agnostic so it accepts any domain's equivalent of `order.id` /
# `user.id` / `http.response.status_code` without hard-coding those literals.
_SEGMENT = r"[a-z][a-zA-Z0-9_]*"
_DOTTED_KEY = rf"{_SEGMENT}(?:\.{_SEGMENT})+"

# Quoted object-literal key immediately followed by `:` — the TS/JS/Python
# dict-style call sites in this suite's candidates use (`"order.id": ...`).
_QUOTED_KEY_RE = re.compile(rf'["\']({_DOTTED_KEY})["\']\s*:')
# Rust/tracing-macro-style field path immediately followed by `=` (and not
# `==`, an equality comparison) — the style the skill's Rust example uses
# (`http.response.status_code = 201`).
_MACRO_FIELD_RE = re.compile(rf"\b({_DOTTED_KEY})\s*=(?!=)")

# Minimum count of distinct dotted, namespaced field keys required. The
# canonical example alone carries three (`order.id`, `user.id`,
# `http.response.status_code`); two is the floor that still distinguishes
# "uses semantic-convention-shaped keys" from "one key happens to have a dot
# in it by coincidence".
_MIN_NAMESPACED_KEYS = 2


def _namespaced_keys(src: str) -> set[str]:
    """Distinct dotted, namespaced field keys found via either call-site style."""
    return {m.group(1) for m in _QUOTED_KEY_RE.finditer(src)} | {
        m.group(1) for m in _MACRO_FIELD_RE.finditer(src)
    }


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

    # R3 — semantic-convention-shaped keys (dotted, namespaced field names),
    # not a fixed field-name list. See the module docstring.
    found = _namespaced_keys(src)
    if len(found) < _MIN_NAMESPACED_KEYS:
        return fail(
            f"R3 semantic-convention keys: found {len(found)} dotted, namespaced "
            f"field key(s) ({', '.join(sorted(found)) or 'none'}); expected at "
            f"least {_MIN_NAMESPACED_KEYS} keys shaped like OpenTelemetry "
            "semantic conventions (e.g. `order.id`, `http.response.status_code`), "
            "not ad-hoc flat/camelCase field names"
        )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
