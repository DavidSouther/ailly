# Workflow continuation-based checkpointing review

Verify that the workflow runtime's pause-and-resume model is shaped as continuation-based checkpointing rather than ad-hoc state-write-and-replay. Today the runtime persists `WorkflowState` (queue plus history) at the end of each task and re-derives the resumable turn on the next run via `find_resumable_turn` walking the conversation root for a turn file with a recorded response. This is closer to log-replay than to a continuation snapshot.

Reference reading before the review: <https://dev.to/yacineb_45/what-i-learned-building-a-workflow-engine-from-scratch-in-rust-2mdk>.

Review questions:

- Is the per-task pause point a true continuation (the runtime can resume at the exact suspension point with all in-process variables restored from disk), or is it a coarser checkpoint plus replay walk?
- If a task suspends mid-tool-call (the future `AwaitingInput` path deferred from the workflow CLI wiring slice), can the runtime resume without re-running the model turn that produced the tool call?
- Would adopting an explicit continuation type collapse `Paused`, the future `AwaitingInput`, and the `find_resumable_turn` walk into a single mechanism?
- What does the linked article propose that the current code does not, and is the gap worth closing now or worth tracking as a separate slice?

If the review concludes the model is sound, close the task. If it concludes a refactor is warranted, open a follow-on slice with its own design pass; do not refactor inline.
