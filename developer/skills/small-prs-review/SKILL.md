---
name: small-prs-review
description: Use when reviewing any complete diff immediately before opening a pull request.
---

# Small PRs Review

## Overview

Review a diff to identify self-contained changes that a reviewer can understand, approve, and revert as a unit. The artifact is a critique document that proposes seams. It does not split branches, move commits, or edit code.

The basis is Google's engineering practice on [small CLs](https://google.github.io/eng-practices/review/developer/small-cls.html): small changes are reviewed faster and more thoroughly, introduce fewer bugs, merge with less conflict, and roll back cleanly.

Line count is a proxy. The governing test is reviewability: one change addresses a single concern, and a reviewer can state that concern in a direct sentence.

## When It Runs

Run against every complete diff between a branch and its base immediately before a PR is opened. `general:review` composes this reviewer whenever the artifact is such a diff. It is not conditional on a title, line count, or a suspected split. The reviewer reads the supplied diff, commit list, and stated PR intent (session `design.md` or the draft PR description when present).

This is an artifact-only review. It can cite changed hunks, file boundaries, imports and callers visible in the diff, included tests, commit ordering, and the stated intent. It cannot establish that a proposed split compiles, passes tests, preserves behavior, or safely merges alone: the split does not exist yet, and the reviewer cannot see the unmodified repository, dependency resolution, runtime behavior, CI configuration, or merge result. Every proposed split is therefore a **feasibility hypothesis**, not a verified PR.

## What Makes a Change Whole

- **One purpose.** The diff has one coherent statement of intent. Concrete hunks with distinct, independently useful purposes may establish a candidate seam.
- **Self-contained.** The evidence in the diff can support a feasibility hypothesis that the change could compile, pass its tests, and leave the system working when merged alone. A proposed split is not evidence that it already does.
- **Code with its proof.** Implementations must include unit tests in the same change. Feature tests may be their own change, preceding implementation in a stack, or part of the implementation change when together they form a self-contained unit of work. A new API stays with its first caller.
- **Reviewable in one sitting.** A reviewer can hold the change in mind. Breadth counts as well as line count: a hundred lines across ten subsystems can be harder than a thousand lines in one file.
- **Reverts cleanly.** If reverting the change after it deploys would likely cause issues, it should be split apart. An implementation with a flag guard should _never_ be in the same commit as the change that activates the flag. Implementations that read config values must have defaults, so that the config changes can be added independently after the implementation releases.

## Seam Taxonomy

Look for these boundaries, in the order a split would merge them:

1. **Mechanical churn.** Renames, moves, formatting, dependency bumps, and generated or vendored output. Large and low-judgment; split so reviewers can skim it apart from logic.
2. **Preparatory refactor.** Behavior-preserving restructuring that the feature needs. Merges first; its review question ("is behavior unchanged?") differs from the feature's.
3. **Enabling change.** A new type, interface, flag, or migration that is used by nothing new yet and appears, from the supplied diff, capable of being exercised by existing callers or tests. It remains a feasibility hypothesis until isolated validation.
4. **Vertical slices.** Independently valuable behaviors, each cut through every layer it needs, in place of one horizontal layer per PR.
5. **Unrelated fixes.** Drive-by bug fixes and cleanups that do not depend on the feature. Each merges and rolls back alone.
6. **Bookkeeping.** Docs, config, changelog, and release metadata, when they do not describe code in the same change.

Not seams: an API apart from its only caller or a split that the diff shows would strand a dependency. Do not call a hypothetical split broken or green unless it has been materialized and validated.

## Procedure

1. Establish intent: one sentence for what the diff is for, from the design, PR description, or commit messages. When the diff supports several sentences, record each.
2. Partition the changed hunks by purpose against the seam taxonomy. Note hunks that belong to two purposes.
3. For each candidate seam, assess feasibility from the supplied diff: identify visible dependencies, included proof, and whether the proposed cut appears to strand anything. Label the result a **feasibility hypothesis**. Do not assert that an unmaterialized split builds, passes, preserves behavior, or merges alone.
4. Order the seams by dependency. Label each as a **proposed independent feasibility hypothesis** or a **proposed stacked feasibility hypothesis** based on diff evidence, not as a verified mergeable PR.
5. Start from **ship as one — focused PR**. Change that verdict only when the diff itself shows a concrete, independently reviewable seam. A single coherent change stays whole even when it is large; do not split only to hit a size.

## Output Format

Produce a critique document with:

- **Intent.** The one-sentence purpose, or the list when several exist.
- **Verdict.** **Ship as one — focused PR**, ship as one with notes, or split, with the reason in reviewability terms. A focused verdict is a successful review outcome, not an absence of review.
- **Proposed PRs.** Only when the verdict is split, for each seam in merge order:
  - title and single-sentence purpose;
  - the files or hunks it carries;
  - proposed dependency on an earlier PR, or "proposed independent";
  - feasibility evidence from the supplied diff, plus uncertainty and evidence the artifact cannot provide;
  - **validation pending/unverified:** the later human/CI work required after materializing the split: inspect the isolated tree, run the relevant build and tests, review behavior, and verify it merges against the intended base alone;
  - what the reviewer should examine in it.
- **Kept together.** Hunks that look separable but are not, with the reason.
- **Cost of splitting.** Any rebase, stacking, or duplicated scaffolding the split introduces, so the human can weigh it.

The document names seams and the way to cut them, for example by commit, by path, or by interactive rebase. It does not perform the split.

## Evidence for a Split

- A refactor and a behavior change are independently understandable from separate hunks.
- Mechanical churn hides logic changes in the same file.
- An unrelated fix has its own justification and can be reverted alone.
- Separate parts require different expertise and have no shared implementation cause.
- Reverting one part would discard other work that remains sound on its own.

## Common Mistakes

- Splitting by layer (schema, then service, then UI) so that no PR delivers or proves behavior.
- Representing a proposed split as verified to build, pass, or merge alone before a human or CI materializes and validates it.
- Treating a line count as the verdict.
- Treating a title, an "and," or a conventional PR phrase as evidence that a change should split.
- Splitting a cohesive mechanical change, such as a rename applied across the codebase, into artificial pieces.
- Editing, rebasing, or pushing as part of the review.
