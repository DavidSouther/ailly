---
description: Review an artifact via general:review's composed reviewers, dispatched through review_run
argument-hint: "<artifact path> [specialist plugin:skill, ...]"
---
Read `general/skills/review/SKILL.md`. Compose the reviewer set: `general:c3-review` is always included; add a specialist (as `<plugin>:<skill>`, e.g. `developer:clean-comments-review`, `developer:small-prs-review`, or `domain:using-domain`) when its description matches this artifact. Include `developer:small-prs-review` for every complete pre-PR diff. Then call the `review_run` tool with the artifact path and any chosen specialists — it runs dispatch and the mandatory convergence step for you. After it returns, fix the ranked findings in a separate pass and re-evaluate; do not fix inside this step.

Artifact and any specialists: $ARGUMENTS
