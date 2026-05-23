# Patterns skill eval

This project shows how to use Ailly to create a test suite for skills, and run the full evaluation.

Regression check for the `patterns:*` plugin from [davidsouther/domain-driven-design](https://github.com/davidsouther/domain-driven-design). The full plugin ships seventeen pattern skills; this eval runs a deliberately minimal cross-section that exercises the two failure modes that matter when any `SKILL.md` is edited:

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
├── context/
│   └── system/
│       └── 00-load-patterns-plugin.md             # /plugin install ./domain-driven-design
├── assemblies/
│   ├── discovery.yaml
│   └── invocation.yaml
├── prompts/
│   ├── discovery/
│   │   ├── newtype-mixed-ids.md                   # "We keep passing UserId where OrderId is expected."
│   │   ├── newtype-vs-evs-order-line.md           # OrderLine has price math → NOT newtype.
│   │   ├── configuring-first-log-line.md          # "First log line in main has nowhere to go."
│   │   ├── emitting-order-placed.md               # Logging order.placed inside a request handler.
│   │   ├── paired-add-propagator.md               # "Where do I install the W3C propagator?"
│   │   └── paired-log-handler-success.md          # "How do I record a successful create_order?"
│   └── invocation/
│       ├── newtype-wrap-user-id.md                # Construction task: wrap a string UserId.
│       ├── configuring-service-pipeline.md        # Stand up the five-layer registry in main.
│       └── emitting-order-placed.md               # Emit order.placed with semantic-convention keys.
├── scripts/
│   ├── check_newtype.py                           # Inner primitive is private; constructor is the only entry; no `as` casts at call sites.
│   ├── check_configuring_logging.py               # Single `init`; Registry → Format → Filter → Enrich → Export; resource attributes; shutdown flush.
│   └── check_emitting_logs.py                     # Structured fields only; `EventName` set on business events; OpenTelemetry semantic-convention keys.
└── evals/
    ├── discovery.yaml
    └── invocation.yaml
```

Both assemblies share the same system context (the patterns plugin) and differ only in the user-prompt source. The same evals format runs against either; `text_contains` against the chosen skill name carries discovery, scripts plus an LLM-as-judge carry invocation.

`assemblies/discovery.yaml`:

```yaml
agent_md: ./AGENT.md
system:
  - context/system/00-load-patterns-plugin.md
user_prompt: prompts/discovery/{case}.md
model: claude-sonnet-4-6
cache_breakpoints: [after_system]
```

## Discovery (skill selection from description)

Each case names a coding situation and asserts on which skill the model loads. The paired cases under `emitting-logs` and `configuring-logging` are where this suite earns its keep. Both descriptions mention "logging"; the trigger lives in *once at process start* versus *every time code emits a log record*. An edit that blurs that distinction lights up the paired cases without breaking either single-skill case.

`evals/discovery.yaml`:

```yaml
cases:
  - name: newtype-for-mixed-ids
    input: prompts/discovery/newtype-mixed-ids.md
    assertions:
      - { type: text_contains, value: "patterns:newtype" }
      - { type: text_not_contains, value: "patterns:entities-value-objects-services" }

  - name: order-line-is-not-newtype
    input: prompts/discovery/newtype-vs-evs-order-line.md
    assertions:
      - { type: text_not_contains, value: "patterns:newtype" }
      - { type: text_contains, value: "patterns:entities-value-objects-services" }

  - name: configuring-for-bootstrap
    input: prompts/discovery/configuring-first-log-line.md
    assertions:
      - { type: text_contains, value: "patterns:configuring-logging" }
      - { type: text_not_contains, value: "patterns:emitting-logs" }

  - name: emitting-for-call-site
    input: prompts/discovery/emitting-order-placed.md
    assertions:
      - { type: text_contains, value: "patterns:emitting-logs" }
      - { type: text_not_contains, value: "patterns:configuring-logging" }

  - name: propagator-is-configuration
    input: prompts/discovery/paired-add-propagator.md
    assertions:
      - { type: text_contains, value: "patterns:configuring-logging" }
      - { type: text_not_contains, value: "patterns:emitting-logs" }
      - type: judge
        prompt: |
          The answer selects patterns:configuring-logging because the W3C
          propagator is installed once at process bootstrap. It does not
          recommend patterns:emitting-logs.

  - name: handler-success-is-emission
    input: prompts/discovery/paired-log-handler-success.md
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

Each case loads exactly one skill plus a construction task. The Python script checks structural conformance, the judge confirms the result is recognisable as the named pattern, and the token budget confirms the skill did not pad the output. The scripts encode the structural rules from each `SKILL.md`'s "Common Mistakes" section; if those rules are reworded out of the prompt, the script is what notices.

`evals/invocation.yaml`:

```yaml
cases:
  - name: newtype-wraps-user-id
    input: prompts/invocation/newtype-wrap-user-id.md
    assertions:
      - { type: script, runtime: Python, script: { path: scripts/check_newtype.py } }
      - type: judge
        prompt: |
          The code introduces a UserId type that wraps a string, exposes
          construction as the only sanctioned entry point, and makes a plain
          string unassignable where UserId is required. There are no `as`
          casts at call sites; validation lives once in the constructor.
      - { type: tokens, metric: total, op: "<", value: 6000 }

  - name: configuring-five-layer-service
    input: prompts/invocation/configuring-service-pipeline.md
    assertions:
      - { type: script, runtime: Python, script: { path: scripts/check_configuring_logging.py } }
      - type: judge
        prompt: |
          The bootstrap installs a single subscriber registry in main with
          Format, Filter, Enrich, and Export layers, attaches `service.*`
          resource attributes, installs the W3C `traceparent` propagator,
          and registers a shutdown flush with a hard timeout. No `init` is
          called from library code.
      - { type: tokens, metric: total, op: "<", value: 8000 }

  - name: emitting-order-placed-with-event-name
    input: prompts/invocation/emitting-order-placed.md
    assertions:
      - { type: script, runtime: Python, script: { path: scripts/check_emitting_logs.py } }
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
# Run both suites
ailly -p e2e/patterns-eval eval --suite all

# Run a single suite
ailly -p e2e/patterns-eval eval --suite discovery
ailly -p e2e/patterns-eval eval --suite invocation

# Sweep two plugin versions over the same prompts
for v in v1 v2; do
  cp context/system/00-load-$v.md context/system/00-load-patterns-plugin.md
  ailly -p e2e/patterns-eval assemble --all
  ailly -p e2e/patterns-eval run --suite invocation
done
ailly diff runs/v1-* runs/v2-*
```

A regression in this minimal cross-section reads as a 3 × 2 matrix: skill × {discovery, invocation}. The paired-skill cases inside discovery catch the failure mode that single-skill cases would miss: when a `description:` edit pulls two paired skills' triggers toward each other, both per-skill cases still pass and only the cross case shows the blur. The report format matches the insurance-claim handler's regression output, so one CI step reads both. Add `ailly -p e2e/patterns-eval eval --suite all` alongside the existing `ailly -p e2e/insurance-claim eval --suite regression` pipeline step.

Extending coverage to the remaining fourteen patterns reuses the two-axis template above; the test surface grows by one prompt per skill per axis and one Python checker per invocation case.
