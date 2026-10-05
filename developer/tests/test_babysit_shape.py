#!/usr/bin/env python3
"""Feature test: Babysit mode is documented as a shape reference and routed from SKILL.md.

Babysit is a terminal alternative to Cleanup that shepherds an open GitHub PR
stack to landing while preserving its shape and form. Its deliverables are
`references/shapes/babysit.md`, the consolidated fix loop in
`references/shapes/babysit-fix.md`, and routing in SKILL.md.

Source-level contract matching `developer/tests/*.py` (standalone, no
third-party deps, `main()` returns 0 when the contract holds or 1 with one
reason line). Stays red until the references and routing exist.
"""

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
AILLY = REPO / "developer" / "skills" / "ailly"
SKILL = AILLY / "SKILL.md"
BABYSIT = AILLY / "references" / "shapes" / "babysit.md"
FIX = AILLY / "references" / "shapes" / "babysit-fix.md"


def fail(reason: str) -> int:
    print(reason)
    return 1


def section(text: str, heading: str) -> str:
    match = re.search(rf"(?im)^##\s+{re.escape(heading)}\s*$", text)
    if not match:
        return ""
    rest = text[match.end():]
    nxt = re.search(r"(?m)^##\s+", rest)
    return rest[: nxt.start()] if nxt else rest


def main() -> int:
    if not BABYSIT.exists():
        return fail("R1 shape reference must exist at references/shapes/babysit.md")
    ref = BABYSIT.read_text(encoding="utf-8").lower()

    if "shape and form" not in ref:
        return fail("R2 babysit.md must state the shape-and-form governing principle")
    if "closing bell" not in ref or "quick-loop" not in ref or "cleanup" not in ref:
        return fail("R3 babysit.md must limit entry to the quick-loop pause or Closing Bell, replacing Cleanup")
    if "escalate:" not in ref:
        return fail("R4 babysit.md must record escalations in long-loop's ESCALATE: format")
    if "prefix" not in ref:
        return fail("R5 babysit.md must land the longest good prefix on escalation")
    if "quarantine" not in ref:
        return fail("R6 babysit.md must classify comments through a quarantined reader")
    if "merge-async" not in ref or "one at a time" not in ref:
        return fail("R7 babysit.md must cover native single merge and one-at-a-time landing")
    if "force-with-lease" not in ref:
        return fail("R8 babysit.md must push with an explicit --force-with-lease")
    if "locally" not in ref:
        return fail("R9 babysit.md must reproduce CI failures locally before pushing")
    if "babysit-state.md" not in ref:
        return fail("R10 babysit.md must persist loop state in babysit-state.md")
    if "references/shapes/babysit-fix.md" not in ref:
        return fail("R11 babysit.md must route fixes to references/shapes/babysit-fix.md")
    sync = section(BABYSIT.read_text(encoding="utf-8"), "Sync to Production Head").lower()
    if "production head" not in sync or "before" not in sync or "baseline" not in sync:
        return fail("R11a babysit.md must sync the stack onto the production head before snapshotting the baseline")

    if not FIX.exists():
        return fail("R12 fix loop reference must exist at references/shapes/babysit-fix.md")
    fix = FIX.read_text(encoding="utf-8").lower()
    if "no draft gate" not in fix:
        return fail("R13 babysit-fix.md must state the fix loop has no draft gates")
    if "shape check" not in fix:
        return fail("R14 babysit-fix.md review step must be a shape check against the baseline")

    skill = SKILL.read_text(encoding="utf-8")
    if "references/shapes/babysit.md" not in section(skill, "Babysit Mode"):
        return fail("R15 SKILL.md must have a `## Babysit Mode` section pointing to references/shapes/babysit.md")
    if "references/shapes/babysit.md" not in section(skill, "Routing"):
        return fail("R16 SKILL.md Routing table must include a Babysit row")
    desc = re.search(r'(?m)^description:\s*"(.*)"\s*$', skill)
    if not desc or "babysit" not in desc.group(1).lower():
        return fail("R17 SKILL.md frontmatter description must name babysit")
    if len(desc.group(1).encode("utf-8")) > 1024:
        return fail("R17 SKILL.md frontmatter description must stay within 1024 bytes")

    print("PASS: Babysit shape reference and routing contract holds")
    return 0


if __name__ == "__main__":
    sys.exit(main())
