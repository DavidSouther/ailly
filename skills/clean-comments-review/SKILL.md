---
name: clean-comments-review
description: Use when reviewing the comments and DocBlocks in code for their audience and longevity, not the code's correctness. Applies when a public DocBlock enumerates current callers or describes how a symbol is used today (detail that rots when usage changes) instead of why the symbol exists, when a comment's audience is unclear (an external-reader DocBlock versus an internal line comment), or when over-documentation should be cut back to intent. Produces a critique document, not edits to the code.
---

# Clean Comments Review

## Overview

Review the comments in a piece of code and report on them. The artifact is a
critique document, not a rewrite of the code. Judge every comment by its
audience and by whether it will still be true after the surrounding code
changes. A comment earns its place when it captures something the code cannot
say for itself. A comment is a liability when it restates the code, or when it
records facts that rot as the code evolves.

Two audiences, two standards:

- A **public DocBlock** addresses an **external reader** who will not read the
  implementation. It states why the code exists and what a caller may expect.
  It must not enumerate the current call sites or describe how the symbol is
  used today: that detail rots the moment usage changes, and the external
  reader cannot act on it. Module-level intent and worked examples help that
  reader; links into the implementation do not.
- An **internal line comment** addresses a future maintainer who can read the
  code and find usages, but cannot recover why a surprising or atypical
  decision was made. Most code needs none. A comment that restates what the
  next line plainly does is noise.

## When to Use

- A public DocBlock lists every current caller, or describes how the symbol is
  used today, and that detail will drift as soon as usage changes.
- A comment documents how a symbol is used rather than why it exists.
- A comment's audience is unclear: it sits on a public symbol but reads like an
  internal note, or sits inline but restates a public contract.
- A reviewer asks whether over-documentation should be reduced to intent.

**When NOT to use:** judging whether the code is correct, fast, or
well-structured. This is a comment review, not a code review. It changes no
code.

## The Audience Model

A comment should capture what the code cannot (Ousterhout, *A Philosophy of
Software Design*). A comment is, in part, a failure to express intent in the
code itself (Martin, *Clean Code*). Knuth-style literate programming, where
prose and code interleave as equals, is the explicit far end this review does
not pursue. The goal is the minimum comment that carries intent the code cannot.

For a **public DocBlock**, ask: could an external reader who never opens the
implementation act on this? State the why. Cut anything that enumerates current
call sites or current usage. That detail rots, and it serves the wrong audience.

For an **internal line comment**, ask: does this recover a why that the code
cannot show? If it restates the code, delete it. If it explains a surprising
decision, keep it.

## Output Format

Produce a critique document. For each comment:

1. Classify its audience: a public DocBlock (external reader) or an internal
   line comment (future maintainer).
2. State whether it serves that audience, and why or why not.
3. Recommend an action: keep, cut to intent, or remove. When a DocBlock
   enumerates current usage, recommend cutting that detail and reducing the
   comment to the why.

Do not edit the code. The artifact is the review.

## Common Mistakes

- **Endorsing rot-prone usage enumeration** Praising a DocBlock that lists
  current callers, or describes how a symbol is used today, as "thorough" or
  "complete." That detail rots when the call sites change, and the external
  reader cannot act on it. Recommend cutting it to intent.
- **Ignoring the comment's audience** Reviewing every comment against one
  standard instead of classifying each as a public DocBlock (external reader)
  or an internal line comment (future maintainer). The audience sets the
  standard.
- **Recommending no change when reduction is warranted** Concluding "the
  comments look thorough, no changes needed" when a comment should be cut back
  to its intent. A review that never recommends reduction is not reviewing for
  longevity.
