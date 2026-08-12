# Ailly Analyzer Build Order

Build the product vertically in this order. Each task should leave a usable, tested seam for the next one. Keep the app local-first and read-only throughout.

## 1. Tauri shell — complete (2026-08-10)

- Add the Tauri 2 desktop shell and Rust workspace.
- Wire the React/Vite frontend into development and production builds.
- Establish typed command invocation between React and Rust.
- Add the application window, native file-system permissions, and a minimal empty-state screen.
- Verify the shell launches on the target development platform and the existing frontend checks still pass.
- Mise task runner to one-command start full Tauri app, just Storybook component tool, and run checks & tests.
- CI hooks for checks, build, and deploy. Deploy using YYYY.0M.00INC+SHA calendar versioning.

Deployment publication remains deferred because this project has no configured deployment target or signing-secret policy. Version is the initial "0.1.0"

## 2. Harness loader and event model — complete (2026-08-11)

- Define the normalized session, turn, tool-call, subagent, token, file, and provenance types.
- Implement adapter interfaces that isolate harness-specific formats.
- Add Claude Code, Codex, and Pi discovery and parsing for their documented local session locations, including configured Pi session roots.
- Preserve source coordinates and unknown fields without inventing missing facts.
- Add fixture-based parser tests for Claude Code, Codex, and Pi, including malformed and partial records and Pi's header, message/tool, tree, and unknown-entry cases.

## 3. SQLite index — complete (2026-08-12)

- Add the rebuildable SQLite schema and FTS5 search indexes.
- Index sessions incrementally using source-file identity and modification metadata.
- Store normalized events, relationships, files, token measurements, and provenance references.
- Expose bounded queries for session lists, event pages, summaries, and search.
- Test interruption, repeat indexing, source changes, and unsupported metadata.

## 4. Journey 1: Find a session

- Discover available harnesses and local session roots.
- Build the session list with recency, project, label, size, duration, and token summary.
- Add search and filters for project, date range, harness, and rough size.
- Add a useful default selection and a clear empty state.
- Keep large collections responsive with pagination or virtualization.

## 5. Journey 6: Read the complete conversation

- Render user turns, assistant text, tool calls, and subagent spawns in source order.
- Keep tool calls collapsed by default and expand their parameters/results on demand.
- Represent subagents inline with expandable nested activity.
- Deliberately omit artifact contents from this traditional conversation view.
- Add anchors that can later connect this view to summary and drill-down views.

## 6. Journey 2: Investigate one session's tool calls

- Add the session summary and tool-category/tool-frequency breakdowns.
- Add sources grouping for shell, file, and web inputs.
- Show pertinent calls, touched files, and the tools responsible for each touch.
- Implement backward navigation from a suspect file or output to candidate introducing events.
- Make uncertainty and unsupported provenance explicit.

## 7. Journey 5: Review subagents

- Show every subagent spawn with type, prompt, duration, outcome, and token usage.
- Provide a nested subagent investigation view with its calls and files.
- Link parent spawn records to child events wherever the source format supports it.
- Add a clearly labeled fallback for inferred or unavailable parent-decision linkage.

## 8. Journey 4: Review token usage

- Show session and collection totals split by orchestrator and subagent.
- Represent cached/reused versus fresh usage only when recorded by the source.
- Attribute usage to each subagent spawn and plot usage over session time.
- Link token spikes directly to the relevant calls or subagent activity.

## 9. Journey 3: Investigate a collection of sessions

- Add ad hoc multi-selection by project, date range, and manual selection.
- Show aggregate and per-session tool, file, duration, agent, and token rollups.
- Add side-by-side comparison for the dimensions relevant to wrong-turn investigation.
- Surface recurring files, subagent types, and pertinent call patterns.
- Preserve drill-down links from collection patterns to individual sessions.

## Deferred from Task 3

- Index-time `session_rollups` (add only after a measured list/summary query is slow).
- Forward `schema_version` migrations beyond delete-and-rebuild.
- User-chosen index directory; content-hash incremental keys; widen FTS coverage.
- Polished indexing progress UI (Journey 1 can show a minimal status first).
- Extra index unit coverage: interrupt mid-batch; source edit then re-index; source delete then prune; `Unsupported`/`Malformed` field round-trip.

## Working rule

Do not add live streaming, session mutation, session resumption, cloud synchronization, or artifact review to this task list without revisiting the product boundary in `BRIEF.md`.
