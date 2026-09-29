---
name: configuring-logging
description: Use when bootstrapping a service's logging pipeline — selecting a subscriber/registry, layering formatter/filter/enricher/exporter, attaching resource attributes, wiring W3C trace propagators, choosing head- or tail-based sampling, configuring redaction as defense-in-depth, and arranging graceful shutdown flush. Applies once at process start, never inside library code.
---

# Configuring Logging

## Overview

A logging pipeline is composed once, at startup, as a stack of layers over a single subscriber registry. Each layer owns one concern: format, filter, enrichment, export. The order is fixed; later code only emits records into it. Bootstrap is also where the W3C `traceparent` propagator is installed, so every call site inherits the active trace context without touching headers.

Two deployment shapes drive every decision below — a long-running **service** (one process serves many requests) and a short-lived **CLI** (one invocation per outcome). The five-layer skeleton is the same; the defaults at every layer differ. Choose the shape first, then walk down the stack.

## When to Use

- Standing up a new service or CLI and the first log line has nowhere to go yet.
- Adding OpenTelemetry to an existing project that uses `tracing_subscriber::fmt().init()`, `console.log`, `logging.basicConfig()`, or similar.
- Reviewing an exporter or a sampler change.
- Wiring `traceparent` propagation, resource attributes, redaction, or graceful shutdown for the first time.

**When NOT to use:** inside a library crate, request handler, or any code reachable more than once. Libraries emit; they do not configure. A second `init()` either fails loudly or silently shadows the first; both produce records that miss the resource attributes and the propagator the application installed.

## Core Pattern

A subscriber registry is built once in `main` and assembled as five layers, in order:

```
Registry → Format → Filter → Enrich → Export
```

- **Registry** — the single global subscriber. Built once. `tracing_subscriber::registry()`.
- **Format** — how records render. JSON for service shape (per 12-Factor); pretty-on-TTY plus a `--log-format` override for CLI shape.
- **Filter** — which records pass. `EnvFilter` reading `RUST_LOG` for services; `clap-verbosity-flag` for CLIs (default = ERROR; `-v`/`-vv`/`-vvv`/`-vvvv` raise it; `-q` silences). Suppress noisy dependencies with directives like `hyper=warn,h2=warn,reqwest=warn`.
- **Enrich** — what every record carries: resource attributes (`service.*` for services, `process.*` for CLIs), the active span's `TraceId`/`SpanId`, and the W3C `traceparent` propagator that lets those IDs flow across processes.
- **Export** — where records go. Service default: JSON to stdout, OTLP to a collector. CLI default: human-readable to stderr (per clig.dev §Output); structured records to `$XDG_STATE_HOME/<app>/log` per XDG Base Directory Specification 0.8; OTLP only under explicit `--telemetry` opt-in. **Never** JSON-to-stdout by default in a CLI; stdout is reserved for program output.

A complete bootstrap function takes about fifteen lines. For full, runnable examples, see [`references/rust.md`](references/rust.md), [`references/typescript.md`](references/typescript.md), and [`references/python.md`](references/python.md).

## Quick Reference

| Concern | Service shape | CLI shape |
|---|---|---|
| Default sink | JSON to stdout (12-Factor); OTLP to collector | Human-readable to stderr (clig.dev §Output); file under `$XDG_STATE_HOME/<app>/log`; OTLP only under `--telemetry` |
| Default surfaced level | INFO | ERROR (per `clap-verbosity-flag` and clig.dev) |
| Verbosity control | `RUST_LOG` / env-driven `EnvFilter` | `-q`/`-v`/`-vv`/`-vvv`/`-vvvv` ladder |
| Resource keys (Enrich) | `service.name`, `service.version`, `service.instance.id`, `deployment.environment`, `host.name` | `process.executable.name`, `process.executable.path`, `process.command_args`, `process.pid`, `process.runtime.name`, `process.runtime.version` |
| `traceparent` propagator | Extract on inbound, inject on outbound | Maybe extract from env on inbound; inject on outbound |
| Sampling | Head and tail; coordinate log + trace decisions | Verbosity ladder (no sampler) |
| Redaction | Allowlist preferred (deny by default); blocklist regex as backstop | Scrub at `--bug-report` / telemetry-uploader boundary |
| Format selection | JSON, fixed | JSON if `--log-format=json` or stderr non-TTY; pretty otherwise; honor `NO_COLOR`, `CLICOLOR` |
| Shutdown | SIGTERM handler → exporter `shutdown()` with hard timeout | `defer`/finally flush at `main` exit + SIGINT (Ctrl-C) handler with hard timeout |
| Bootstrap cost | Eager (amortized across requests) | Lazy (paid every run); construct exporters lazily |

## Pipeline Concerns

Each subsection corresponds to one column of the table above. The Quick Reference is the lookup; this section is the rationale.

### Startup cost

Services pay the bootstrap cost once and amortize it across the process lifetime; eager construction is the right default. CLIs pay it on every invocation, so the default pipeline is stderr only and exporters are constructed lazily. Network-bound exporters (OTLP) are gated behind an explicit `--telemetry` flag; no network calls happen at bootstrap unless the user opts in.

### Resource attributes

Resource attributes attach once, at the resource layer, and never per record. The keys differ by deployment shape:

- **Service shape.** `service.name`, `service.version`, `service.instance.id`, `deployment.environment`, `host.name`. Together they let the backend split a metric by version or by environment without changing call-site code.
- **CLI shape.** `process.executable.name`, `process.executable.path`, `process.command_args`, `process.pid`, `process.runtime.name`, `process.runtime.version` from the OTel `process` semantic conventions. `service.name` may still be set to the binary name as a degenerate convention.

CLIs typically inject `traceparent` on outbound calls and have nothing to extract from on inbound; configure the propagator for outbound only.

### Sampling

Sampling is a service-shape concern. Head sampling decides at span start; tail sampling decides at span end (after error or latency is known); hybrid combines them. The choice is a trade-off between cost (head is cheap, tail buffers spans), completeness (tail keeps interesting traces, head may drop them), and operational complexity. Whichever is chosen, the log sampler and the trace sampler must reach coordinated decisions or the wide-event recovery property breaks.

CLIs do not sample. Verbosity replaces sampling: `-q`/`-v`/`-vv`/`-vvv`/`-vvvv` selects which records the EnvFilter passes through. This is the entire surface a user has for tuning a CLI's output volume.

### Redaction layer

The redaction layer is **defense-in-depth**, not the primary control. The primary control is upstream typing via `patterns:parse-dont-validate`: secrets arrive at any log call site already wrapped in `Secret<T>`, whose `Debug` impl renders `[REDACTED]`. The redaction layer exists for the leak that typing cannot prevent — a free-text message body that happens to include a credit-card number, or a third-party library that logs raw payloads. Configure it as an **allowlist** (deny everything except known fields) in services where the field set is stable. Add a blocklist regex as a backstop, never as the primary policy.

In CLI shape the analogous boundary is the `--bug-report` bundling step or a telemetry uploader; scrub at the share/upload edge rather than installing a collector processor. The `Secret<T>` parse-boundary discipline is unchanged in either shape.

### TTY and format selection

Format selection in CLI shape is driven by two inputs: the `--log-format` flag and whether stderr is a TTY. JSON wins if the flag asks for it or stderr is non-TTY (piped, redirected, captured by an orchestrator); otherwise pretty. Honor `NO_COLOR` (no-color.org) and `CLICOLOR` / `CLICOLOR_FORCE` for color. When the CLI uses a progress UI, coordinate with logs via `tracing-indicatif`'s writer wrappers or `ProgressBar::suspend()` so log lines do not corrupt the bar. Service shape is simpler: JSON, always.

### Graceful shutdown

Both shapes need a flush before exit, and both need a hard timeout so a stuck exporter cannot hang the process (clig.dev §Robustness).

- **Service shape.** Register a SIGTERM handler that calls `shutdown()` on the OTLP exporter with a hard timeout (typically a few seconds). On orchestrators that escalate to SIGKILL, the timeout is the contract.
- **CLI shape.** A `defer`/finally-shaped flush at `main` exit covers the normal path, and a SIGINT (Ctrl-C) handler covers the interruption path: flush, then re-raise. Apply the same hard timeout. A CLI that hangs on shutdown is worse than a CLI that loses the last batch of telemetry.

## Common Mistakes

- **Re-initializing from a library.** A library that calls `init()` either panics or silently shadows the application's pipeline. Libraries emit through the global subscriber; only the application configures it.
- **Format-by-environment-name.** Branching the format on `if env == "production"` is brittle and invisible to operators. Detect TTY and expose a `--log-format` flag instead.
- **Three layers instead of five.** A registry with only Format and Filter omits Enrich and Export. The records emit, but they have no resource attributes, no `traceparent`, and no path to a collector. The wide-event recovery property fails immediately.
- **Resource attributes attached per record.** Resource attributes belong at the resource, not in every macro call. Per-record attachment is allocation, drift, and a sign that the enrich layer was skipped.
- **Treating OTel as a future upgrade.** "Add OpenTelemetry later without restructuring" leaves every record emitted before the upgrade orphaned. Wire the OTLP exporter, the resource attributes, and the propagator at first commit; the future-you that needs them does not exist yet.
- **Missing flush on shutdown.** A pipeline that batches records and never flushes loses the last interval. Register the handler, set the timeout, and test it by sending a real SIGTERM.
- **Per-call header propagation.** Reading and writing `traceparent` at every HTTP call point is the symptom; the cure is installing the W3C propagator once at bootstrap so the framework's middleware does it for you.
- **Blocklist regex as the primary PII control.** A regex catches what it knows about. Allowlist what you intend to log; treat the blocklist as a backstop. The actual control is upstream, in the parser that wraps secrets at the boundary.
- **JSON-to-stdout in a CLI.** Stdout belongs to the program's primary output; piping to another tool must continue to work. CLI logs go to stderr by default, with structured records optionally landing in `$XDG_STATE_HOME/<app>/log`.

## Composes With

- **`patterns:bootstrap-and-service`** — bootstrap is where `init_logging()` is called; the configuration skill defines the layered shape, the bootstrap-and-service skill defines where it sits in the wiring.
- **`patterns:parse-dont-validate`** — the upstream control for sensitive data. The redaction layer here is a backstop; secrets are already wrapped by the time any log call site runs.
- **`patterns:emitting-logs`** — the call-site partner. Configuration sets the envelope, the exporter, and the propagator; emission attaches the per-event fields the configuration enforces. Run them together.
