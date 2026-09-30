---
name: code-review
description: Use when a change has finished `general:review` and is about to become public (push, pull request, squash-merge), and the user wants to review the diff themselves.
---

# Code Review

## Overview

Code review is the human beat between LLM review and publication. `general:review` is internal and adversarial: composed reviewers critique, converge, and fix. That loop is one-way, and it pulls work toward the average. A human reviewing the diff does something different: the reviewer learns what the author intended, and the author learns what is and is not convincing. That exchange builds shared understanding, and no additional LLM pass produces it.

This skill is optional. Ailly offers it after `general:review` has settled and before the change is public, and runs it only when the user accepts. It is never started automatically, and declining it does not block cleanup or the merge gate. The user is the reviewer. `general:review` discovers reviewer skills and agents from the project; this skill is distinct from every one of them, because its reviewer is the human developer guiding the session. The skill has the same overall shape as `general:review`, with the human's comments as the findings: collect the comments with [wong2/diffx](https://github.com/wong2/diffx), hand them to a subagent that applies the edits, run `general:review` on the resulting diff, and return to the user.

Background: <https://davidsouther.com/blog/llm_review_is_not_human_review/>.

**Announce at start:** "Using developer:code-review to collect human comments on the diff."

## Preconditions

- The user asked for this review or accepted an offer of it.
- `general:review` has completed for the pending change, and its residue was reported to the user. If it has not run, tell the user that `general:review` has not run, and proceed only if the user still wants human review first.
- Work is visible to the chosen diff: committed when the diff is `<base>..HEAD`, or present in the working tree when the diff is a working-tree diff. Nothing is pushed yet.
- `diffx` is installed (`npm install -g diffx-cli`). If the command fails, follow `developer/skills/ailly/references/checks/tool-failure.md` rather than substituting another tool.

## Process

### 1. Launch diffx

Choose the diff the user should review. For a branch review, derive the base from the repository's default branch or the branch's upstream (for example `git merge-base HEAD origin/HEAD`) rather than assuming `main`, and review `<base>..HEAD`. Anything after `--` goes to `git diff`.

```bash
diffx --no-open --persist -- <base>..HEAD   # run in the background
```

Run it in the background so the server stays alive. Read the actual URL from the server's `diffx server running at ...` output and use it as `$URL` in the commands below. Tell the user the URL and stop the turn:

> diffx is running at `<URL>`. Leave inline comments on the diff. When finished, return here and say so.

Do not proceed until the user returns. Do not review the diff on the user's behalf in the meantime.

### 2. Collect comments

```bash
curl -s $URL/api/comments
```

Each comment has `id`, `filePath`, `scope` (`line`, `file`, or `diff`; optional, and absent means `line`), `side` (`additions` or `deletions`), `lineNumber`, `startLineNumber`, `lineContent`, `body`, `status`, `createdAt`, `editedAt`, and `replies`. A multi-line comment sets `startLineNumber` and `lineNumber` is its last line; `startLineNumber` is absent for a single line. A `file` or `diff` scope comment has `lineNumber` 0 and empty `lineContent`; a `diff` scope comment also has an empty `filePath`.

Take the comments with `"status": "open"`, except those whose last reply starts with `[agent]`: those were answered and await the user, so skip them unless the user has replied since. `CommentReply` has no author field (only `id`, `body`, `createdAt`), so the `[agent]` prefix is the marker that separates agent replies from the user's. With none to act on, report that nothing remains and stop.

### 3. Apply through a subagent

Dispatch one isolated subagent to apply the comments, following `general:dispatching-agents` and its model-selection reference, and announce the model chosen. Pass it every selected comment verbatim with its reply history, the repository path, and the classification below. Applying edits is a separate pass from evaluating them, as in `general:review`.

| Kind | Signal | Subagent action |
|---|---|---|
| Change request | Asks for a specific edit | Apply it, scoped to what the comment asks |
| Question | Asks why or whether | Draft an answer; do not edit |
| Discussion | Expresses a concern, preference, or tradeoff without a specific edit | Draft the reasoning and options; do not edit |
| Ambiguous | Intent unclear | Draft a clarifying question; do not edit |

For `file` scope the subagent reads the whole file, and for `diff` scope the whole diff. A change request that contradicts the design, the plan, or another comment is not applied silently; the subagent reports the conflict instead. Comments that interact (a rename touching several files) are handled together. The edits stay uncommitted in the working tree. The subagent returns, per comment, its kind, the edit made or reply drafted, and any check it ran; it does not give the final test verdict, which belongs to the coordinator in step 5.

Without subagent support, perform this step as a clearly separated inline pass.

### 4. Review the resulting diff

Run `general:review` on `git diff <base>`, which covers commits and the working tree, so the step 3 edits are included. Line numbers from the launch-time diff shift once edits land, so anchor later work to content, not to the original line numbers. Its findings and fixes follow that skill's own process. Report any residue to the user rather than looping.

### 5. Run the tests

The coordinator runs the project's tests after the review's fixes. This result is the test verdict for step 6.

### 6. Reply and resolve

Post a reply for each comment the subagent handled, prefixing every body with `[agent]`:

```bash
curl -s -X POST $URL/api/comments/<id>/replies \
  -H "Content-Type: application/json" -d '{"body": "[agent] Renamed x to parsedToken in parser.ts and its two call sites."}'

curl -s -X PUT $URL/api/comments/<id> \
  -H "Content-Type: application/json" -d '{"status": "resolved"}'
```

Resolve only comments whose edit was applied and whose tests pass. Questions, discussion, ambiguous comments, and conflicts stay open for the user.

### 7. Record and return to the user

Write each comment's outcome as a dated entry in the session's `reviews/` folder (`.ailly/developer/<session>/reviews/code-review.md`), using the dated-block format in `developer/skills/ailly/references/shapes/long-loop.md`, as `general:review`'s Recording section directs. The fields are adapted to comment, kind, action taken, and status. Entries stay open while comments await the user, and close once the user accepts the reply or the edit. List the pending entries in the final response. Without a session folder, include the whole record in the final response.

Tell the user which comments were applied, answered, or left open, and what the follow-up `general:review` found. Nothing is committed until the user chooses the next step: another review round, or opening the pull request. A later round uses `<base>..HEAD` after the edits are committed, or a working-tree diff (`git diff <base>`) before that. Reuse the running diffx server: persisted review identity is tied to the resolved revisions, so it does not restore after HEAD moves on a commit, and reuse keeps the comments and replies. Control returns to the coordinator for the merge gate in `developer:ailly` cleanup once the user is finished.

## Common Mistakes

- Running before `general:review` has settled, so the user spends attention on defects an LLM pass would have caught.
- Reviewing the diff for the user while diffx is open.
- Applying edits in the coordinator instead of a subagent, or skipping the `general:review` pass on the edited diff, or letting the subagent give the final test verdict.
- Treating every comment as an instruction. Questions and discussion get replies, not edits.
- Resolving a comment that was only answered, which hides an open concern from the user.
- Guessing at ambiguous comments instead of asking.
- Pushing or opening the pull request before the user chooses the next step.
