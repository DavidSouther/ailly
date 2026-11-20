---
name: review
description: Use when a development phase finishes, when preparing work for handoff, or before claiming a task is complete.
---

Compose independent reviewers for the artifact under review. Use **full review** by default. Use **quick review** when the caller requests a phase review.

## Shared Process

1. **Compose.** Include `general:c3-review` in every run. For Ailly, include Intent review using `developer/skills/ailly/references/abilities/intent-review.md` and its original-request anchor. Add a specialist when its description matches the artifact, such as `developer:clean-comments-review` for changed code comments. Give each reviewer the artifact and the evidence it needs to apply its own lens.
2. **Dispatch.** Run each reviewer in an isolated subagent, in parallel (see `general:dispatching-agents`). Apply the review instructions in separate passes when the environment cannot spawn agents. Ask specialists for findings using their own review instructions. Below six reviewers, use static parallel calls when available. At six or more, use a dynamic workflow if the harness supports one.
3. **Converge.** In a separate pass, verify each finding against the artifact, deduplicate, and severity-rank the findings that hold. Include reviewer failures in the result. Name each finding by its location and a short description of the defect, not by an invented code such as `M1` or `F3`, in every report the user or another agent reads.

## Quick Review

Scope the review to the completed phase's output. Run one collection and convergence pass, focusing each reviewer on findings that matter to the next phase or handoff. Return the verified findings to the coordinator, who resolves blockers or carries forward open findings as appropriate.

## Full Review

Scope the review to the entire current deliverable. After convergence, apply `general:c3-review`'s High-severity challenge protocol using a cold challenger and a fresh verifier when there is a qualifying finding. Apply verified fixes in a separate pass. Re-evaluate the resulting artifacts and run the required checks on the final reviewed state. Report the review as complete once those checks pass and all blockers are resolved.

When the artifact is a code change headed for publication, recommend `developer:code-review` so the user can review the diff before it is public. Run it only when the user accepts. In Ailly, the coordinator makes this recommendation at Build Handoff.

When the environment has no tools or file system (a self-contained review prompt), perform this inline. Compose from the reviewer instructions supplied in the prompt or session context, always include `general:c3-review`, evaluate each lens in a separate pass, then verify, deduplicate, and severity-rank the combined findings. State which reviewer sources were unavailable, and stop short of editing.

## Finding Specialist Reviewers

Discover specialists from the environment rather than assuming a particular harness or directory layout:

1. Identify the artifact's domains, media, technologies, and risk areas. Use those concerns as search terms alongside `review`, `critique`, and `audit`.
2. Inventory every reviewer source the environment exposes: project-local skill and agent descriptions; harness-provided skills, agents, reviewer roles, and review-capable tools; installed Ailly or other plugin skills; and reviewer definitions supplied in the prompt or session context. Prefer the harness's native catalog or discovery command as a starting point, then check the other exposed sources. Inspect conventional files only when a filesystem is available.
3. Read each shortlisted reviewer's entrypoint before selecting it. A name is not enough. When metadata is missing or vague, inspect the entrypoint if its location or declared role suggests review; otherwise record it as unresolved rather than assuming it applies. Select candidates whose instructions provide a distinct, read-only critique lens applicable to the artifact. A routing skill may lead to a specialist, and an agent description may qualify directly when review is its stated role.
4. Deduplicate aliases, wrappers, and reviewers with materially identical lenses. Record why each remaining candidate was selected or rejected, and note any source that was absent or inaccessible. Without persistent storage, include this record in the review response or session context.
5. Stop discovery only after every source class visible in the current environment has been checked. Then invoke the selected reviewers using the environment's available mechanism; discovery does not require subagents, a filesystem, Pi, or any particular tool name.

When no specialist can be discovered or applied, run `general:c3-review` and state that specialist coverage was unavailable. If a specialist's instructions are readable but no invocation mechanism exists, apply them as a distinct inline pass. Do not present a C3-only review as specialist coverage. When a source is inaccessible, report the resulting coverage gap rather than guessing what it contains.

## Recording: the `reviews/` Folder

A reviewer that poses falsifiable questions rather than deciding or editing directly — Ailly's `developer/skills/ailly/references/abilities/intent-review.md` is one such reviewer — notates its findings instead of writing them in place into the artifact under review. Feedback resolves into an edit or a closed note; it does not become a standing unresolved section living forever inside the primary artifact.

Reuse long-loop's dispatch shape and dated-block entry format (`developer/skills/ailly/references/shapes/long-loop.md`), adapted for posing rather than deciding. In a session-folder harness such as Ailly's, this lands in a `reviews/` folder sibling to the session's `research/` folder (`.ailly/developer/<session>/reviews/`); notate each finding there as a dated entry. Once the human answers a finding — by revising the artifact, replying, or dismissing it — mark that entry **resolved** and **closed** in place. It does not remain an open item inside the artifact under review.

## Common Mistakes

- Skipping convergence: handing the fix pass a flat, unverified, unranked list. Verify against the artifact, deduplicate, and severity-rank first.
- Prompting a specialist to write a rubric instead of consuming its critique. The specialist's critique document is already its findings.
- Treating the review as a single rubric instead of a composed set, so orthogonal concerns get a thinner pass than a dedicated reviewer would give them.
- Dropping `general:c3-review` from the set, so the artifact receives no C3 review.
- Running a dynamic workflow below the six-reviewer threshold where static `Agent` calls would do.
- Combining evaluation and editing into one pass. These happen in separate agents, to reduce context bloat.
- Claiming the task is complete before the review cycle finishes.
- Labeling findings with invented codes (`M1`, `U3`, `N2`) that the reader must look up. Refer to each finding by where it is and what is wrong.
- Treating a punctuation mark as the defect. Swapping a flagged em-dash for a colon or semicolon papers over the same weak structure. Restructure the sentence instead.
