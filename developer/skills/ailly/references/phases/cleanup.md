# Cleanup Phase

> Phase reference loaded by the coordinator (`developer:ailly`) when entered as
> `/ailly cleanup ...`. The coordinator hands this to an isolated phase runner
> that reads only this one reference through the active harness's isolation path. There is no standalone `developer:cleanup`
> skill; the phase is entered by argument.

## Clean Up

Perform final refactoring and review passes for the topic. Check for lint findings, and include them in the refactor passes. Run automated formatters and lint fixers.

Extract deferred decisions from design.md via the program-management using reference (`references/abilities/program-management/using.md`) when a tracker is configured (it writes them as labeled tasks under the right parent); otherwise write them to `TASKS.md`, including any necessary TASK-NOTES.

For a **Project**-shape topic with a doc-system target configured, publish the accepted notes as long-lived docs via the program-management using reference (`references/abilities/program-management/using.md`) and mark them `completed: YYYY-MM-DD`. Keep the durable local project records, including supporting feature/design/decision records referenced by the project, even after its checkouts are retired. A feature or bug leaves only its task record behind once its session folder is eligible for removal.

Before opening a PR, run `general:review` on the complete branch diff, composing `developer:small-prs-review` alongside `general:c3-review`. Keep session intent available until review finishes. Proposed splits are advice; do not split the branch without approval.

After `general:review` settles and before publication, offer `developer:code-review`; run it only when the user accepts. Its record is written before any eligible session-folder removal; entries still awaiting the user are carried in the final response.

Retain the canonical absolute session parent and the selected checkouts through review, PR preparation, and merge. Prepare the PR or squash merge according to the project configuration; the coordinator pauses for human approval before opening the PR or performing the squash merge.

Apply retirement checks separately to each recorded checkout. Preserve reused worktrees. Retire a session-created checkout only after its relevant changes have merged and no active feature, PR, or project integration/Closing Bell work still needs it, checking the creator/consumer notes before removal. Verify it has no uncommitted or untracked work, leave the checkout, then run `git -C <primary-repository-root> worktree remove <actual-absolute-checkout-path>`. Update its associations after removal. Do not force removal. Preserve and report dirty work or a removal failure. Never remove checkouts by recursively deleting their session parent. Remove the remaining canonical session folder only when safe; retain it while a consumer needs it, if it contains a registered worktree or unrelated work, or when it holds durable project records.
