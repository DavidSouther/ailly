---
name: ailly-skill-eval
description: Use when building or regression-testing an evaluation suite for a collection of LLM-agent skills — asserting that a SKILL.md edit improves or degrades the skills' discovery and invocation — distinct from running a single ad-hoc eval against one prompt.
---

# Authoring a skill-eval suite

A *skillset* is a collection of LLM-agent skills, each a `SKILL.md` with
`description:` frontmatter that routes the model to it and a body that shapes
what the model produces once loaded. Editing any `SKILL.md` risks two
independent regressions:

- **Discovery** — the `description:` no longer routes the model to the right
  skill for the situation. An edit that pulls two skills' triggers together
  passes every single-skill check and only surfaces on a paired case.
- **Invocation** — once loaded, the skill body no longer produces output that
  structurally exhibits the pattern.

This skill describes how to build an Ailly project that regression-tests both
modes, and names the adversarial techniques worth applying. It describes the
machinery; the author decides how much of it to use. The method is medium- and
language-neutral: a skill's output may be code, prose, structured data, or a
plan, and the harness checks it as text. The worked example,
[e2e/patterns-eval/](../../e2e/patterns-eval/), happens to test TypeScript
code-generation skills, but that is the example's choice, not the method's.

For the full long-form walk-through of each part with its rationale, read
[references/method.md](references/method.md). For the `assembly`,
`conversation`, and `evaluation` schemas, link out to
[DESIGN.md](../../DESIGN.md) — this skill never restates them.

## Project anatomy

A skill-eval project is an Ailly content folder. Each part has one job:

- `context/skills/` — the skills under test, each `SKILL.md` vendored verbatim
  into `context/skills/<name>/`. Vendoring removes the plugin-install
  dependency, pins the exact text being scored, and lets a sweep repoint the
  prefix at a variant directory.
- `context/skills/disclosure.md` — the routing table for the discovery axis: the
  concatenated `description:` frontmatter of every candidate skill. It is the
  entire discovery surface the model selects from.
- `assemblies/` — one assembly per arm. Each fixes a `prefix:` (the context
  window built exactly as written) and a `matrix:` (the cases or skills swept),
  then writes one conversation skeleton per binding.
- `prompts/` — the user turns the matrix fills in, one file per case or skill,
  grouped by axis.
- `evals/` — one evaluation suite per arm, its cases keyed by conversation
  filename; `evals/scripts/` holds the optional `script`/`program` checkers.
- `runs/` — the conversation files `assemble` writes and `run` fills; the run
  artifact is the conversation itself.

You drive these parts with the four-subcommand workflow below; wiring that walk
into a CI script and enforcing the gate is your project's job, not a fixed part
of the anatomy.

## The two axes

The suite sweeps two assemblies that hold different things fixed:

- **discovery** — the prefix is fixed (the routing table plus a bootstrap skill,
  no individual skill bodies). The matrix sweeps *cases*. Each prompt names a
  coding situation; the assertions check *which skill the model names*.
- **invocation** — the prefix loads *one* skill body. The matrix sweeps
  *skills*, the per-binding prefix path templated on the matrix value. Each
  prompt asks for output in that skill's domain; the assertions check that the
  *produced output conforms* to the pattern.

## Assertion palette

Pick the `evaluation` assertion types that fit each axis (schema in
[DESIGN.md](../../DESIGN.md)):

- **discovery** — `text_contains` / `text_not_contains` assert the model named
  the right skill and not its neighbour; a `judge` confirms it chose for the
  right reason on a paired case.
- **invocation** — a `judge` is the medium-neutral structural check (does the
  output read as the named pattern?); a `script` or `program` adds a mechanical
  check when the output is parseable (e.g. code), encoding the skill's "Common
  Mistakes" as ordered rules; a `tokens` budget bounds output size.

`text_matches`, `text_semantic_match`, and `json_path` are available for prose
or structured outputs. The author picks what fits the skill's output medium.

## Falsification as an optional layer

To prove a skill *earns its place*, add a **baseline** arm: the same invocation
prompts, run against a prefix with the skill removed. Compare the two arms and
read the four buckets — `improved`, `regressed`, `unchanged_pass`,
`unchanged_fail`. The gate is `improved > 0` (the skill helped on at least one
assertion the baseline failed) and `regressed == 0` (it broke nothing the
baseline passed). This is the adversarial technique the author chooses to apply,
not a mandatory step; [references/method.md](references/method.md) covers it in
full, including how to read a deliberate null result.

## CLI workflow

The operator's journey is four subcommands. You run them — by hand while
iterating, or wired into a CI script you write — and check the falsification
gate yourself. Only `assemble` runs without a model; gate `run`, `eval`, and
`report` on credentials (an `ANTHROPIC_API_KEY` in the shell or a project
`.env`):

- `assemble <suite>` — expand the matrix; write the conversation skeletons.
- `run <run-dir>` — fill each blank assistant turn by calling the model.
- `eval <suite> --over <run-dir>` — score the conversations against the suite.
- `report <run-id>` (single) or `report <id-a> <id-b>` (comparison) — emit the
  report; the comparison is what surfaces the falsification buckets.

(The top-level README still says "three commands"; `report` is the real fourth.)

## Pointers

- [e2e/patterns-eval/](../../e2e/patterns-eval/) — the worked example this skill
  generalizes: discovery and invocation assemblies, the baseline arm, vendored
  skills, the disclosure table, and the script + judge + token assertion mix.
- [references/method.md](references/method.md) — the long-form guide, one section
  per part with its rationale and the adversarial techniques.
- [DESIGN.md](../../DESIGN.md) — the authoritative `assembly`, `conversation`,
  and `evaluation` schemas.
