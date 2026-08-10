# Development Guide

Ailly Analyzer is a local-first desktop application for investigating completed agent sessions. It reads existing Claude Code, Codex, and future harness transcripts from disk; it does not run agents, connect to live harnesses, resume sessions, or mutate source transcripts.

## Technology

- **Desktop runtime:** Tauri 2
- **Frontend:** React, TypeScript, Vite
- **State and navigation:** URL/query state for shareable investigation locations; a small local store such as Zustand for transient UI state
- **Backend:** Rust commands exposed through Tauri
- **Index:** SQLite with FTS5, treated as a rebuildable local cache
- **Visualization:** Recharts for ordinary charts; D3 only where a custom provenance graph is necessary
- **Large collections:** TanStack Virtual
- **Tooling:** Vitest, Rust unit tests, Playwright, Biome, TypeScript, Storybook

## Architecture

Harness-specific loaders parse source files into a normalized event model. The Rust backend owns filesystem discovery, parsing, indexing, and bounded queries. The React frontend owns presentation, filtering, drill-down, and navigation between the summary and transcript lenses.

```text
source session files
        |
        v
harness adapters -> normalized events -> SQLite index
                                             |
                                             v
                                      React investigation UI
```

Original session files are authoritative and read-only. SQLite is an indexed cache that can be discarded and rebuilt when a harness format changes. Every indexed event retains its source location so the UI can explain where evidence came from.

The normalized model preserves session identity, agent identity, parent/child relationships, ordering, timestamps, tool category, raw source location, files, URLs, commands, and token metadata when available. Missing fields remain explicitly unknown rather than being inferred as facts.

## Product boundaries

The product investigates completed sessions on disk. Live or streaming views, artifact review in the traditional conversation view, session mutation, and session resumption are outside the product boundary.

## Local development

Install the Node dependencies, then run the frontend during early UI work with:

```sh
npm install
npm run dev
```

Once the Tauri shell is present, use the repository's Tauri development command for the desktop app. The standard checks are:

```sh
npm run check
npm test
npm run build
```

Rust code must also pass its package tests and formatter checks. Keep parsing and aggregation logic independently testable without starting the desktop shell.

## Implementation principles

1. Keep harness differences behind adapters.
2. Preserve raw source coordinates for every normalized event.
3. Make indexing incremental, interruptible, and safe to repeat.
4. Keep expensive parsing and aggregation off the UI thread.
5. Prefer progressive disclosure for large sessions: summarize first, expand on demand.
6. Make unsupported token or provenance detail visible as unavailable rather than fabricating precision.
7. Keep the source transcripts untouched.

## Program Management

The active task list is `.ailly/TASKS.md`. There is no external tracker configured for this project; the task list is the source of truth for the build order.
