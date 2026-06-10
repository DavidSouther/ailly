---
name: skill-forge
description: Use when authoring, iterating, or refining a skill against an Ailly eval harness, driving a SKILL.md to a falsification gate (improved > 0 and regressed == 0) through assemble/run/eval/report. The harness-assembling loop pairs with ailly-skill-eval (which describes the harness anatomy) and delegates the authoring algorithm to skill-creator. Distinct from building the harness by hand or running one ad-hoc eval against a single prompt.
---

# skill-forge

`skill-forge` runs an authoring algorithm on an Ailly eval harness as one loop.
It **delegates** the authoring steps (capture intent, write or refine the
`SKILL.md`, reason about improvements) to an *authoring role*, and it **owns**
the substrate steps (build the harness, run the arms, read the gate, commit).
The loop is **refine-centric and eval-first**: the falsification eval is the
spec, built from intent, and the loop drives a skill body toward passing it.

This skill is the rare, idempotent harness-assembling *loop*. Its pair,
[ailly-skill-eval](../ailly-skill-eval/SKILL.md), is the frequent within-harness
*reference*: the project anatomy, the two axes, the assertion palette, and the
falsification gate. `skill-forge` defers to it for all of that and never restates
it. It delegates the authoring algorithm to an authoring role, preferring
Anthropic's `skill-creator` when present (see
[references/loop.md](references/loop.md), "The authoring role"); the substrate
loop is identical regardless of which authoring strategy resolves.

For the long-form walk-through of each phase, read
[references/loop.md](references/loop.md). For the generation contract (what the
scaffold emits and the disciplines it must hold), read
[references/scaffold.md](references/scaffold.md). The forged artifact is a
`SKILL.md` conforming to the open
[Agent Skills standard](https://agentskills.io/home); the `assembly`,
`conversation`, and `evaluation` schemas live in [DESIGN.md](../../DESIGN.md).
This skill restates none of them.

## Entry-state resume

On entry, determine the resume point from what already exists, mirroring
`developer:ailly`'s resume table. The eval is built from **intent** (the
situations the skill should route on, and the output properties a correct
response must exhibit), never derived from the skill body, so it can exist before
a meaningful body does.

| Skill body? | Eval/harness? | Resume at |
| --- | --- | --- |
| no  | no  | build the eval from intent, create an empty/skeleton `SKILL.md`, then refine |
| yes | no  | build the eval from intent, then refine |
| no  | yes | create an empty/skeleton `SKILL.md`, then refine |
| yes | yes | refine directly |

Every row converges on **refine against the gate**. The rows differ only in how
much scaffolding precedes the first refinement. Re-entry is idempotent: a present
eval or skill is used as-is, never clobbered, so a second invocation on a green
skill is a no-op.

## The loop

The phases split across the delegate/own boundary. Steps marked *(delegate)* go
to the authoring role; steps marked *(own)* are the substrate `skill-forge`
drives. [references/loop.md](references/loop.md) walks each with its rationale.

0. **Determine the entry state** and resume at the first unsatisfied step (the
   table above).
1. **Capture intent** *(delegate)*. The role conducts the interview and research.
   The intent feeds both the eval and any skill draft.
2. **Author, stub, or refine `SKILL.md`** *(delegate)*: a `description:` that
   routes, a body that shapes output, a "Common Mistakes" section. An
   empty/skeleton body is a valid starting point. *Draft gate:* the author
   confirms each revision before it is evaluated.
3. **Reference the skill in place** *(own)*. The invocation assembly names the
   authored `skills/<name>/SKILL.md` via a `kind: external` prefix block. Nothing
   is vendored; an edit is picked up at the next `assemble`.
4. **Build the harness from intent** *(own)*: generate the discovery, invocation,
   and baseline assemblies, the prompts, the suites, and `check_<skill>.py`, per
   [references/scaffold.md](references/scaffold.md). *Review gate:* the author
   tightens every generated artifact before evaluating.
5. **Evaluate** *(own)*: `assemble`, `run`, `eval`, `report` for the three arms.
   The committed `runs/<ts-id>/` conversations and `evals/reports/` are the
   record.
6. **Read the gate** *(own)*: `report <baseline-id> <invocation-id>`, then check
   `improved > 0 && regressed == 0` and discovery routing at or above 0.9.
7. **Refine** *(delegate)*. If the skilled arm still fails an assertion it should
   satisfy (or `regressed > 0`, or a checker errored), hand the committed eval
   report back to the role for a revised `SKILL.md`, and re-evaluate. Loop to
   green.
8. **Done.** Commit. Every iteration's conversation, trace, and comparison report
   is already committed; there is no separate ledger.

## The two gates

- **Draft gate (after step 2):** the author confirms a skill revision before it
  is evaluated.
- **Review gate (after step 4):** the author tightens every generated artifact
  before the first evaluation.

These mirror `developer:ailly`'s draft discipline: generation proposes, the human
confirms, then the substrate runs.

## Two disciplines the loop must hold

- **Never encode the answer.** A generated discovery prompt describes the
  situation and never names the skill. The baseline-shared prefix and every
  baseline prompt contain no skill routing identifier (the bare `name`, or its
  `<plugin>:<name>` form). A leaky scaffold makes the baseline arm pass too, so
  `improved == 0`. When the gate reads `improved == 0`, the response is "re-frame
  the prompt at the situation," **never** "weaken the checker." The falsification
  gate catches a leak by failing loudly; it is the safety net that makes full
  scaffolding safe.
- **A null result is not a red.** Both arms passing (`unchanged_pass`) is a true
  null, a legitimate statement that a capable model already produces the pattern,
  and the loop does not iterate on it. The loop iterates only while the *skilled*
  arm fails, `regressed > 0`, or a checker errors. It never clears `improved == 0`
  by weakening the checker. The `check_<skill>.py` rules and the SKILL.md "Common
  Mistakes" bullets are kept 1:1 as a maintained invariant (both project the same
  intent), so refining a failure mode updates both, never the checker alone.

## Pointers

- [ailly-skill-eval](../ailly-skill-eval/SKILL.md) — the paired harness
  reference: project anatomy, the two axes, the assertion palette, the
  falsification gate. `skill-forge` defers to it for all harness anatomy.
- [references/loop.md](references/loop.md) — the long-form per-phase walk-through,
  the entry-state resume table, and the authoring-role contract and resolution
  order.
- [references/scaffold.md](references/scaffold.md) — the generation contract: what
  the scaffold emits, the in-place `kind: external` reference, the prompt-framing
  discipline, and the checker ↔ Common-Mistakes invariant.
- [e2e/clean-comments-review/](../../e2e/clean-comments-review/) — a worked forge,
  whose skill body lives at
  [skills/clean-comments-review/SKILL.md](../clean-comments-review/SKILL.md) and
  is referenced in place via `kind: external`.
- `skill-creator` — the authoring algorithm `skill-forge` delegates to (capture
  intent, the Skill Writing Guide, improvement reasoning) when installed.
- [Agent Skills standard](https://agentskills.io/home) — the open spec the forged
  `SKILL.md` conforms to.
- [DESIGN.md](../../DESIGN.md) — the authoritative `assembly`, `conversation`, and
  `evaluation` schemas.
