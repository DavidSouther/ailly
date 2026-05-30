# DELEGATE-52

A scaled-down reproduction of the delegated-workflow protocol from Laban, Schnabel, and Neville (_LLMs Corrupt Your Documents When You Delegate_, [arXiv:2604.15597v1](https://arxiv.org/abs/2604.15597), Microsoft Research, April 2026), packaged as an Ailly content folder. The original paper measures silent document corruption across **52 professional domains** using **19 LLMs** over long delegated editing workflows. The headline finding is approximately 25% silent corruption by workflow end for frontier models, growing with document size, workflow length, and distractor count. Companion code available at [microsoft/DELEGATE52](https://github.com/microsoft/DELEGATE52).

This e2e runs the same protocol at a smaller scale: a four-domain representative subset, a six-turn workflow, and three provider families (OpenAI, Google, Anthropic). It produces artifacts the paper's per-domain scorers consume directly. The project demonstrates using Ailly for common published experimental setups.

| Ailly claim | How this project demonstrates it |
|---|---|
| **Multi-provider parity from one source of truth** | One assembly recipe; provider is one axis of the `matrix:`. System fragments, seed documents, distractor corpus, and turn sequence are identical across runs; only the `model:` field varies per binding. No per-provider driver code. |
| **Filesystem-as-history audit trail** | Each `provider × domain × distractor_count` binding lands as a single conversation file under `runs/<ts>/<binding>.yaml`. The full six-turn transcript is inline: user prompts, filled assistant responses (each carrying the post-edit document), and per-message trace. The paper's scorers read the final assistant turn directly out of the YAML. |
| **Declarative composition of seed plus distractor context** | `context/seeds/` and `context/distractors/` are version-controlled folders; the assembly globs both. Sweeping the distractor count (one of the paper's documented degradation axes) is one matrix-value edit, not a code edit. |

## What the protocol does

The paper's delegated-workflow protocol gives an LLM a seed document, a task brief, and a set of distractor files, then runs the model through a sequence of N editing turns. Each turn introduces a small instruction (tighten this paragraph; add a footnote; reorder these sections) that should leave the document's load-bearing facts intact. After the final turn, the document is scored against the seed for silent corruption. Did facts drifted? Were dates changed? Did numbers move? Any citations break? The paper's notable failure mode is "sparse but severe": frontier models pass surface checks while named entities and numeric values quietly migrate.

This e2e ships four domains chosen as a deliberate scaled-down sample along the corruption axes the paper documents:

| Domain | What can silently corrupt |
|---|---|
| `prose-bio` | Names, dates, places, life events. |
| `code-sql` | Column names, join conditions, aggregation semantics. |
| `data-citation` | Author lists, page ranges, DOIs, year of publication. |
| `notation-music` | Pitches, durations, time signatures, dynamics markings. |

The test surface scales linearly if more of the paper's 52 domains are added.

## Test surface

```
e2e/delegate-52/
├── AGENTS.md                                         # Named explicitly in the assembly prefix
├── context/
│   ├── system/
│   │   ├── 00-delegate-role.md                       # "You edit documents for a busy professional..."
│   │   └── 10-non-corruption-rules.md                # "Preserve all named entities, dates, and citations verbatim."
│   ├── seeds/                                        # One seed document per domain.
│   │   ├── prose-bio.md
│   │   ├── code-sql.md
│   │   ├── data-citation.md
│   │   └── notation-music.md
│   └── distractors/                                  # Globbed by the assembly; sweepable in count.
│       ├── 00-meeting-notes.md
│       ├── 01-style-guide.md
│       └── 02-prior-version.md
├── prompts/
│   └── turns/                                        # The six-turn protocol, one prompt per turn.
│       ├── 01-tighten.md
│       ├── 02-add-context.md
│       ├── 03-reorder.md
│       ├── 04-soften.md
│       ├── 05-summarise.md
│       └── 06-finalise.md
├── assemblies/
│   └── delegated-workflow.yaml                       # prefix + six-turn conversation skeleton + provider × domain × distractor_count matrix
├── runs/
│   └── 2026-05-20T09-00-delegated-workflow/         # One conversation .yaml per matrix binding
│       ├── anthropic-prose-bio.yaml                  # 12 files at the default distractor_count: 3 providers × 4 domains
│       ├── anthropic-code-sql.yaml                   # Each file holds the full six-turn transcript with inline trace
│       ├── ...
│       └── google-notation-music.yaml
└── evals/
    ├── corruption.yaml
    ├── scripts/                                      # Per-domain scorers ported from microsoft/DELEGATE52
    │   ├── score_prose_bio.py
    │   ├── score_code_sql.py
    │   ├── score_data_citation.py
    │   └── score_notation_music.py
    └── reports/
```

## Assembly (one recipe, three providers, declarative distractor sweep)

`assemblies/delegated-workflow.yaml`:

```yaml
name: delegated-workflow

matrix:
  provider:
    - { name: anthropic, model: claude-opus-4-7 }
    - { name: openai,    model: gpt-5-turbo }
    - { name: google,    model: gemini-3-pro }
  domain:           [prose-bio, code-sql, data-citation, notation-music]
  distractor_count: [3]                              # axis sweep: edit to [0, 3, 6, 9] to reproduce the paper's degradation curve

prefix:
  - { kind: file,    path: ./AGENTS.md,                                                    cache: true }
  - { kind: system,  path: context/system/*.md,                                            cache: true }
  - { kind: file,    path: context/seeds/{{ domain }}.md,                                  cache: true }
  - { kind: context, source: context/distractors/, glob: "*.md", count: "{{ distractor_count }}" }

conversation:
  - { role: user, path: prompts/turns/01-tighten.md }
  - { role: assistant }
  - { role: user, path: prompts/turns/02-add-context.md }
  - { role: assistant }
  - { role: user, path: prompts/turns/03-reorder.md }
  - { role: assistant }
  - { role: user, path: prompts/turns/04-soften.md }
  - { role: assistant }
  - { role: user, path: prompts/turns/05-summarise.md }
  - { role: assistant }
  - { role: user, path: prompts/turns/06-finalise.md }
  - { role: assistant }
```

What this proves about context composition:

- **Single source of truth.** The system prompt, the seed file path, the distractor glob, and the turn sequence appear once. Provider, domain, and distractor count are matrix axes; `ailly assemble` writes the cross-product as one conversation file per binding without duplicating the recipe.
- **Axes are explicit.** The paper's documented degradation axes (document size, distractor count, workflow length) are matrix entries (`distractor_count`) or a list edit on `conversation:` (workflow length). No CLI sweep flags are needed.
- **Cache markers align with the protocol.** The `cache: true` on the seed file is the natural breakpoint: the seed is constant across all six turns of a run, so every assistant turn after the first hits the prompt cache for the seed prefix.
- **Multi-turn skeletons are filled in place.** `assemble` writes six blank assistant turns per conversation file; `run` walks them in order, each resolved against the cumulative transcript so far. The post-edit document for turn N lives inside that turn's assistant content.

## Evaluation (the paper's scorers, run as `program` assertions)

The per-domain scorers from `microsoft/DELEGATE52` are ported under `evals/scripts/` and invoked as `program` assertions. Each scorer reads the final assistant turn of a conversation file (the post-edit document at turn six) and the seed, and exits non-zero on corruption above the per-domain threshold. The judge assertion adds a cross-provider rollup that the paper's scorers do not by themselves provide.

`evals/corruption.yaml`. Cases bind a subset of matrix axes; one case template fans out across every binding that matches the `when:` filter. No `input:` field is needed; the eval walks every conversation file in the run directory and applies matching cases.

```yaml
cases:
  - when: { domain: prose-bio }                      # applies to anthropic-prose-bio.yaml, openai-prose-bio.yaml, google-prose-bio.yaml
    assertions:
      - { type: program, script: "evals/scripts/score_prose_bio.py --seed context/seeds/prose-bio.md" }
      - { type: tokens, metric: total, op: "<", value: 50000 }

  - when: { domain: code-sql }
    assertions:
      - { type: program, script: "evals/scripts/score_code_sql.py --seed context/seeds/code-sql.md" }

  - when: { domain: data-citation }
    assertions:
      - { type: program, script: "evals/scripts/score_data_citation.py --seed context/seeds/data-citation.md" }

  - when: { domain: notation-music }
    assertions:
      - { type: program, script: "evals/scripts/score_notation_music.py --seed context/seeds/notation-music.md" }

  - name: cross-provider-corruption-rollup            # no `when:` ⇒ runs once over the whole run directory
    assertions:
      - type: judge
        prompt: |
          For each domain, compare the final document (last assistant turn)
          from anthropic, openai, and google against its seed. Identify any
          named entity, date, citation, or numeric value that drifted in any
          provider's output. Report the per-provider corruption count and
          the per-domain worst offender. Do not re-score; consume the
          per-domain scorer output attached as `program_outputs`.
```

The cross-provider rollup is the headline output: per-provider corruption counts, per-domain worst offenders, written into `evals/reports/`. The paper's scoring methodology is preserved verbatim by the per-domain scripts; what Ailly contributes is the side-by-side comparison across providers from one recipe.

## A/B testing along the paper's degradation axes

The paper documents three axes along which silent corruption increases: document size, workflow length, and distractor count. Each is a single edit.

```sh
# Distractor-count sweep: reproduce the paper's degradation curve at fixture scale.
# Either widen the matrix in the assembly to `distractor_count: [0, 3, 6, 9]` and run once,
# or loop with --var to keep one binding per run directory:
for n in 0 3 6 9; do
  ailly -p e2e/delegate-52 assemble delegated-workflow --var distractor_count=$n
  ailly -p e2e/delegate-52 run runs/<ts>/
  mv runs/<ts> runs/distractors-$n
done
ailly diff runs/distractors-0 runs/distractors-9

# Workflow-length sweep: edit the conversation: list in the assembly to add or drop turns.
# Re-assemble and re-run.

# Cross-provider sweep is implicit in the assembly's provider matrix axis.
```

The diff reports the change in corruption count per provider, per domain, per axis value. Reproducing the paper's degradation curve at fixture scale is a shell loop over one matrix-value edit, not a notebook.

## Workflow at a glance

```
1. Edit the matrix to scope the run, or pin a binding: `--var domain=code-sql --var distractor_count=6`.
2. `ailly -p e2e/delegate-52 assemble delegated-workflow`. One conversation skeleton per matrix binding.
3. `ailly -p e2e/delegate-52 run runs/<ts>/`. Six blank assistant turns are filled per file, in order.
4. `ailly -p e2e/delegate-52 eval corruption --over runs/<ts>/`. Per-domain scorers plus cross-provider judge.
5. Read evals/reports/<ts>.json. Per-provider corruption counts, per-domain worst offender.
6. Commit the run directory. The next change in any system fragment, distractor, or turn prompt is measured against it.
```

## CI integration

```sh
# Per-PR step: narrow the matrix to the cheap pair of domains across all three providers.
# Edit assemblies/delegated-workflow.yaml `matrix.domain` to [prose-bio, code-sql] for the PR branch,
# or run the assembler with --var domain=prose-bio,code-sql to constrain the axis at the CLI.
ailly -p e2e/delegate-52 assemble delegated-workflow --var domain=prose-bio,code-sql
ailly -p e2e/delegate-52 run  runs/<ts>/
ailly -p e2e/delegate-52 eval corruption --over runs/<ts>/

# Weekly scheduled step: full four-domain matrix and the distractor sweep.
# The assembly's matrix is `domain: [prose-bio, code-sql, data-citation, notation-music]` and
# `distractor_count: [0, 3, 6, 9]` on the scheduled branch; otherwise pass --var to widen.
ailly -p e2e/delegate-52 assemble delegated-workflow --var distractor_count=0,3,6,9
ailly -p e2e/delegate-52 run  runs/<ts>/
ailly -p e2e/delegate-52 eval corruption --over runs/<ts>/
```

Per-PR cost is bounded by the narrowed matrix; the full sweep runs on a schedule and posts its report alongside the `insurance-claim` and `patterns-eval` reports in the shared format.

## Fidelity notes

- **Domain count.** Four of the paper's 52, chosen for axis variety, not statistical comparability. Adding domains is one file per domain under `context/seeds/`, one entry in `matrix.domain`, and one scorer under `evals/scripts/`.
- **Model selection.** Three providers (Anthropic, OpenAI, Google) rather than the paper's 19 models. Adding models is an entry in `matrix.provider` with a `model:` field.
- **Scorers.** Ported verbatim from `microsoft/DELEGATE52`; no re-implementation of corruption detection. Each scorer reads the final assistant turn out of the conversation YAML rather than a standalone `document.md`.
- **Terminology.** The paper says "delegated workflows"; the project's earlier framing used "round-trip relay", which is approximate. The recipe uses `delegated-workflow` as the assembly name to match the source.
