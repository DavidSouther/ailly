#!/usr/bin/env python3
"""Corruption scorer for the `code-sql` domain.

Ported in spirit from microsoft/DELEGATE52: a SQL fragment's load-bearing facts
are its column names, its aggregation, its tables, and its join condition. The
paper's failure mode here is a join predicate or aggregation that silently
rewrites while the query still parses.

Reads the candidate (the final assistant turn's document) from stdin and the
seed from `context/seeds/code-sql.md`, resolved relative to the project-root
working directory. `program` assertions take no arguments; the seed path is
hardcoded (see the README fidelity notes). Both candidate and seed are reduced
to their fenced SQL via `extract_code` so prose around the block cannot satisfy
or break a rule.

Each load-bearing token of the seed's query (`customer_id`, `SUM(amount)`,
`total_amount`, `orders`, `customers`, the join condition, the `GROUP BY` key)
must survive verbatim, whitespace-normalized, in the candidate's SQL. On the
first missing token it prints a single-line reason to stdout and exits 1; if all
survive it exits 0. stderr is never written to.
"""

import re
import sys
from pathlib import Path

from _checker_utils import extract_code, fail

SEED_PATH = Path("context/seeds/code-sql.md")

# The seed query's load-bearing tokens, in corruption-severity order. Each is a
# verbatim substring of the seed's fenced SQL; checking them here keeps the
# scorer's contract explicit rather than re-deriving a SQL parser at fixture
# scale.
LOAD_BEARING = (
    "orders.customer_id = customers.id",  # join condition
    "SUM(amount)",  # aggregation
    "total_amount",  # aggregation alias
    "customer_id",  # grouping / projection key
    "orders",  # source table
    "customers",  # joined table
    "GROUP BY",  # aggregation clause
)


def normalize(sql: str) -> str:
    """Collapse runs of whitespace so reflowing the query does not look like
    corruption."""
    return re.sub(r"\s+", " ", sql)


def main() -> int:
    candidate = normalize(extract_code(sys.stdin.read()))
    seed = normalize(extract_code(Path(SEED_PATH).read_text(encoding="utf-8")))

    for token in LOAD_BEARING:
        needle = normalize(token)
        # The seed must actually contain the token (guards the fact list against
        # drifting away from the seed); then the candidate must preserve it.
        if needle not in seed:
            continue
        if needle not in candidate:
            return fail(f"code-sql: load-bearing token dropped or altered: {token!r}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
