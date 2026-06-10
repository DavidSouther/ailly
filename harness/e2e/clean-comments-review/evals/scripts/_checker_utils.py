"""Shared utilities for the comment-review checker.

The checker reads the assistant critique from stdin, applies ordered rules, and
either exits 0 (all rules hold) or exits 1 with a single-line reason on stdout.
stderr is never written to, so the eval runner records Fail, not Errored.

Unlike the patterns-eval checkers, the material under review is a critique
document (prose), not code, so there is no `extract_code` / `strip_comments`
here: the checker keys on the critique text directly.
"""

import sys


def fail(reason: str) -> int:
    """Write a single-line reason to stdout and return exit code 1.

    Leaves stderr untouched so the runner records Fail, not Errored.
    """
    sys.stdout.write(reason + "\n")
    return 1
