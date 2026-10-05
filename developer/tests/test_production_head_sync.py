#!/usr/bin/env python3
"""Feature test: rebasing onto the upstream branch is a general Ailly practice.

One permanent ability reference at `references/abilities/rebase.md` owns the
procedure, upstream-branch model, push policy, per-commit checks, and cadence
heuristics. SKILL.md, initialize.md, and configuring.md prompt for the branch
model in DEVELOPMENT.md. Babysit's "Rebase onto Upstream" section references
the ability rather than restating it.

Source-level contract matching `developer/tests/*.py` (standalone, no
third-party deps, `main()` returns 0 when the contract holds or 1 with one
reason line). Checks behavior keywords, not exact wording. Stays red until the
ability and its references exist.
"""

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
AILLY = REPO / "developer" / "skills" / "ailly"
SKILL = AILLY / "SKILL.md"
REBASE = AILLY / "references" / "abilities" / "rebase.md"
BABYSIT = AILLY / "references" / "shapes" / "babysit.md"
INIT = AILLY / "references" / "abilities" / "initialize.md"
CONFIG = AILLY / "references" / "abilities" / "program-management" / "configuring.md"
REF = "references/abilities/rebase.md"


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


def has_all(text: str, *words: str) -> bool:
    return all(w in text for w in words)


def mentions_branch_model(text: str) -> bool:
    t = text.lower()
    return has_all(t, "development.md", "upstream", "trunk", "feature")


def main() -> int:
    if not REBASE.exists():
        return fail(f"R1 rebase ability must exist at {REF}")
    rebase = REBASE.read_text(encoding="utf-8").lower()
    if not has_all(rebase, "fetch", "rebase", "local checks"):
        return fail("R2 rebase ability must fetch, rebase, and run local checks")
    if "--exec" not in rebase and "each commit" not in rebase and "every commit" not in rebase:
        return fail("R3 rebase ability must run local checks at each replayed commit")
    if not mentions_branch_model(rebase):
        return fail("R4 rebase ability must take the upstream branch (trunk or feature) from DEVELOPMENT.md")
    if "--force-with-lease=<ref>:<sha>" not in rebase:
        return fail("R5 rebase ability must push with the explicit --force-with-lease=<ref>:<sha> form")
    if not has_all(rebase, "local-only", "push"):
        return fail("R6 rebase ability must keep local-only branches local until a push-implying command")
    if not has_all(rebase, "wip", "tip"):
        return fail("R7 rebase ability must allow WIP or fixup commits only at the tip")
    if "human merge gate" not in rebase or "heuristic" not in rebase:
        return fail("R8 rebase ability must give cadence heuristics and rebase before the human merge gate")
    if "escalat" not in rebase or "worktree" not in rebase:
        return fail("R9 rebase ability must escalate non-mechanical conflicts and branches in other worktrees")

    skill = SKILL.read_text(encoding="utf-8")
    if REF not in section(skill, "Routing"):
        return fail("R11 SKILL.md Routing table must include the rebase ability")
    if re.search(r"--force-with-lease(?!=)", skill):
        return fail("R12 SKILL.md must not use the bare --force-with-lease form")
    if not mentions_branch_model(section(skill, "Phase-Entry Checks")):
        return fail("R13 SKILL.md Phase-Entry Checks must write the branch model to DEVELOPMENT.md when absent")

    init = INIT.read_text(encoding="utf-8")
    if not mentions_branch_model(init):
        return fail("R14 initialize.md must write the branch model to DEVELOPMENT.md when absent")
    hooks = section(init, "Four Development Hooks").lower()
    if "commit" not in hooks or "local checks" not in hooks:
        return fail("R15 initialize.md hooks must run local checks at commit")
    if not mentions_branch_model(CONFIG.read_text(encoding="utf-8")):
        return fail("R16 configuring.md must write the branch model to DEVELOPMENT.md when absent")

    bsec = section(BABYSIT.read_text(encoding="utf-8"), "Rebase onto Upstream")
    if REF not in bsec:
        return fail("R17 babysit.md Rebase onto Upstream must reference the rebase ability")
    if "restack" in bsec.lower():
        return fail("R18 babysit.md must not restate the rebase procedure")

    print("PASS: rebasing onto the upstream branch is a general practice referenced by Babysit")
    return 0


if __name__ == "__main__":
    sys.exit(main())
