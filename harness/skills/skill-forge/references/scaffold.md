# The scaffold contract

What `skill-forge` generates in Phase 4 (build the harness from intent), and the
disciplines the generation must hold. The harness shape is the `patterns-eval`
shape, minus the vendored skill body, because the body is referenced in place.
For the worked output, read [e2e/clean-comments-review/](../../../e2e/clean-comments-review/);
for the harness anatomy these files inhabit, read
[ailly-skill-eval](../../ailly-skill-eval/SKILL.md). This file does not restate
the anatomy; it states the generation rules.

## Derived from intent, not from the skill body

The eval is the spec. It is generated from the captured **intent** (the
situations the skill should route on, and the output properties a correct
response must exhibit), not from the `SKILL.md` body. This is what lets the
harness exist before a meaningful body does (the eval-first entry states in
[loop.md](loop.md)), and it removes a circularity: an empty skill has no
"Common Mistakes" bullets or Output-format from which to derive a checker and a
judge. Deriving the eval from the body would also have the spec inherit the very
body it is meant to test.

## Files emitted

For a forged skill `<name>`, the scaffold writes the `patterns-eval` shape minus
the vendored body:

- `context/skills/disclosure.md` — the discovery routing table: the `description:`
  frontmatter block for `<name>` (and any sibling skills under test). This is the
  entire discovery surface the model selects from.
- `assemblies/discovery.yaml` — prefix is the constitution plus `disclosure.md`;
  matrix sweeps the discovery cases.
- `assemblies/invocation.yaml` — prefix loads the skill body via a `kind: external`
  block (see below); matrix sweeps the skill.
- `assemblies/baseline.yaml` — identical to invocation minus the skill-body
  prefix, so the loaded skill is the only difference between the arms.
- `prompts/discovery/<name>.md` — a situation that routes to the skill, which
  never names it (see "Prompt framing").
- `prompts/invocation/<name>.md` — a task in the skill's domain, answered by both
  the invocation and baseline arms.
- `evals/discovery.yaml` — `text_contains` / `text_not_contains` that the model
  named the right skill and not its nearest rival, optionally a paired `judge`.
- `evals/invocation.yaml` — a `judge` derived from the desired output format, a
  `script` running `check_<name>.py`, and a `tokens` budget that is **equal across
  arms** so output size is never an Improved/Regressed signal.
- `evals/scripts/check_<name>.py` — the structural checker (see "The checker").
- `AGENTS.md`, `context/AGENTS.md` — the two prefix files (the constitution and
  the candidate-project file), if absent. They frame the task and the output
  format, never the technique.
- `ci.sh` — the live forge driver (assemble/run/eval/report across the three
  arms), with the `improved > 0 && regressed == 0` gate.

## In-place external reference

The invocation assembly names the authored body in place, never a copy:

```yaml
- { kind: external, path: ../../skills/<name>/SKILL.md, cache: true }
```

The repo-relative path climbs out of the harness root
(`e2e/<name>` -> `e2e` -> repo root) to the sibling `skills/` tree. `kind: external`
is the path sandbox's sanctioned escape (it anchors on the canonical host root,
resolves the `..`, and rejects absolute paths and host-rootless projects before
any read). Nothing is vendored: there is no `context/skills/<name>/` copy to
generate, sync, or protect. `assemble` reads the external file once and pins its
text inline into the committed conversation, so replay stays hermetic and
`run`/`report` never re-touch the source. A later `SKILL.md` edit is picked up at
the next `assemble`.

(This differs from a fixture. A fixture skill under `context/skills/<name>/` is
frozen test data with no external source to track, referenced by ordinary
`kind: system` paths. A forged skill has a live authoritative source in the same
repo, so it is referenced, not copied. Vendoring survives only as the cross-repo
fallback, which is deferred.)

## Prompt framing: never encode the answer

A generated **discovery** prompt describes the coding situation and never names
the skill. Routing is earned from the situation, not handed over. The
baseline-shared prefix (`AGENTS.md`, `context/AGENTS.md`) and every generated
**baseline** prompt must contain no skill routing identifier: not the bare `name`
(e.g. `clean-comments-review`), and not its `<plugin>:<name>` form. The technique
the skill teaches must live only in the `SKILL.md` the invocation arm loads.

This is falsifiable by grep on the generated files, and it is enforced for free by
the gate: a leaky scaffold hands the baseline the answer, so the baseline arm
passes too and `improved == 0`. When the gate reads `improved == 0`, re-frame the
prompt at the situation; never weaken the checker (see [loop.md](loop.md),
"red vs true null").

## Judge prompts derived from the output format

The invocation `judge` prompt is derived from the skill's desired Output-format:
it describes what a correct response looks like (its structure and the properties
it must exhibit), so the judge confirms the output reads as the named pattern. It
is a medium-neutral check that complements the mechanical `script` checker.

## The checker, and the 1:1 invariant

`check_<name>.py` reads the candidate from stdin, applies ordered rules, and exits
0 when all hold or exits 1 with a single-line reason on stdout. It leaves stderr
untouched, so a miss records `Fail`, not `Errored`. When the skill's output is
prose (a review, a plan), the checker keys on that text directly; when it is code,
it extracts and strips per the harness's checker utilities before applying rules.

Each checker rule is the projection of one `SKILL.md` "Common Mistakes" bullet.
The two are kept **1:1 as a maintained invariant**, both projecting the same
intent. Neither is derived from the other: refining a failure mode updates both
the rule and the bullet together. The checker is never weakened to clear an
`improved == 0` (that manufactures a false improvement); the rules are tuned to
discriminate a skilled response from an un-skilled one, then generalized.
