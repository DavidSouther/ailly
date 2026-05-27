#!/usr/bin/env python3
"""Structural checker for `patterns:emitting-logs` invocation cases.

The eventual rule set:
- Structured fields only (no string interpolation in the message body).
- `EventName` set on business events.
- OpenTelemetry semantic-convention keys on attached fields.

Encoding those rules is deferred to the `knowledge: eval-script` slice;
until then this placeholder reads stdin and writes a placeholder verdict.
"""

import sys


def main() -> int:
    _ = sys.stdin.read()
    _ = sys.stdout.write('{"status": "placeholder", "reason": "eval-script not yet wired"}\n')
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
