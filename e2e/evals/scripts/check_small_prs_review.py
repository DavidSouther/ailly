#!/usr/bin/env python3
"""Static contract test for the developer:small-prs-review specialist."""

import re
import sys
from pathlib import Path
from typing import Optional

sys.path.insert(0, str(Path(__file__).resolve().parent))
from _checker_utils import REPO, fail, read  # noqa: E402


SMALL_PRS_REVIEW = REPO / "developer" / "skills" / "small-prs-review" / "SKILL.md"
DISCLOSURE = REPO / "developer" / "e2e" / "context" / "skills" / "disclosure.md"
PROMPT = REPO / "prompts" / "review.md"
COORDINATOR = REPO / "developer" / "skills" / "ailly" / "SKILL.md"
CLEANUP = REPO / "developer" / "skills" / "ailly" / "references" / "phases" / "cleanup.md"
README = REPO / "README.md"


def require_text(path: Path, pattern: str, reason: str) -> Optional[int]:
    if not path.is_file():
        return fail(f"small-prs-review: missing {path.relative_to(REPO)}")
    if not re.search(pattern, read(path), flags=re.IGNORECASE | re.DOTALL):
        return fail(f"small-prs-review: {reason}")
    return None


def main() -> int:
    for path, pattern, reason in [
        (SMALL_PRS_REVIEW, r"\A---\s*\nname:\s*small-prs-review\s*\ndescription:\s*Use when\b.*?\n---", "SKILL.md frontmatter must provide name and a Use when description"),
        (SMALL_PRS_REVIEW, r"Run against every complete diff", "SKILL.md must apply the reviewer to every complete pre-PR diff"),
        (SMALL_PRS_REVIEW, r"artifact-only", "SKILL.md must make the review artifact-only"),
        (SMALL_PRS_REVIEW, r"feasibility hypothesis.*?validation pending", "SKILL.md must label proposed splits as feasibility hypotheses with validation pending"),
        (SMALL_PRS_REVIEW, r"ship as one — focused PR", "SKILL.md must make a focused one-PR verdict an explicit successful outcome"),
        (SMALL_PRS_REVIEW, r"not conditional on a title", "SKILL.md must reject PR titles as a split trigger"),
        (README, r"developer:small-prs-review", "README must disclose developer:small-prs-review"),
        (DISCLOSURE, r"developer:small-prs-review", "skill disclosure must list developer:small-prs-review"),
        (PROMPT, r"developer:small-prs-review", "review prompt must name developer:small-prs-review"),
        (COORDINATOR, r"full `general:review`.*?complete current change.*?developer:small-prs-review", "the full Build review must include the small PRs specialist when preparing a PR"),
        (CLEANUP, r"Keep the session folder.*?Finalize After Review.*?developer:small-prs-review.*?remove the session folder", "keep session intent available through the pre-PR review"),
    ]:
        result = require_text(path, pattern, reason)
        if result is not None:
            return result
    return 0


if __name__ == "__main__":
    sys.exit(main())
