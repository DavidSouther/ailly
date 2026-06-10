# The skill-forge loop

The long-form walk-through of each phase, with rationale. It mirrors
[ailly-skill-eval/references/method.md](../../ailly-skill-eval/references/method.md)
in structure: one section per part. The loop is an authoring algorithm run on an
Ailly eval harness, with the substrate-bound steps overridden. Read
[SKILL.md](../SKILL.md) first for the phase contract and the two disciplines;
read [scaffold.md](scaffold.md) for the generation contract.

## The delegate/own boundary

`skill-forge` does not re-create an authoring algorithm. It **delegates** the
authoring steps and **owns** the substrate steps. The boundary is exactly the set
of steps that touch where results live, how the arms are run, and how they are
compared.

- **Delegated (authoring role):** capture intent, write or refine the `SKILL.md`,
  reason about improvements. These steps carry real authoring content (an
  interview, a Skill Writing Guide, improvement reasoning). Re-deriving them here
  would drop that content and drift from its source, so they stay delegated.
- **Owned (substrate):** build the harness, reference the skill in place, run
  `assemble`/`run`/`eval`/`report`, read the four-bucket gate, commit. These are
  the steps bound to Ailly's persistence, and they are identical regardless of
  which authoring strategy resolved.

## The authoring role

`skill-forge` depends on an **authoring role**, not on a concrete authoring tool.
This is dependency inversion with a Null-Object default, so the loop degrades but
never breaks, and so the loop that showcases Ailly's neutrality does not itself
hard-bind to one provider's plugin.

- **Contract.** Intent in, `SKILL.md` out. Eval report in, revised `SKILL.md`
  out. Nothing else crosses the boundary.
- **Resolution order.** (1) Anthropic's `skill-creator` if present, the default;
  (2) any other installed authoring skill; (3) **inline drafting against the
  Agent Skills spec**, always available, the Null-Object fallback. The open,
  small standard makes the fallback fully viable.
- **The substrate never depends on which resolved.** The assemble/run/eval/report
  loop, the in-place `kind: external` reference, and the gate are the same in all
  three cases.

## Phase 0: determine the entry state

Inspect what already exists and resume at the first unsatisfied step. The eval is
the spec, built from intent, so it can precede a skill body.

| Skill body? | Eval/harness? | Resume at |
| --- | --- | --- |
| no  | no  | Phase 1 (intent) then Phase 4 (build eval), create skeleton skill (Phase 2), then refine |
| yes | no  | Phase 1 (intent) then Phase 4 (build eval), then refine |
| no  | yes | create skeleton skill (Phase 2), then refine |
| yes | yes | Phase 5 (refine directly) |

The fourth row is steady-state iteration. The third is pure TDD: a red harness
waiting for an implementation. A present eval or skill is used as-is and never
clobbered (see "Re-scaffold idempotency"), so a second invocation on a green
skill is a no-op.

## Phase 1: capture intent (delegate)

Hand the intent to the authoring role; it conducts the interview and research.
`skill-forge` does not paraphrase it. The intent feeds *both* the eval (Phase 4)
and any skill draft (Phase 2), which is what lets the eval be built before a skill
body exists.

## Phase 2: author, stub, or refine the SKILL.md (delegate)

The role writes or improves the `SKILL.md`: a `description:` that routes (the
discovery surface), a body that shapes output (the invocation surface), and a
"Common Mistakes" section. In the eval-first entries an empty or skeleton body is
a valid starting point; refinement (Phase 7) fills it against the gate. The file
conforms to the Agent Skills standard.

**Draft gate.** The author confirms each revision before it is evaluated. This is
the same discipline `developer:ailly` enforces: the human reviews before the next
step builds on it.

## Phase 3: reference the skill in place (own)

The harness names the authored `skills/<name>/SKILL.md` directly, via a
`kind: external` prefix block in the invocation assembly (see
[scaffold.md](scaffold.md), "In-place external reference"). Nothing is copied; a
later `SKILL.md` edit is picked up at the next `assemble`. A/Bing two revisions
repoints the external path at a sibling working tree or a
`skills/<name>-<variant>/` directory.

## Phase 4: build the harness from intent (own)

Generate the eval and harness from the captured intent, not from the skill body.
The full generation contract is in [scaffold.md](scaffold.md). In short: write
`disclosure.md` from the `description:`; extend the three assemblies, pointing the
invocation assembly's skill-body prefix at the authored `SKILL.md` with a
`kind: external` block; generate the discovery prompt (a situation, never the
skill name) and the invocation prompt (a task in the skill's domain); derive the
discovery and invocation suites and a `check_<skill>.py` with one rule per
intended failure mode; write `ci.sh` and the two prefix files if absent.

**Review gate.** The author tightens every generated artifact before evaluating.

## Phase 5: evaluate (own)

Run `assemble`, `run`, `eval`, and `report` for discovery, invocation, and
baseline (see [ailly-skill-eval](../../ailly-skill-eval/SKILL.md), "CLI
workflow"). Durable output: committed `runs/<ts-id>/` conversations with inline
trace, plus `evals/reports/`.

## Phase 6: read the gate (own)

Run `report <baseline-id> <invocation-id>` and read its four buckets
(`improved`, `regressed`, `unchanged_pass`, `unchanged_fail`). The gate is
`improved > 0 && regressed == 0`, with discovery routing at or above 0.9 on
`evals/discovery.yaml`. Both arms are scored against the same invocation suite so
the comparison is apples-to-apples.

## Phase 7: refine (delegate), and red vs true null

Iterate only while the loop is genuinely red. Tell red apart from a true null by
*which arm fails*:

- **Red, so loop.** The skilled (invocation) arm still fails an assertion the
  skill is meant to satisfy; or `regressed > 0`; or a checker *errored* (wrote to
  stderr). This includes the eval-first / empty-skill entry, where the first run
  reads `improved == 0` only because the skill has not taught the improvement yet.
  Hand the committed eval report back to the authoring role, which applies its
  improvement reasoning (generalize from feedback, keep the prompt lean, explain
  the why) and returns a revised `SKILL.md`. Re-evaluate. No re-scaffold is needed
  unless the intent itself changed, because `kind: external` picks up the edit at
  the next `assemble`.
- **True null, so stop.** Both arms already pass (`unchanged_pass`): a capable
  model produces the pattern with or without the skill. This is a legitimate
  result about the model, not a failure. Report it; do not loop.

**Never clear `improved == 0` by weakening the checker.** A lenient checker that
no longer fails un-skilled output manufactures a false `improved`. If `improved`
will not move, the cause is either a true null (stop) or a leaky prompt that hands
the baseline the answer (re-frame the prompt at the situation, per the
answer-encoding discipline in [SKILL.md](../SKILL.md)). The checker rules and the
SKILL.md "Common Mistakes" bullets are kept 1:1 as a maintained invariant: an edit
that adds a failure mode updates both, never the checker alone.

## Phase 8: done

Commit. Durability is automatic: every iteration's conversation and trace and
every comparison report is committed. There is no separate iteration ledger in
v1; the committed `runs/` and `reports/` are the record.

## Re-scaffold idempotency

Re-running the build on an existing harness updates only the changed skill's
generated artifacts and never clobbers a sibling's hand-tightened prompt or
checker. `kind: external` removes the skill body from this concern entirely:
there is no vendored copy to regenerate or protect, because the body is referenced
in place. Idempotency therefore asserts on the author-owned prompts and checkers
only, which the scaffolder treats as author-owned once tightened and regenerates
only on explicit request.

## Reproducibility

"Reproducible" means *converges to the same gate*, not byte-identical text. The
harness shape and the eval (the spec) are deterministic given the intent. The
refined `SKILL.md` prose varies run to run because authoring is model-driven. A
second forge of the same intent reaches the same green gate (routing at or above
0.9, `improved > 0 && regressed == 0`) over the same harness shape, with a skill
body that satisfies the same checker rules, not the same words.
