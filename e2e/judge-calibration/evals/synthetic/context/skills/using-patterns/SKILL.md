---
name: using-patterns
description: Bootstrap skill for design patterns. Loaded at session start to establish when to invoke each pattern skill.
---

# Design Patterns Workflow

You are working in a project that uses structured design patterns. During a plan phase, invoke an appropriate skill to ensure the plan includes reasonable design patterns.

| Situation | Invoke |
|-----------|--------|
| A primitive type (string, number, UUID) represents a distinct domain concept | `patterns:newtype` |
| Modeling domain objects — deciding what has identity, what is a value, what is a function | `patterns:entities-value-objects-services` |
| Constructing an object with many fields, required vs. optional distinction, or partial-init risk | `patterns:builder` |
| A domain object exposes public fields, leaks mutable collections, or allows state changes via assignment | `patterns:visibility` |
| Data arrives from an external boundary (HTTP, user input, file, message queue) | `patterns:parse-dont-validate` |
| Modeling a state machine, lifecycle stages, or finite set of mutually-exclusive states | `patterns:type-states` |
| Decoupling domain logic from a specific storage technology | `patterns:repository` |
| Defining an operation that must transition the domain from one legal state to another atomically | `patterns:aggregate` |
| Bridging an Aggregate with a Repository inside a single durable transaction | `patterns:unit-of-work` |
| Structuring the application layer, wiring concrete dependencies, or separating domain from HTTP/CLI | `patterns:bootstrap-and-service` |
| Writing or reviewing a test — ensuring setup, action, and assertions are clearly separated | `patterns:arrange-act-assert` |
| Fake implementation passes the first test and the correct generalization is not yet obvious | `patterns:triangulate` |
| Converting one domain type to another, especially when `as` casts, extract-and-rewrap, or duplicated `to_X`/`from_X` pairs appear | `patterns:type-conversion` |
| Bootstrapping a logging pipeline at process start — subscriber, formatter, filter, resource attributes, W3C trace propagator, exporter, sampling, redaction, graceful shutdown | `patterns:configuring-logging` |
| Writing a log call site — choosing a severity, attaching fields under OpenTelemetry semantic conventions, scoping spans to units of work, naming business events, logging an error chain once at the boundary | `patterns:emitting-logs` |

## Pattern Composition

Patterns compose in predictable ways — recognise these combinations:

- **`parse-dont-validate` + `newtype`** — parse external input directly into domain types; the parsed type *is* the proof of validity.
- **`repository` + `aggregate`** — persistence-ignorant domain model with clean consistency boundaries. Add `unit-of-work` when the operation must be atomic and durable.
- **`bootstrap-and-service`** — the outer shell that wires `repository`, `unit-of-work`, and protocol adapters together at startup. Apply last.
- **`type-conversion` + `newtype`** — newtype constructors *are* the canonical conversions; total constructors implement `From`, partial constructors implement `TryFrom`/`parse`, and call sites stop reaching into `.0` to rewrap.
- **`configuring-logging` + `emitting-logs`** — the two halves of structured logging. Configuration sets the envelope and the exporter once; emission attaches the per-event fields under the semantic conventions the configuration enforces. Run them together; one without the other produces structured logs that aren't queryable, or queryable logs that aren't centralized.

Do not apply all patterns upfront. Start with the one that addresses the immediate design pressure.
