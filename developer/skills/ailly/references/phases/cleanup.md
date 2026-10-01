# Cleanup Phase

> Phase reference loaded by the coordinator (`developer:ailly`) when entered as
> `/ailly cleanup ...`. The coordinator hands this to an isolated phase runner
> that reads only this one reference through the active harness's isolation path. There is no standalone `developer:cleanup`
> skill; the phase is entered by argument.

## Clean Up

Perform final refactoring and review passes for the topic. Check for lint findings, and include them in the refactor passes. Run automated formatters and lint fixers.

Extract deferred decisions from design.md via the program-management using reference (`references/abilities/program-management/using.md`) when a tracker is configured (it writes them as labeled tasks under the right parent); otherwise write them to `TASKS.md`, including any necessary TASK-NOTES.

For a **Project**-shape topic with a doc-system target configured, publish the accepted notes as long-lived docs via the program-management using reference (`references/abilities/program-management/using.md`) and mark them `completed:` before removing the local folder. A feature or bug leaves only its task record behind.

After `general:review` settles and before publication, offer `developer:code-review`; run it only when the user accepts. Its record is written before the session folder is removed; entries still awaiting the user are carried in the final response.

Remove the `.ailly/developer/YYYY-MM-DD-A-<topic>` folder.

Either open a PR, or prepare a squash merge, depending on the project configurations.
