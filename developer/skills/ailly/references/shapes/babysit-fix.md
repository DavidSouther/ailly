# Babysit Fix Loop

> Loaded by `references/shapes/babysit.md` for each CI failure or in-bounds comment. One pass runs in the stack's worktree on the owning PR's branch.

## Passes

There are no draft gates: the watch loop cannot block on a human, so each pass writes its note and the next begins. Notes go under the session folder beside `babysit-state.md`.

1. **Research.** Read the failure or comment, the owning PR's diff, and the baseline. Record the cause in one paragraph.
2. **Design.** State the change in observed / expected / unchanged terms, scoped to the owning PR.
3. **Plan.** List the files to touch and the local check that proves the fix.
4. **Implement.** Make the change and run the local check until green.
5. **Review.** Run the shape check below.

## Shape Check

Compare the result against the baseline in `babysit-state.md`: the ordered PR set, each PR's stated purpose, and the session's design artifacts. The check fails when the change adds, removes, splits, or reorders a PR, alters a PR's stated purpose, departs from the design artifacts, or touches a protected path. A diff that grows beyond what the cause plainly needs is a tripwire that forces a careful shape check; it is not a numeric definition of "small". A failed shape check discards the change and escalates per `references/shapes/babysit.md`.
