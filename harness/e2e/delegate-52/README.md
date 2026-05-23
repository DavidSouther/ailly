# DELEGATE-52

A scaled-down reproduction of the delegated-workflow protocol from Laban, Schnabel, and Neville (_LLMs Corrupt Your Documents When You Delegate_, [arXiv:2604.15597v1](https://arxiv.org/abs/2604.15597), Microsoft Research, April 2026), packaged as an Ailly content folder. The original paper measures silent document corruption across **52 professional domains** using **19 LLMs** over long delegated editing workflows. The headline finding is approximately 25% silent corruption by workflow end for frontier models, growing with document size, workflow length, and distractor count. Companion code available at [microsoft/DELEGATE52](https://github.com/microsoft/DELEGATE52).

This e2e runs the same protocol at a smaller scale — a four-domain representative subset, a six-turn workflow, three provider families (OpenAI, Google, Anthropic) — and produces artifacts the paper's per-domain scorers consume directly. The project demonstrates using Ailly for common published experimental setups.

| Ailly claim | How this project demonstrates it |
|---|---|
| **Multi-provider parity from one source of truth** | One assembly recipe; three providers swept via the `providers:` matrix. System fragments, seed documents, distractor corpus, and turn sequence are identical across runs; only the `model:` field varies. No per-provider driver code. |
| **Filesystem-as-history audit trail** | Every turn lands a snapshot under `runs/<ts>/<provider>/turn-<n>/` containing the window, the response, the post-edit document, the per-turn diff, and the trace. The paper's scorers ingest the seed plus the final document directly; intermediate turns are inspectable on disk in plaintext. |
| **Declarative composition of seed plus distractor context** | `context/seeds/` and `context/distractors/` are version-controlled folders; the assembly globs both. Sweeping the distractor count (one of the paper's documented degradation axes) is one variable change, not a code edit. |

## What the protocol does

The paper's delegated-workflow protocol gives an LLM a seed document, a task brief, and a set of distractor files, then runs the model through a sequence of N editing turns. Each turn introduces a small instruction (tighten this paragraph; add a footnote; reorder these sections) that should leave the document's load-bearing facts intact. After the final turn, the document is scored against the seed for silent corruption — facts that drifted, dates that changed, numbers that moved, citations that broke. The paper's notable failure mode is "sparse but severe": frontier models pass surface checks while named entities and numeric values quietly migrate.

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
├── AGENT.md
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
│   └── delegated-workflow.yaml                       # One recipe; provider matrix sweeps it.
├── runs/
│   └── 2026-05-20T09-00-prose-bio/
│       ├── anthropic/
│       │   ├── turn-01/{window.txt,response.json,document.md,diff.patch,trace.json}
│       │   ├── ...
│       │   └── turn-06/{window.txt,response.json,document.md,diff.patch,trace.json}
│       ├── openai/
│       └── google/
├── scripts/                                          # Per-domain scorers ported from microsoft/DELEGATE52.
│   ├── score_prose_bio.py
│   ├── score_code_sql.py
│   ├── score_data_citation.py
│   └── score_notation_music.py
└── evals/
    └── corruption.yaml
```

## Assembly (one recipe, three providers, declarative distractor sweep)

`assemblies/delegated-workflow.yaml`:

```yaml
agent_md: ./AGENT.md
system:
  - context/system/00-delegate-role.md
  - context/system/10-non-corruption-rules.md
seed:
  source: context/seeds/{{ domain }}.md
distractors:
  source: context/distractors/
  glob: "*.md"
  count: "{{ distractor_count | default(3) }}"
turns:
  - prompts/turns/01-tighten.md
  - prompts/turns/02-add-context.md
  - prompts/turns/03-reorder.md
  - prompts/turns/04-soften.md
  - prompts/turns/05-summarise.md
  - prompts/turns/06-finalise.md
providers:
  - { name: anthropic, model: claude-opus-4-7 }
  - { name: openai,    model: gpt-5-turbo }
  - { name: google,    model: gemini-3-pro }
cache_breakpoints: [after_system, after_seed]
variables:
  domain: prose-bio
  distractor_count: 3
```

What this proves about context composition:

- **Single source of truth.** The system prompt, the seed selection, the distractor glob, and the turn sequence appear once. The provider matrix runs each combination without duplicating the recipe per provider; multi-provider parity is the absence of per-provider code, not the presence of an abstraction.
- **Variables are explicit.** `domain` and `distractor_count` are named, so the paper's documented degradation axes (document size, distractor count, workflow length) become CLI sweeps. Workflow length is the cardinality of `turns:`; trimming or extending it is a list edit.
- **Cache breakpoints align with the protocol.** `after_seed` is a natural breakpoint: the seed is constant across turns within a run, so every turn after the first hits the prompt cache for the seed prefix.

## Evaluation (the paper's scorers, run as `program` assertions)

The per-domain scorers from `microsoft/DELEGATE52` are ported under `scripts/` and invoked as `program` assertions. Each scorer reads the final document and the seed and exits non-zero on corruption above the per-domain threshold. The judge assertion adds a cross-provider rollup that the paper's scorers do not by themselves provide.

`evals/corruption.yaml`:

```yaml
cases:
  - name: prose-bio-anthropic
    input: runs/{{ run_id }}/anthropic/turn-06/document.md
    assertions:
      - { type: program, script: "scripts/score_prose_bio.py --seed context/seeds/prose-bio.md" }
      - { type: tokens, metric: total, op: "<", value: 50000 }

  - name: code-sql-openai
    input: runs/{{ run_id }}/openai/turn-06/document.md
    assertions:
      - { type: program, script: "scripts/score_code_sql.py --seed context/seeds/code-sql.md" }

  - name: data-citation-google
    input: runs/{{ run_id }}/google/turn-06/document.md
    assertions:
      - { type: program, script: "scripts/score_data_citation.py --seed context/seeds/data-citation.md" }

  - name: cross-provider-corruption-rollup
    assertions:
      - type: judge
        prompt: |
          For each domain, compare the final document from anthropic, openai,
          and google against its seed. Identify any named entity, date,
          citation, or numeric value that drifted in any provider's output.
          Report the per-provider corruption count and the per-domain worst
          offender. Do not re-score; consume the per-domain scorer output
          attached as `program_outputs`.
```

The cross-provider rollup is the headline output: per-provider corruption counts, per-domain worst offenders, written into `evals/reports/`. The paper's scoring methodology is preserved verbatim by the per-domain scripts; what Ailly contributes is the side-by-side comparison across providers from one recipe.

## A/B testing along the paper's degradation axes

The paper documents three axes along which silent corruption increases: document size, workflow length, and distractor count. Each is a one-variable sweep:

```sh
# Distractor-count sweep: reproduce the paper's degradation curve at fixture scale.
for n in 0 3 6 9; do
  ailly -p e2e/delegate-52 assemble delegated-workflow \
    --var distractor_count=$n --tag distractors-$n
  ailly -p e2e/delegate-52 run --suite corruption --tag distractors-$n
done
ailly diff runs/distractors-0-* runs/distractors-9-*

# Workflow-length sweep: trim turns to 2 or extend to 10.
ailly -p e2e/delegate-52 assemble delegated-workflow --turns 2,3,6,10
ailly -p e2e/delegate-52 run --suite corruption

# Cross-provider sweep is implicit in the assembly's providers: matrix.
```

The diff reports the change in corruption count per provider, per domain, per axis value. Reproducing the paper's degradation curve at fixture scale is a shell loop, not a notebook.

## Workflow at a glance

```
1. Pick a domain and an axis: `--var domain=code-sql --var distractor_count=6`.
2. `ailly -p e2e/delegate-52 assemble delegated-workflow` — one window per provider per turn.
3. `ailly -p e2e/delegate-52 run --suite corruption` — six turns × three providers per domain.
4. Read evals/reports/<ts>.json. Per-provider corruption counts, per-domain worst offender.
5. Commit the run-id. The next change in any system fragment, distractor, or turn prompt is measured against it.
```

## CI integration

```sh
# Per-PR step covers the cheap pair of domains across all three providers.
ailly -p e2e/delegate-52 eval --suite corruption --domains prose-bio,code-sql

# Weekly scheduled step covers the full four-domain matrix and the distractor sweep.
ailly -p e2e/delegate-52 eval --suite corruption --domains all --sweep distractor_count=0,3,6,9
```

Per-PR cost is bounded by domain count; the full sweep runs on a schedule and posts its report alongside the `insurance-claim` and `patterns-eval` reports in the shared format.

## Fidelity notes

- **Domain count.** Four of the paper's 52, chosen for axis variety, not statistical comparability. Adding domains is one file per domain under `context/seeds/` plus one scorer under `scripts/`.
- **Model selection.** Three providers (Anthropic, OpenAI, Google) rather than the paper's 19 models. Adding models is an entry in `providers:` and a model ID.
- **Scorers.** Ported verbatim from `microsoft/DELEGATE52`; no re-implementation of corruption detection.
- **Terminology.** The paper says "delegated workflows"; the project's earlier framing used "round-trip relay", which is approximate. The recipe uses `delegated-workflow` as the assembly name to match the source.
