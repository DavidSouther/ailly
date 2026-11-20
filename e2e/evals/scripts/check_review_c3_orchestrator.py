#!/usr/bin/env python3
"""Static checks that the review skills, the Ailly coordinator, and Pi review_run agree on the phase review schedule, reviewer composition, and C3's independent challenge."""

import sys
from pathlib import Path
from typing import Optional
import re

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _checker_utils import AILLY_DIR, C3_REVIEW, GENERAL_REVIEW, INTENT_REVIEW, REPO, fail, read  # noqa: E402


def require(path: Path, phrase: str, reason: str) -> Optional[int]:
    if not path.is_file():
        return fail(f"C3 feature: missing {path.relative_to(REPO)}")
    if phrase.lower() not in read(path).lower():
        return fail(f"C3 feature: {reason}")
    return None


def require_pattern(path: Path, pattern: str, reason: str) -> Optional[int]:
    if not path.is_file():
        return fail(f"C3 feature: missing {path.relative_to(REPO)}")
    if not re.search(pattern, read(path), flags=re.IGNORECASE | re.DOTALL):
        return fail(f"C3 feature: {reason}")
    return None


def main() -> int:
    c3 = read(C3_REVIEW) if C3_REVIEW.is_file() else ""
    if not c3:
        return fail("C3 feature: missing dedicated general:c3-review skill")
    if not re.search(
        r"Correctness.*?Conciseness.*?Clarity", c3, flags=re.IGNORECASE | re.DOTALL
    ):
        return fail("C3 feature: general:c3-review must define Correctness, Conciseness, and Clarity")
    result = require_pattern(
        C3_REVIEW,
        r"Correctness.*?Conciseness.*?Clarity",
        "general:c3-review must define Correctness, Conciseness, and Clarity",
    )
    if result is not None:
        return result

    pi_review = REPO / ".pi" / "extensions" / "lib" / "review.ts"
    result = require_pattern(
        pi_review,
        r'general/skills/c3-review/SKILL\.md.*?id:\s*"c3-review"',
        "Pi review_run must invoke the dedicated general:c3-review lane",
    )
    if result is not None:
        return result
    result = require_pattern(
        GENERAL_REVIEW,
        r"general:c3-review.*?every run.*?specialist.*?description.*?matches.*?artifact",
        "general:review must dispatch general:c3-review and retain applicable-specialist composition",
    )
    if result is not None:
        return result
    for heading in ("Quick Review", "Full Review"):
        result = require(GENERAL_REVIEW, f"## {heading}", f"general:review must define {heading}")
        if result is not None:
            return result

    coordinator = read(AILLY_DIR / "SKILL.md")
    schedule = coordinator.split("## Phase Reviews", 1)[-1].split("\n## ", 1)[0]
    if "general:review" not in schedule or "Intent review" not in schedule:
        return fail("C3 feature: the coordinator must compose general:review with Intent review")
    for phase in ("Research", "Design", "Plan", "Build", "Cleanup"):
        if not re.search(rf"^\| {phase} \|.*?quick", schedule, re.IGNORECASE | re.MULTILINE):
            return fail(f"C3 feature: {phase} must end with a quick general:review")
    if not re.search(r"^\| Build \|.*?quick.*?full", schedule, re.IGNORECASE | re.MULTILINE):
        return fail("C3 feature: Build must run full general:review after its quick pass")
    quick_loop = coordinator.split("## Quick-loop Mode", 1)[-1].split("\n## ", 1)[0]
    if not re.search(r"full review.*?before.*?paus", quick_loop, re.IGNORECASE | re.DOTALL):
        return fail("C3 feature: the quick-loop pause must wait for full Build review")
    if not re.search(r'"no review".*?human.*?automated reviews.*?run', quick_loop, re.IGNORECASE | re.DOTALL):
        return fail("C3 feature: quick-loop no review must skip only the human pause")

    result = require_pattern(
        C3_REVIEW,
        r"challenger reads only.*?evidence.*?(falsification|refut)",
        "the challenger must be cold, evidence-located, and falsifiable",
    )
    if result is not None:
        return result

    for phrase, reason in [
        ("every phase or stage", "Intent review must run continuously at every Ailly phase or stage"),
        ("original request", "Intent review must remain anchored to the original request"),
        ("never clears", "Intent review must not clear a human gate"),
    ]:
        result = require(INTENT_REVIEW, phrase, reason)
        if result is not None:
            return result

    result = require_pattern(
        AILLY_DIR / "SKILL.md",
        r"quick `general:review` after every completed phase.*?Intent review",
        "Ailly must run quick general:review with Intent review after every completed phase",
    )
    if result is not None:
        return result

    for pattern, reason in [
        (r"findingId.*?counterclaim.*?evidence.*?(falsification|refut)", "the C3 section must describe a challenge packet with findingId, counterclaim, evidence, and a falsification condition"),
        (r"(accepted|rejected).*?unresolved.*?failed", "the C3 section must name all four challenge outcomes: accepted, rejected, unresolved, and failed"),
        (r"(both presentation orders|order.swap|order-swap)", "the C3 section must require the order-swap check for a consequential, disputed challenge"),
        (r"accepted.*?(remove|omit).*?rejected.*?(retain|keep).*?(unresolved|failed).*?(remove|omit)", "the C3 section must state how each challenge status controls whether a finding stays actionable"),
    ]:
        result = require_pattern(C3_REVIEW, pattern, reason)
        if result is not None:
            return result

    return 0


if __name__ == "__main__":
    sys.exit(main())
