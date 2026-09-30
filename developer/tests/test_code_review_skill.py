#!/usr/bin/env python3
"""Feature test: `developer:code-review` is a standalone skill for human review.

The skill sits after `general:review` (LLM review) and before a change is made
public. It uses diffx to collect the user's inline comments, then acts on them.
This is a source-level contract matching the `developer/tests/*.py` convention:
`main()` returns 0 when the contract holds, or 1 with one reason line.
"""

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SKILL = REPO / "developer" / "skills" / "code-review" / "SKILL.md"
README = REPO / "README.md"
OFFER_FILES = (
    "developer/skills/ailly/references/phases/cleanup.md",
    "general/skills/review/SKILL.md",
)
FORBIDDEN = ("mandatory", "must run", "always run")


def fail(reason: str) -> int:
    print(reason)
    return 1


def heading_index(text: str, pattern: str) -> int:
    m = re.search(r"(?m)^### \d+\. .*(?:%s).*$" % pattern, text, re.I)
    return m.start() if m else -1


def readme_row(readme: str) -> str:
    section = readme.split("### Developer (`developer:*`)", 1)
    table = section[1].split("\n### ", 1)[0] if len(section) == 2 else ""
    m = re.search(r"(?m)^\| `developer:code-review` \|.*$", table)
    return m.group(0) if m else ""


def main() -> int:
    if not SKILL.exists():
        return fail("R1 skill must exist at developer/skills/code-review/SKILL.md")
    text = SKILL.read_text(encoding="utf-8")
    low = text.lower()

    front = re.match(r"---\n(.*?)\n---\n", text, re.S)
    if not front or not re.search(r"(?m)^name:\s*code-review\s*$", front.group(1)):
        return fail("R2 frontmatter must declare name: code-review")
    if not re.search(r"(?m)^description:\s*Use when", front.group(1)):
        return fail("R2 description must start with 'Use when'")

    if not re.search(r"after `general:review`[^.]{0,80}before the change is public", text):
        return fail("R3 must place itself after general:review and before the change is public")
    if "wong2/diffx" not in text:
        return fail("R4 must cite wong2/diffx")

    if "diffx --no-open --persist --" not in text:
        return fail("R5 launch command must contain 'diffx --no-open --persist --'")
    if "/api/comments" not in text or not re.search(r"-X POST \$URL/api/comments/<id>/replies", text):
        return fail("R5 must POST replies to /api/comments/<id>/replies")
    if not re.search(r"-X PUT \$URL/api/comments/<id>", text) or '"status": "resolved"' not in text:
        return fail("R5 must PUT status resolved to /api/comments/<id>")
    for i, line in enumerate(text.splitlines(), 1):
        if "main..HEAD" in line and "for example" not in line.lower():
            return fail(f"R6 line {i} hardcodes main..HEAD outside a 'for example' note")

    idx = {
        "collect": heading_index(text, "collect"),
        "apply": heading_index(text, "apply.*subagent"),
        "review": heading_index(text, "review.*(diff|result)"),
        "reply": heading_index(text, "reply.*resolve"),
        "return": heading_index(text, "return"),
    }
    for name, pos in idx.items():
        if pos < 0:
            return fail(f"R7 missing step heading for {name!r}")
    order = [idx[k] for k in ("collect", "apply", "review", "reply", "return")]
    if order != sorted(order):
        return fail("R7 steps must run collect < apply subagent < review < reply/resolve < return")
    review_body = text[idx["review"]:idx["reply"]]
    if "general:review" not in review_body:
        return fail("R7 the review step must run general:review")

    for kind in ("change request", "question", "discussion", "ambiguous"):
        if kind not in low:
            return fail(f"R8 must classify comments; missing {kind!r}")
    if not re.search(r"distinct from every one of them", text) or "project" not in low:
        return fail("R9 must state it is distinct from the project's reviewers")
    if ".ailly/developer/<session>/reviews/code-review.md" not in text:
        return fail("R10 must record in .ailly/developer/<session>/reviews/code-review.md")
    if not re.search(r"(?i)human.{0,40}(reviewer|review)", text) or "shared understanding" not in low:
        return fail("R11 must state that human review builds shared understanding, not a second LLM pass")

    if not re.search(r"(?i)never started automatically|optional", text) or "accepts" not in low:
        return fail("R12 must be optional: offered, run only when the user accepts")
    readme = README.read_text(encoding="utf-8")
    row = readme_row(readme)
    if not row:
        return fail("R13 README developer skills table must have a developer:code-review row")
    offers = []
    for rel in OFFER_FILES:
        body = (REPO / rel).read_text(encoding="utf-8")
        lines = [ln for ln in body.splitlines() if "developer:code-review" in ln]
        offers.append((rel, "\n".join(lines)))
    offers.append(("README row", row))
    for name, body in offers:
        blow = body.lower()
        needs_accept = name != "README row"
        if "offer" not in blow or (needs_accept and "accept" not in blow):
            return fail(f"R14 {name} must say the review is offered" + (" and run on accept" if needs_accept else ""))
        for bad in FORBIDDEN:
            if bad in blow:
                return fail(f"R14 {name} must not make the review mandatory ({bad!r})")
    for rel, body in offers[:2]:
        if "`developer:code-review`" not in body:
            return fail(f"R14 {rel} must name developer:code-review")
    return 0


if __name__ == "__main__":
    sys.exit(main())
