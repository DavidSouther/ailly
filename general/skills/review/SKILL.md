---
name: review
description: Use when preparing a final work product for handoff, before claiming a task is complete.
---

No single review format works for every document type. This skill orchestrates a number of reviewers as appropriate for the current deliverable or artifact. It composes independent reviewers for the final artifact. The skill always includes `general:c3-review`, and specialists compose in when the artifact matches them. The skill dispatches reviewers in parallel, converges, deduplicates, and verifies their findings, dispatches an agent to apply fixes, and then re-evaluates the edited artifacts.

## Process

1. **Compose.** Read the final artifact and the available review skills. Dispatch one isolated subagent that invokes `general:c3-review`; it is always in the set. Add a specialist when its `description` matches the final artifact — for example `developer:clean-comments-review` when reviewing code with DocBlocks or inline comments, or `domain:using-domain` when a domain model, its objects, or its ubiquitous language changed. Selection is model-decided from the available skills, not a maintained table, so a newly installed specialist is composed in with no edit here.
2. **Dispatch.** Run each composed reviewer in parallel, each in its own subagent with isolated context so the lenses do not cross-contaminate (see `general:dispatching-agents`). When the environment cannot spawn agents, apply each reviewer's instructions in a clearly separated pass instead. C3's subagent collects feedback per `general:c3-review`; a specialist produces its own critique document. Those documents are their findings — do not prompt a specialist to write a rubric first. Below six reviewers, dispatch with static parallel `Agent` calls when available. At six or more, use a dynamic workflow that pipelines the assembled list when the harness supports one.
3. **Converge (mandatory).** One isolated subagent, or a separate inline pass when agents are unavailable, collects every reviewer's findings and performs three steps in order: **verify** each candidate against the actual artifact (read the code, trace the claim) and drop what does not hold; **deduplicate** findings that more than one reviewer raised; **severity-rank** the survivors. The fix pass receives a verified, deduplicated, ranked list, never a flat dump.
4. **Fix.** A separate subagent or inline pass addresses the ranked findings. Evaluation never emits edits; fixing is always a separate pass.
5. **Re-evaluate** and flag any remaining issues to the user. Repeated LLM editing risks attractor states, so handing the residue to the user mitigates that.

After initial convergence, dispatch a separate cold challenger and fresh final verifier under `general:c3-review`'s High-severity challenge protocol. C3 and any specialists run only to prepare a final artifact for handoff; Intent review is the continuous, every-phase-or-stage mechanism for an evolving Ailly artifact instead. When the artifact is a code change headed for publication, offer `developer:code-review` once this review settles, so the user can review the diff before it is public. The offer is optional: it runs only when the user accepts, and never unprompted.

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
- Dropping `general:c3-review` from the set, so the final artifact ships without the C3 floor.
- Running a dynamic workflow below the six-reviewer threshold where static `Agent` calls would do.
- Combining evaluation and editing into one pass. These happen in separate agents, to reduce context bloat.
- Claiming the task is complete before the review cycle finishes.
- Treating a punctuation mark as the defect. Swapping a flagged em-dash for a colon or semicolon papers over the same weak structure. Restructure the sentence instead.
