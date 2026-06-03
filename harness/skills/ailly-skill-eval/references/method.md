# The skill-eval method

A long-form walk-through of how to build an Ailly project that regression-tests
a skillset, one part at a time with its rationale. The worked example this
generalizes is [e2e/patterns-eval/](../../../e2e/patterns-eval/); read it
alongside this guide. Schemas for the `assembly`, `conversation`, and
`evaluation` formats live in [DESIGN.md](../../../DESIGN.md) and are never
restated here.

The whole method rests on two facts about a skill. Its `description:`
frontmatter is what routes an agent to it (the **discovery** surface), and its
body is what shapes the output once the agent loads it (the **invocation**
surface). An edit to either can regress independently, so the suite tests them
on separate axes.

## 1. Vendoring skills under test

Copy each skill's `SKILL.md` verbatim — frontmatter and body — into
`context/skills/<name>/SKILL.md`, matching the upstream Claude Code skill
layout. Vendoring buys three things:

- **No plugin-install dependency.** The eval scores files in the repo, not a
  plugin the CI runner has to fetch and install first.
- **The exact text is pinned.** You are scoring against a known revision of the
  skill, not whatever the live plugin happens to ship.
- **A sweep can repoint the prefix.** To A/B two revisions of one skill, copy a
  sibling into `context/skills/<name>-<variant>/SKILL.md` and point the
  assembly's prefix path at the variant. The rest of the suite is unchanged.

A bootstrap/routing skill (in patterns-eval, `using-patterns`) is vendored too
and listed first in the prefix, so the model has the routing table before any
individual skill body.

## 2. Building `disclosure.md`

The discovery axis selects from a routing table, and that table is
`context/skills/disclosure.md`: the concatenated `description:` frontmatter of
every candidate skill, one block per skill. It is the *entire* discovery
surface — the model sees these descriptions and nothing of the bodies, exactly
as it would when a plugin presents its skill list. Building the table by
concatenating the real frontmatter (rather than paraphrasing) keeps the thing
under test identical to the thing that ships.

## 3. Discovery case design

A discovery assembly fixes its prefix (the disclosure table plus the bootstrap
skill — no individual skill bodies) and sweeps a `case` matrix. Each prompt
names a coding situation; each eval case is keyed by the conversation filename
and asserts which skill the model named.

Two kinds of case:

- **Single-skill cases.** One situation that should route to exactly one skill.
  Assert `text_contains` for the right skill and `text_not_contains` for its
  nearest neighbour. (patterns-eval: `newtype-mixed-ids` must name
  `patterns:newtype` and must not name `patterns:entities-value-objects-services`.)
- **Paired cross-cases.** The technique that detects *description blur*. When two
  skills are a pair whose descriptions both touch the same topic, a single-skill
  case for each still passes after an edit that pulls their triggers together;
  only a case framed at the seam between them catches it. patterns-eval's logging
  pair is the model: both descriptions mention "logging", and the only trigger
  separating them is *once at process start* versus *every time code emits a log
  record*. The paired cases (`paired-add-propagator`,
  `paired-log-handler-success`) add a `judge` on top of the `text_contains` /
  `text_not_contains` combo to confirm the model chose for the *right reason*,
  not by luck.

This is why a discovery suite has more prompts than skills: each skill gets a
base case, and each overlapping pair gets an extra cross-case.

## 4. Invocation case design

An invocation assembly loads exactly one skill body into the prefix (the prefix
path templated on the matrix value) and sweeps a `skill` matrix. Each prompt
asks for output in that skill's domain — patterns-eval asks for code: wrap a
string `UserId`, stand up the five-layer logging registry, emit an
`order.placed` record. The eval case mixes up to three assertion kinds:

- **A `judge`** — the medium-neutral structural check. It confirms the output is
  recognizable as the named pattern. This is the assertion that works for *any*
  output medium, because it reads the text the way a reviewer would.
- **An optional `script` / `program` checker** — a mechanical check for output
  that is parseable (code, JSON, anything with structure). It encodes the
  skill's "Common Mistakes" as an ordered list of rules; on the first violated
  rule it prints a single-line reason to **stdout** and exits non-zero, leaving
  **stderr untouched**. That empty-stderr-on-fail contract is what makes the
  runner record a genuine `Fail` rather than an `Errored` broken checker — a
  checker that crashes (writes to stderr) is a different outcome from a checker
  that judged the candidate and failed it. Each rule should trace 1:1 to a
  "Common Mistakes" bullet, so a reworded skill body that drops a rule is what
  the checker notices.
- **A `tokens` budget** — bounds output size so a skill cannot pass by padding.
  Keep the budget *equal* across the invocation and baseline arms: a size
  difference between the arms should read as an `improved`/`regressed` signal,
  not a budget artifact. (patterns-eval lifts the `configuring-logging` budget to
  14000 on both arms because the full five-layer pipeline legitimately runs
  larger than the baseline's partial attempt.)

## 5. Output-medium agnosticism

The harness checks text, so the method covers anything an LLM writes — code,
prose, structured data, a plan. The output medium is not fixed by the harness;
it is pinned by the **candidate-project context**. In patterns-eval that is
`context/AGENTS.md`, which tells the model the codebase is TypeScript and every
answer must be TypeScript source. A prose skillset would pin its format there
instead and lean on `judge`, `text_matches`, and `text_semantic_match` rather
than a code checker. The `script`/`program` checker is an *optimization* for
parseable output, not a requirement of the method; when output is not
mechanically parseable, the `judge` carries the invocation axis alone.

## 6. The falsification arm

A skill earns its place only by changing behaviour a baseline model would
otherwise get wrong. To prove that, build a **baseline** arm: the same
invocation prompts, run against a prefix with the skill removed. In patterns-eval
the baseline prefix is just the two `AGENTS.md` files; the invocation prefix adds
the bootstrap skill and the one skill body. Both arms run the *same* eval
assertions, so any difference is attributable to the skill.

Compare the two run directories with `report <baseline-id> <invocation-id>`. The
comparison report sorts every assertion into four buckets:

- `improved` — failed on baseline, passed on invocation (the skill helped).
- `regressed` — passed on baseline, failed on invocation (the skill harmed).
- `unchanged_pass` — passed on both.
- `unchanged_fail` — failed on both.

The falsification gate is two conditions: `improved > 0` and `regressed == 0`.
The skill must help on at least one assertion the baseline failed, and must
break nothing the baseline passed. A checker too lenient to fail un-skilled
output yields `improved == 0` and the gate fails — which is the point, because a
gate that never fails proves nothing.

### Reading a deliberate null result

Not every skill produces improvement, and that is a feature. In patterns-eval,
`emitting-logs` is the clear positive — without it the model interpolates values
into the message body and skips the event name; with it the record is structured
under semantic-convention keys, and the judge flips from fail to pass. (The
structural checker encodes those same three changes as rules, but a capable
baseline often already satisfies some of them, so it is the judge that reliably
carries the `improved` signal — both `improved` assertions in the committed
comparison report are judges.) `configuring-logging` improves on the judge alone
(a fuller pipeline than the baseline's partial attempt).

`newtype` is a deliberate **null result**. A capable model already reaches for
brand types when asked for swap-proof ids, with or without the skill, so both
arms pass and the skill lands in `unchanged_pass`, contributing nothing to
`improved`. It is retained precisely to show what a skill that does *not* change
a capable model's output looks like in the report. The gate does not depend on
it — `improved > 0 && regressed == 0` is carried by the skills that do clear the
bar. When you read a null result, do not "fix" it by weakening the checker until
the baseline fails; that manufactures a false `improved`. A null result is a
true statement about the model, not a defect in the suite.

### Fidelity rule

Describe what the project *builds*, not a README narrative that has drifted from
it. The built falsification arm is named `baseline` — `assemblies/baseline.yaml`
and `evals/baseline.yaml`. An earlier draft of the patterns-eval README narrated
a differently-named arm; when the prose and the built files disagree, trust the
files and reconcile the prose. (That README has since been reconciled to
`baseline`, so the two now agree.)

## 7. Extending to N skills

The two-assembly template scales linearly:

- **Invocation** — one prompt, one matrix entry, and (for parseable output) one
  checker per skill.
- **Discovery** — one base case per skill, plus one extra paired cross-case for
  every two skills whose descriptions overlap. This is why the discovery prompt
  count exceeds one-per-skill: patterns-eval has six discovery prompts for three
  skills, because the logging pair and the `newtype` neighbour each need a seam
  case.

Adding the remaining skills of a plugin is mechanical: it grows the matrix and
the prompt/checker count, not the structure.

## 8. CLI workflow

The operator's journey is four subcommands. You run them — by hand while
iterating, or scripted into whatever CI your project uses — gating the live
steps on credentials (`ANTHROPIC_API_KEY` in the shell or a project `.env`):
`assemble` runs without a model, while `run`/`eval`/`report` need one.

- `assemble <suite>` — expand the matrix; write one conversation skeleton per
  binding under `runs/<id>/`.
- `run <run-dir>` — fill each blank assistant turn by calling the model.
- `eval <suite> --over <run-dir>` — score the conversations; write a per-run
  report.
- `report <run-id>` — single-run summary; `report <id-a> <id-b>` — the
  comparison report that surfaces the four falsification buckets and the gate.

`report` is a real subcommand: single-run summary, or the two-arm comparison
that surfaces the falsification buckets. The top-level README still says "three
commands" and predates it.
