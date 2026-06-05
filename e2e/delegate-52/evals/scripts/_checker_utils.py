"""Shared utilities for structural checker scripts.

Each checker reads a TypeScript candidate from stdin, applies ordered rules, and
either exits 0 (all rules hold) or exits 1 with a single-line reason on stdout.
stderr is never written to so the eval runner records Fail, not Errored.
"""

import re
import sys

FENCE = re.compile(r"```[^\n]*\n(.*?)```", re.DOTALL)


def extract_code(src: str) -> str:
    """Return fenced code blocks joined together, or the whole input if none.

    The runner pipes the whole assistant message (prose, tables, fenced code).
    Validate the code, not the explanation.
    """
    blocks = FENCE.findall(src)
    return "\n".join(blocks) if blocks else src


def strip_comments(src: str) -> str:
    """Remove block and line comments so comment prose cannot satisfy a rule.

    `://` (URLs) is preserved via the negative lookbehind on `//`.
    """
    src = re.sub(r"/\*.*?\*/", " ", src, flags=re.DOTALL)
    src = re.sub(r"(?<!:)//[^\n]*", " ", src)
    return src


def fail(reason: str) -> int:
    """Write a single-line reason to stdout and return exit code 1.

    Leaves stderr untouched so the runner records Fail, not Errored.
    """
    sys.stdout.write(reason + "\n")
    return 1
