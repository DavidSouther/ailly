---
name: emitting-logs
description: Use when writing log call sites in domain or application code — choosing a severity, attaching structured fields with semantic-convention keys, scoping spans to units of work, recording errors with their full chain at a single boundary, and naming business events. Applies every time code emits a log record.
---

# Emitting Logs

## Overview

A log record is a wide structured event that maps to the OpenTelemetry log data model. Every emit site is constructing one of these events. The call site's job is small and fixed: choose a severity, attach the right fields under the right keys, and name it. Trace context (`TraceId`, `SpanId`, `TraceFlags`) is inherited from the active span. The record destination is managed by the collector configuration.

## When to Use

- Tracing the inputs to a request, command, or unit of work in domain or application code.
- Recording the outcome of a request, command, or unit of work in domain or application code.
- Surfacing an error at the boundary where it escapes a use case.
- Naming a business event the backend will count (`order.placed`, `payment.declined`, `feature_flag.evaluated`).
- During review, code that uses `println!`, `console.log`, `print()`, or `info!("…{}…", x)` interpolation.

**When NOT to use:** to set up the pipeline, attach resource attributes, install propagators, or configure exporters. Those belong in `patterns:configuring-logging`. A log call site never names the logging destination.

## Core Pattern

Structured fields, never string interpolation. Field names come from the OpenTelemetry semantic conventions; the message body is a stable human-readable string and never carries variable values. When the event represents a business occurrence, give it an `EventName` so the backend can count it without parsing the body.

```rust
// Span scope is the unit of work; events inside inherit TraceId and SpanId.
#[tracing::instrument(skip_all, fields(http.route = "/orders", user.id = %req.user_id))]
async fn create_order(req: CreateOrderReq) -> Result<OrderId, PlaceOrderError> {
    let order_id = orders::place(&req).await?;

    // Success: structured fields under semantic-convention keys; the message
    // is stable; the EventName is the business name in ubiquitous language.
    tracing::info!(
        name: "order.placed",
        http.request.method = "POST",
        http.response.status_code = 201,
        order.id = %order_id,
        "order placed"
    );
    Ok(order_id)
}

// Failure: log the full chain once, here, at the boundary. Render via
// `error = &err as &dyn std::error::Error` and let the subscriber walk
// `Error::source()`. Attach the error type explicitly under the OTel
// exception conventions; do not log-and-rethrow.
match create_order(req).await {
    Err(err) => tracing::error!(
        name: "order.placement_failed",
        error.type = std::any::type_name_of_val(&err),
        exception.type = std::any::type_name_of_val(&err),
        error = &err as &dyn std::error::Error,
        "order placement failed"
    ),
    _ => {}
}
```

The user identifier is rendered via its newtype's `Display` impl (`patterns:newtype`); a `Secret<T>` would render `[REDACTED]` automatically because the type carries the safety property — never `expose_secret()` on the way to a log call. For complete examples, see [`references/rust.md`](references/rust.md), [`references/typescript.md`](references/typescript.md), and [`references/python.md`](references/python.md).

## Quick Reference

### Severity

| `SeverityNumber` | Name  | Policy | CLI verbosity |
|---|---|---|---|
| 1–4   | TRACE | Per-iteration loops, fine-grained internal state. Off by default everywhere. | `-vvvv` |
| 5–8   | DEBUG | Decisions a developer needs while diagnosing. Off in production by default. | `-vvv` |
| 9–12  | INFO  | One per unit of work: request handled, command completed, job finished. | `-vv` |
| 13–16 | WARN  | Recoverable surprise: retry hit, fallback used, deprecated path. | `-v` |
| 17–20 | ERROR | A use case failed. The exit code reports; the log explains. | default |
| 21–24 | FATAL | Process cannot continue. Followed by exit. | default |

CLI defaults follow `clap-verbosity-flag` and clig.dev §Output: ERROR is the surfaced floor; `-q` silences. A two-level policy (INFO + ERROR only) is also fine for services that prefer span attributes over WARN/DEBUG records. This should be chosen for the project and recorded in the project's DEVELOPER.md or similar documentation.

### Field selection (OpenTelemetry semantic conventions)

| Operation | Keys |
|---|---|
| HTTP server / client | `http.request.method`, `http.route`, `http.response.status_code`, `url.path`, `url.scheme`, `network.peer.address` |
| Database | `db.system.name`, `db.namespace`, `db.collection.name`, `db.operation.name`, `db.query.summary` |
| RPC | `rpc.system`, `rpc.service`, `rpc.method`, `rpc.grpc.status_code` |
| Messaging | `messaging.system`, `messaging.destination.name`, `messaging.operation.name` |
| Exception | `exception.type`, `exception.message`, `exception.stacktrace` |
| Errors (general) | `error.type` |
| Identity (your domain) | `user.id`, `session.id`, `tenant.id` (or your bounded-context equivalent) |
| Business event | `EventName` (e.g. `order.placed`, `payment.declined`, `auth.denied`) |

Hard-code nothing in the response, the framework, or the domain object can supply. `http.response.status_code` reads from the actual `StatusCode` value; `user.id` reads through a `UserId` newtype's `Display` impl. Drift between log fields and reality starts the moment a call site embeds a literal `201` next to a response that may someday return `202`.

## Emit-Site Practices

### Spans and units of work

Open a span at every unit of work: request entry, RPC call, DB transaction, background-job iteration. Every event emitted inside the span inherits its `TraceId` and `SpanId` automatically. Use `#[instrument(skip_all, fields(...))]` (or the equivalent `with_span` in Pino, `structlog.contextvars` in Python) and pull the right fields onto the span itself, so child events do not need to repeat them.

`#[instrument(skip(...))]` exists to omit *large* arguments for performance, never to suppress secrets. Wrapped secrets are already safe by type — if reaching for `skip` near a `Secret<T>` feels load-bearing, the upstream parse boundary is what is wrong, not the log line.

### Error chains

Log the full chain once, at the boundary where the error escapes the use case. The subscriber walks `Error::source()` (Rust), `cause` (TypeScript `Error`, `cause:` option), or `__cause__` (Python `raise X from e`) when the structured `error` field is passed in. Attach `error.type` and `exception.type` explicitly so the backend can group failures without parsing the message.

Do not log-and-rethrow. Each emit duplicates the chain; the second copy is grep-noise and the dashboards count one failure as two. If a caller needs context, attach it via the error type's `from`/`context` constructor, not via a second log call.

### Business events

A business event carries a stable name in the ubiquitous language: `order.placed`, `payment.declined`, `feature_flag.evaluated`, `auth.denied`. The OpenTelemetry `EventName` field (Rust `tracing` macros: the `name:` directive; Pino: `event_name` mixin; structlog: a key in the bound context) is what the backend pivots on to derive metrics. The body of the message is for humans; the `EventName` is for the count, the rate, and the SLO.

### Hot loops

Never emit per iteration. Aggregate to one summary log per batch with the per-item count, success/failure breakdown, and total duration as fields. Pair the per-batch summary with a per-item progress UI when the user is watching: `indicatif` or `tracing-indicatif` on the CLI side, a metric counter on the service side.

### Severity policy

Pick a policy and write it down. Two policies are common. The full ladder of TRACE/DEBUG/INFO/WARN/ERROR/FATAL is useful when WARN is operationally meaningful. A two-level version uses INFO and ERROR, and is useful when WARN is just "we don't want to think about it". Reviewing severities is easier when "level by mood" is not on the table.

## Common Mistakes

- **String interpolation in the message body.** `info!("handled {} for {}", req.path, user_id)` flattens the structured event into a free-text line. Move the values to fields under semantic-convention keys; keep the body stable.
- **Ad-hoc field names.** `method = "POST"` is invented vocabulary; `http.request.method` is the OpenTelemetry semantic convention. The backend knows the latter; metrics derive from it. Use the table above as the lookup.
- **Hard-coded literals where the value is in scope.** `status = 201` next to a response that *might* return `202` desyncs silently. Read `http.response.status_code` from the actual response value.
- **Logging an unwrapped secret.** `expose_secret()` near a logger is evidence of an upstream parse-boundary failure. Fix it at the boundary; secrets carry their `[REDACTED]` `Debug` impl through to every emit site. See `patterns:parse-dont-validate`.
- **Log-and-rethrow.** A failure logged at the inner layer and again at the outer layer counts twice on the dashboard. Log the chain once at the boundary that owns the use case.
- **Level by mood.** Severity is a policy, not a feeling. Either the full ladder or two-level (INFO + ERROR); write it down.
- **Per-iteration in hot loops.** A million-row loop emitting one log per row is a denial of service against your own backend. Aggregate to one summary; pair with a progress UI.
- **Defaulting CLI output to INFO.** `clap-verbosity-flag` and clig.dev §Output put ERROR at the floor for CLIs. INFO is `-vv`, not the default.
- **Embedding non-deterministic fields in CLI stdout.** Timestamps, trace IDs, and durations turn snapshot tests into flake generators. Honor `SOURCE_DATE_EPOCH` or expose `--no-timestamps` for tests; keep diagnostics on stderr where they belong.
- **Treating `error!` as the failure signal.** A CLI's failure signal is the exit code (per sysexits.h: `EX_USAGE=64`, `EX_DATAERR=65`, `EX_NOINPUT=66`, …). The log is an explaination of the problem, the exit code is the final output.
- **Skipping `EventName` on a business outcome.** Without an `EventName`, the event has a body but no name; the backend cannot count it as a thing without parsing strings. Name it in the ubiquitous language.

## Composes With

- **`patterns:configuring-logging`** — the bootstrap partner. Configuration sets the envelope, the exporter, and the propagator; emission attaches the per-event fields the configuration enforces. One without the other produces structured logs that aren't queryable, or queryable logs that aren't centralized.
- **`patterns:errors-typed-untyped`** — typed errors render via `error.type` and `exception.*`. The chain is the API; the emit site is where the chain becomes a record.
- **`patterns:newtype`** — newtype `Display` impls produce stable, semantic-convention-shaped attribute values (`UserId` → `user.id`, `OrderId` → `order.id`). Identifiers do not become strings until the moment of emission.
- **`patterns:domain-objects`** — entities and value objects supply the field values; `Secret<T>` arrives wrapped, never unwrapped near a logger.
