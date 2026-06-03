# Patterns skill eval

This project shows how to use Ailly to create a test suite for skills, and run the full evaluation.

Regression check for the `patterns:*` plugin from [davidsouther/domain-driven-design](https://github.com/davidsouther/domain-driven-design). The full plugin ships seventeen pattern skills; this eval runs a minimal cross-section that exercises the two failure modes that matter when any `SKILL.md` is edited:

- **Discovery.** Given a code base, does the model pick the right skill from its `description:` frontmatter alone?
- **Invocation.** Once the skill is loaded, does the model produce code that structurally exhibits the pattern?

The minimal cross-section is three skills with overlapping discovery surfaces:

| Skill | Discovery Neighbour | Notes |
|---|---|---|
| `patterns:newtype` | `entities-value-objects-services` | A wrapped-primitive concept and a value-object-with-behaviour can read the same in one sentence. |
| `patterns:configuring-logging` | `emitting-logs` | Paired skill: bootstrap-cadence. Description overlap with its partner is the entire risk. |
| `patterns:emitting-logs` | `configuring-logging` | Paired skill: per-call-site cadence. Same risk, opposite side. |

Three skills cover the two hard discovery cases (a paired skill set whose triggers blur in description edits, and a standalone whose neighbour looks the same but behaves differently) and the two hard invocation cases (a wrapping pattern that is invisible without a structural check at the call site, and a bootstrap pattern that is invisible without inspecting the full registry stack).

## Test surface

```
e2e/patterns-eval/
├── AGENTS.md                                      # Named explicitly in both assemblies' prefix
├── context/
│   └── skills/                                    # Patterns skills vended directly into this project, one SKILL.md per directory, matching the upstream Claude Code skill layout.
│       ├── using-patterns/SKILL.md                # Bootstrap routing table; lists every patterns:* skill the model may name in discovery.
│       ├── newtype/SKILL.md                       # The newtype skill, frontmatter and body verbatim from the upstream patterns plugin.
│       ├── configuring-logging/SKILL.md           # Bootstrap-cadence half of the paired logging set.
│       └── emitting-logs/SKILL.md                 # Per-call-site half of the paired logging set.
│                                                  # Sibling SKILL.md revisions can be added (e.g. newtype-v1/SKILL.md, newtype-v2/SKILL.md) for the version sweep below by pointing the assembly prefix at the pinned directory.
├── assemblies/
│   ├── discovery.yaml                             # prefix + conversation skeleton + matrix over discovery cases
│   ├── invocation.yaml                            # positive arm: skill loaded; matrix over invocation cases
│   └── baseline.yaml                              # negative arm: no skill loaded (falsification)
├── prompts/
│   ├── discovery/
│   │   ├── newtype-mixed-ids.md                   # "We keep passing UserId where OrderId is expected."
│   │   ├── newtype-vs-evs-order-line.md           # OrderLine has price math → NOT newtype.
│   │   ├── configuring-first-log-line.md          # "First log line in main has nowhere to go."
│   │   ├── emitting-order-placed.md               # Logging order.placed inside a request handler.
│   │   ├── paired-add-propagator.md               # "Where do I install the W3C propagator?"
│   │   └── paired-log-handler-success.md          # "How do I record a successful create_order?"
│   └── invocation/
│       ├── newtype.md                             # Construction task: wrap a string UserId.
│       ├── configuring-logging.md                 # Stand up the five-layer registry in main.
│       └── emitting-logs.md                       # Emit order.placed with semantic-convention keys.
├── runs/                                          # One conversation .yaml per matrix binding, per assembly
│   ├── 2026-05-23T10-00-discovery/
│   │   ├── newtype-mixed-ids.yaml
│   │   ├── newtype-vs-evs-order-line.yaml
│   │   └── ...                                    # six files total, one per discovery case
│   ├── 2026-05-23T10-05-invocation/
│   │   ├── newtype.yaml
│   │   ├── configuring-logging.yaml
│   │   └── emitting-logs.yaml
│   └── 2026-05-23T10-06-baseline/                 # same cases, no skills loaded
│       ├── newtype.yaml
│       ├── configuring-logging.yaml
│       └── emitting-logs.yaml
└── evals/
    ├── discovery.yaml                             # case `name` matches conversation filename
    ├── invocation.yaml                            # positive arm: skill loaded
    ├── baseline.yaml                              # negative arm: no skill loaded (falsification)
    ├── scripts/                                   # used by `program`/`script` assertions only
    │   ├── check_newtype.py                       # Inner primitive is private; constructor is the only entry; no `as` casts at call sites.
    │   ├── check_configuring_logging.py           # Single `init`; Registry → Format → Filter → Enrich → Export; resource attributes; shutdown flush.
    │   └── check_emitting_logs.py                 # Structured fields only; `EventName` set on business events; OpenTelemetry semantic-convention keys.
    └── reports/
```

The discovery assembly uses a single `case` axis and a fixed prefix (routing table only, no individual skill files). The invocation assemblies use a single `skill` axis and differ only in whether the relevant skill is included in the prefix. The same eval assertions run against both arms; a skill is confirmed useful when the positive arm passes assertions that the baseline arm fails.

`assemblies/discovery.yaml`:

```yaml
name: discovery
model: claude-sonnet-4-6

matrix:
  case:
    - newtype-mixed-ids
    - newtype-vs-evs-order-line
    - configuring-first-log-line
    - emitting-order-placed
    - paired-add-propagator
    - paired-log-handler-success

prefix:
  - { kind: file,   path: ./AGENTS.md,                                cache: true }
  - { kind: system, path: context/skills/disclosure.md,               cache: true }
  - { kind: system, path: context/skills/using-patterns/SKILL.md,     cache: true }

conversation:
  - { role: user, path: "prompts/discovery/{{ case }}.md" }
  - { role: assistant }
```

`assemblies/invocation.yaml` (positive arm — one skill per run):

```yaml
name: invocation
model: claude-sonnet-4-6

matrix:
  skill:
    - newtype
    - configuring-logging
    - emitting-logs

prefix:
  - { kind: file,   path: ./AGENTS.md,                                  cache: true }
  - { kind: file,   path: ./context/AGENTS.md,                          cache: true }
  - { kind: system, path: ./context/skills/using-patterns/SKILL.md,     cache: true }
  - kind: system
    path: "context/skills/{{ skill }}/SKILL.md"
    cache: true

conversation:
  - { role: user, path: "prompts/invocation/{{ skill }}.md" }
  - { role: assistant }
```

`assemblies/baseline.yaml` (negative arm — no skill loaded):

```yaml
name: baseline
model: claude-sonnet-4-6

matrix:
  skill:
    - newtype
    - configuring-logging
    - emitting-logs

prefix:
  - { kind: file, path: ./AGENTS.md,         cache: true }
  - { kind: file, path: ./context/AGENTS.md, cache: true }

conversation:
  - { role: user, path: "prompts/invocation/{{ skill }}.md" }
  - { role: assistant }
```

The four SKILL.md files are vended into the project at `context/skills/<name>/SKILL.md`. Each file is a verbatim copy of the upstream `patterns:*` skill from [davidsouther/domain-driven-design](https://github.com/davidsouther/domain-driven-design), including frontmatter. Vending the skills directly removes the implicit dependency on a Claude Code plugin-install step and lets the eval pin the exact skill text being scored against. `using-patterns` is listed first so the model has the routing table before any individual skill body; the three pattern skills follow in alphabetical order. To pin a different revision for a sweep, copy a sibling SKILL.md into `context/skills/<name>-<variant>/SKILL.md` and adjust the assembly prefix to point at the variant.

## Discovery (skill selection from description)

Each case names a coding situation and asserts on which skill the model loads. The paired cases under `emitting-logs` and `configuring-logging` carry the discovery axis. Both descriptions mention "logging"; the trigger lives in *once at process start* versus *every time code emits a log record*. An edit that blurs that distinction lights up the paired cases without breaking either single-skill case.

`evals/discovery.yaml`. The case `name` matches the conversation filename produced by `matrix.case`.

```yaml
cases:
  - name: newtype-mixed-ids
    assertions:
      - { type: text_contains, value: "patterns:newtype" }
      - { type: text_not_contains, value: "patterns:entities-value-objects-services" }

  - name: newtype-vs-evs-order-line
    assertions:
      - { type: text_not_contains, value: "patterns:newtype" }
      - { type: text_contains, value: "patterns:entities-value-objects-services" }

  - name: configuring-first-log-line
    assertions:
      - { type: text_contains, value: "patterns:configuring-logging" }
      - { type: text_not_contains, value: "patterns:emitting-logs" }

  - name: emitting-order-placed
    assertions:
      - { type: text_contains, value: "patterns:emitting-logs" }
      - { type: text_not_contains, value: "patterns:configuring-logging" }

  - name: paired-add-propagator
    assertions:
      - { type: text_contains, value: "patterns:configuring-logging" }
      - { type: text_not_contains, value: "patterns:emitting-logs" }
      - type: judge
        prompt: |
          The answer selects patterns:configuring-logging because the W3C
          propagator is installed once at process bootstrap. It does not
          recommend patterns:emitting-logs.

  - name: paired-log-handler-success
    assertions:
      - { type: text_contains, value: "patterns:emitting-logs" }
      - { type: text_not_contains, value: "patterns:configuring-logging" }
      - type: judge
        prompt: |
          The answer selects patterns:emitting-logs because the success
          occurs inside an already-bootstrapped handler. It does not
          recommend re-running patterns:configuring-logging.
```

## Invocation (skill used correctly in pattern)

Each invocation prompt targets one specific skill. The falsification test runs two arms against the same prompts: `invocation` loads the relevant skill; `baseline` loads no skill at all. A skill is confirmed useful when the `invocation` arm passes assertions that the `baseline` arm fails. Both arms use the same eval assertions.

Run names match the `skill` matrix axis: `newtype.yaml`, `configuring-logging.yaml`, `emitting-logs.yaml`.

The Python script checks structural conformance, the judge confirms the result is recognisable as the named pattern, and the token budget confirms the skill did not pad the output. The scripts encode the structural rules from each `SKILL.md`'s "Common Mistakes" section; if those rules are reworded out of the prompt, the script is what notices.

`evals/invocation.yaml` (also `evals/baseline.yaml`, identical assertions):

```yaml
cases:
  - name: newtype
    assertions:
      - { type: script, runtime: python, script: { path: evals/scripts/check_newtype.py } }
      - type: judge
        prompt: |
          The code introduces a UserId type that wraps a string, exposes
          construction as the only sanctioned entry point, and makes a plain
          string unassignable where UserId is required. There are no `as`
          casts at call sites; validation lives once in the constructor.
      - { type: tokens, metric: total, op: "<", value: 6000 }

  - name: configuring-logging
    assertions:
      - { type: script, runtime: python, script: { path: evals/scripts/check_configuring_logging.py } }
      - type: judge
        prompt: |
          The bootstrap installs a single subscriber registry in main with
          Format, Filter, Enrich, and Export layers, attaches `service.*`
          resource attributes, installs the W3C `traceparent` propagator,
          and registers a shutdown flush with a hard timeout. No `init` is
          called from library code.
      - { type: tokens, metric: total, op: "<", value: 14000 }

  - name: emitting-logs
    assertions:
      - { type: script, runtime: python, script: { path: evals/scripts/check_emitting_logs.py } }
      - type: judge
        prompt: |
          The call site emits a structured log record with `EventName` set
          to `order.placed` and attaches fields under OpenTelemetry
          semantic-convention keys (`http.response.status_code`, `order.id`,
          `user.id`). The message body is a stable string; no values are
          interpolated into it.
      - { type: tokens, metric: total, op: "<", value: 6000 }
```

## Workflow

```sh
# Discovery
ailly -p e2e/patterns-eval assemble discovery                            # → runs/<ts>-discovery/*.yaml
ailly -p e2e/patterns-eval run runs/<ts>-discovery/                      # fill assistant turns
ailly -p e2e/patterns-eval eval discovery --over runs/<ts>-discovery/
ailly -p e2e/patterns-eval report <ts>-discovery                         # single-run summary

# Invocation falsification: run positive and baseline arms, then compare
ailly -p e2e/patterns-eval assemble invocation                           # → runs/<ts>-invocation/*.yaml
ailly -p e2e/patterns-eval run runs/<ts>-invocation/
ailly -p e2e/patterns-eval eval invocation --over runs/<ts>-invocation/

ailly -p e2e/patterns-eval assemble baseline                             # → runs/<ts>-baseline/*.yaml
ailly -p e2e/patterns-eval run runs/<ts>-baseline/
ailly -p e2e/patterns-eval eval baseline --over runs/<ts>-baseline/

# Compare the arms; the comparison report sorts every assertion into the four
# falsification buckets (improved / regressed / unchanged_pass / unchanged_fail).
ailly -p e2e/patterns-eval report <ts>-baseline <ts>-invocation

# Sweep two skill revisions. Copy the SKILL.md to context/skills/<name>-<variant>/SKILL.md,
# update the assembly prefix path, then assemble + run + eval each revision.
for v in v1 v2; do
  sed -i.bak "s|context/skills/newtype/SKILL.md|context/skills/newtype-$v/SKILL.md|" \
    assemblies/invocation.yaml
  ailly -p e2e/patterns-eval assemble invocation
  ailly -p e2e/patterns-eval run runs/<ts>-invocation/
  mv runs/<ts>-invocation runs/$v
  ailly -p e2e/patterns-eval eval invocation --over runs/$v/
  mv assemblies/invocation.yaml.bak assemblies/invocation.yaml
done
ailly -p e2e/patterns-eval report v1 v2                                  # compare the two revisions
```

A regression in this minimal cross-section reads as a 3 × 3 matrix: skill × {discovery, invocation, baseline}. The paired-skill cases inside discovery catch the failure mode that single-skill cases would miss: when a `description:` edit pulls two paired skills' triggers toward each other, both per-skill cases still pass and only the cross case shows the blur. The falsification pair (invocation vs baseline) catches the failure mode where a skill edit removes the structural guidance that the Python checker enforces — the baseline arm will start passing assertions it previously failed, which is the signal that the skill is no longer contributing.

The report format matches the insurance-claim handler's regression output, so one CI step reads both. Extending coverage to the remaining fourteen patterns reuses the two-assembly template above; the test surface grows by one prompt per skill per axis, one entry per skill in the matrix, and one Python checker per invocation case.
