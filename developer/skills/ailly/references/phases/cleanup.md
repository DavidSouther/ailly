# Cleanup Phase

> Phase reference loaded by the coordinator (`developer:ailly`) when entered as
> `/ailly cleanup ...`. The coordinator hands this to an isolated phase runner
> that reads only this one reference through the active harness's isolation path. There is no standalone `developer:cleanup`
> skill; the phase is entered by argument.

The coordinator completes Build Handoff in `developer/skills/ailly/SKILL.md` before dispatching this phase.

## Clean Up

Run automated formatters and lint fixers and address remaining lint findings.

Extract deferred decisions from design.md via the program-management using reference (`references/abilities/program-management/using.md`) when a tracker is configured (it writes them as labeled tasks under the right parent); otherwise write them to `TASKS.md`, including any necessary TASK-NOTES.

Return the closeout changes and deferred-task record to the coordinator. Keep the session folder available for the quick phase review.

## Finalize After Review

The coordinator runs the quick Cleanup review defined in Phase Reviews, using the reviewed Build state to identify the closeout changes. If the deliverable changed materially, finish a fresh full review and run the required checks on the updated change. For a change headed to a PR, ensure `developer:small-prs-review` has reviewed the complete branch diff. Include it now if the Build review ran before the publication path was known. Treat proposed splits as advice and get approval before splitting the branch.

If Cleanup materially changed the diff the user reviewed, offer `developer:code-review` on the updated change before publication. Carry any entries still awaiting the user into the handoff.

After approval, open a PR or prepare the squash merge according to the project's configuration. For a Project-shaped topic with a configured doc-system target, publish the accepted notes and mark them `completed:` per `references/abilities/program-management/using.md`. For a feature or bug, retain the task record and remove the session folder when closeout is complete and it contains only disposable bookkeeping. Preserve task deliverables.
