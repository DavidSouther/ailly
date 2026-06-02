#!/usr/bin/env python3
"""Structural checker for `patterns:configuring-logging` invocation cases.

Reads a TypeScript candidate from stdin and applies an ordered list of rules,
each tracing 1:1 to a bullet in the configuring-logging SKILL.md "Common
Mistakes" section. On the first violated rule it prints a single-line reason to
stdout and exits 1; if every rule holds it exits 0. stderr is left untouched so
the eval runner records a genuine Fail, never an `Errored` broken checker.

The rules are concept-based, not keyed to one library's class names: a bootstrap
install may be `.install()`, `.init()`, `setGlobalLoggerProvider(...)`, or an SDK
`.start()`; the pipeline layers are matched by keyword family so a hand-rolled
`Format/Filter/Enrich/Export` registry and an OpenTelemetry SDK both validate.
What fails is a missing layer, missing `service.*` resource attributes, or a
missing shutdown flush.

Rules:
- R1 a bootstrap entry point ("Re-initializing from a library").
- R2 full pipeline breadth ("Three layers instead of five").
- R3 `service.*` resource attributes ("Resource attributes attached per record").
- R4 shutdown flush registered ("Missing flush on shutdown").
"""

import re
import sys

INSTALL = re.compile(
    r"\.install\s*\(|\.init\s*\(|\bsetGlobalLogger\w*\s*\(|\bsetLoggerProvider\s*\("
    r"|\.start\s*\(|\bregisterGlobal\w*\s*\(|\b(?:logs|trace)\.setGlobal\w*\s*\("
)

LAYERS = {
    "Format": re.compile(r"\b(?:format|formatter|json|layout|fmt|pretty|encode)\w*", re.I),
    "Filter": re.compile(r"\b(?:filter|loglevel|fromenv|envfilter|sampl)\w*", re.I),
    "Enrich": re.compile(r"\b(?:enrich|resource|attribute|traceparent|propagat|baggage)\w*", re.I),
    "Export": re.compile(r"\b(?:export|exporter|otlp|collector|endpoint|processor)\w*", re.I),
}

FLUSH = re.compile(r"\b(?:shutdown|flush|forceFlush)\s*\(")

FENCE = re.compile(r"```[^\n]*\n(.*?)```", re.DOTALL)


def extract_code(src: str) -> str:
    """The runner pipes the whole assistant message: prose, tables, and fenced
    code. Validate the code, not the explanation. Concatenate fenced blocks; if
    there are none (e.g. a raw-source candidate), use the whole input."""
    blocks = FENCE.findall(src)
    return "\n".join(blocks) if blocks else src


def strip_comments(src: str) -> str:
    """Remove block and line comments so a keyword that appears only in prose
    does not satisfy a rule. `://` (URLs) is preserved."""
    src = re.sub(r"/\*.*?\*/", " ", src, flags=re.DOTALL)
    src = re.sub(r"(?<!:)//[^\n]*", " ", src)
    return src


def fail(reason: str) -> int:
    """Print the single-line reason to stdout and signal a failed candidate.

    Leaves stderr untouched so the runner records Fail, not Errored.
    """
    sys.stdout.write(reason + "\n")
    return 1


def main() -> int:
    src = strip_comments(extract_code(sys.stdin.read()))

    # R1 — a bootstrap entry point.
    if not INSTALL.search(src):
        return fail(
            "R1 bootstrap entry point: no recognizable global install "
            "(`install`/`init`/`setGlobalLoggerProvider`/SDK `start`); the "
            "application must install the global subscriber once at bootstrap"
        )

    # R2 — full pipeline breadth.
    missing = [name for name, pat in LAYERS.items() if not pat.search(src)]
    if missing:
        return fail(
            f"R2 full pipeline breadth: missing {', '.join(missing)}; a pipeline "
            "short of Format/Filter/Enrich/Export loses resource attributes or the "
            "path to a collector"
        )

    # R3 — service.* resource attributes.
    if not re.search(r"service\.\w+", src):
        return fail(
            "R3 service.* resource attributes: no `service.*` attribute attached at "
            "the resource; resource attributes belong on the enrich layer, once"
        )

    # R4 — shutdown flush registered.
    if not FLUSH.search(src):
        return fail(
            "R4 shutdown flush registered: no `shutdown`/`flush(<timeout>)` handler; "
            "a batching pipeline that never flushes loses the last interval"
        )

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
