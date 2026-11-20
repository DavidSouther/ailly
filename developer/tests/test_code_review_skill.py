#!/usr/bin/env python3
"""Feature test: `developer:code-review` is a standalone skill for human review.

The skill collects the user's diffx comments after full `general:review`.
Ailly recommends the pass at Build Handoff and runs it when the user accepts.
This is a source-level contract matching the `developer/tests/*.py` convention:
`main()` returns 0 when the contract holds, or 1 with one reason line.
"""

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SKILL = REPO / "developer" / "skills" / "code-review" / "SKILL.md"
README = REPO / "README.md"
AILLY = REPO / "developer" / "skills" / "ailly"
GENERAL_REVIEW = REPO / "general" / "skills" / "review" / "SKILL.md"
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
    if "untracked task files" not in review_body or "temporary Git index" not in review_body:
        return fail("R7 follow-up review must refresh the complete diff scope")

    for kind in ("change request", "question", "discussion", "ambiguous"):
        if kind not in low:
            return fail(f"R8 must classify comments; missing {kind!r}")
    if "the user is the reviewer" not in low:
        return fail("R9 must assign the review to the user")
    if ".ailly/developer/<session>/reviews/code-review.md" not in text:
        return fail("R10 must record in .ailly/developer/<session>/reviews/code-review.md")
    if not re.search(r"(?i)human.{0,40}(reviewer|review)", text) or "shared understanding" not in low:
        return fail("R11 must state that human review builds shared understanding, not a second LLM pass")

    if not re.search(r"(?i)recommended, not required", text) or "accepts" not in low:
        return fail("R12 must be recommended: run only when the user accepts")
    coordinator = (AILLY / "SKILL.md").read_text(encoding="utf-8")
    handoff = re.search(r"^## Build Handoff\n(.*?)(?=^## |\Z)", coordinator, re.M | re.S)
    if not handoff or not re.search(
        r"After full Build review.*?recommend `developer:code-review`.*?before.*?Cleanup.*?accepts.*?declines.*?Cleanup",
        handoff[1],
        re.I | re.S,
    ):
        return fail("R13 Build Handoff must recommend code-review after full review and proceed to Cleanup when declined")

    readme = README.read_text(encoding="utf-8")
    row = readme_row(readme)
    if not row:
        return fail("R14 README developer skills table must have a developer:code-review row")
    general_review = GENERAL_REVIEW.read_text(encoding="utf-8")
    offers = [
        ("Build Handoff", handoff[1]),
        ("general:review", "\n".join(ln for ln in general_review.splitlines() if "developer:code-review" in ln)),
        ("README row", row),
    ]
    for name, body in offers:
        blow = body.lower()
        if "recommend" not in blow or "accept" not in blow:
            return fail(f"R14 {name} must recommend the review and run it on accept")
        for bad in FORBIDDEN:
            if bad in blow:
                return fail(f"R14 {name} must not make the review mandatory ({bad!r})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
