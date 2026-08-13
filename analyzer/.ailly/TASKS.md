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

## 4. Journey 1: Find a session — complete (2026-08-12)

- Discover available harnesses and local session roots.
- Build the session list with recency, project, label, size, duration, and token summary.
- Add search and filters for project, date range, harness, and rough size.
- Add a useful default selection and a clear empty state.
- Keep large collections responsive with pagination or virtualization.

Shipped: auto-discover default roots; streaming non-blocking index refresh with progress;
session list (harness, project, event count, last activity); harness filter; client-side
text filter; empty/error/zero states; Zustand session store. Also fixed Task 3 reopen bug
(`meta.schema_version` TEXT read as i64).

## 5. Journey 6: Read the complete conversation — complete (2026-08-12)

- Render user turns, assistant text, tool calls, and subagent spawns in source order.
- Keep tool calls collapsed by default and expand their parameters/results on demand.
- Represent subagents inline with expandable nested activity.
- Deliberately omit artifact contents from this traditional conversation view.
- Add anchors that can later connect this view to summary and drill-down views.

Shipped: `getEventPage` + `AillyEvent` bindings; conversation pane beside the session
list; ordered render by `EventKind`; collapsed tool calls / expandable subagent
detail; stable `event-*` anchors; independent pane scrolling with document
overscroll locked. Backend `get_event_page` was already sufficient (frontend-only).

## 6. Journey 2: Investigate one session's tool calls — complete (2026-08-12)

- Add the session summary and tool-category/tool-frequency breakdowns.
- Add sources grouping for shell, file, and web inputs.
- Show pertinent calls, touched files, and the tools responsible for each touch.
- Make uncertainty and unsupported provenance explicit.

Shipped: Summary tab beside Conversation; category split and calls-by-tool;
Sources (shell / file access / web) with expandable calls; files touched live
inside File access with start-truncated paths; command and per-call cwd parsing
for Claude/Codex/Pi (`SCHEMA_VERSION` 2, then 3); captured tool output on
shell rows and Conversation `tool_result` rows; cwd shown only when ambiguous
(relative path or different from the session).

## 7. Journey 5: Review subagents — complete (2026-08-12-D-review-subagents)

- Show every subagent spawn with type, prompt, duration, outcome, and token usage.
- Provide a nested subagent investigation view with its calls and files.
- Link parent spawn records to child events wherever the source format supports it.
- Add a clearly labeled fallback for inferred or unavailable parent-decision linkage.

Shipped: `Subagent` grown to every dimension a harness may record and hung off
`Event` (`subagent_json`, `SCHEMA_VERSION` 4, drop and rebuild); Claude `Agent`
/ `Task`, Codex `spawn_agent`, and Pi `ailly_subagent` calls now emit
`EventKind::SubagentSpawn` with the outcome, duration, and tokens their own
transcripts wrote, folded back from the later result record by recorded id
rather than adjacency; `resolve_child_session_id` closes the parent→child link
at read time from the child transcript's path. A Subagents tab lists every
spawn (prompt preview of three lines, full text on expand) and opens a linked
child into the same Summary / Conversation / Subagents lenses as the parent.
Summary's "Include subagent tools" toggle folds descendants into Calls by tool
and the category split together; call details that used to live under Sources
expand from each tool row. Zero spawns read as `0`. Schema open now rebuilds
atomically, so a half-written index heals on the next launch. Subagent tokens
live on the payload only, never on the event's `token_usage`, so Journey 4's
split stays open. The conformance suite asserts a spawn edge exists if and
only if the source named a child.

Deferred:

- `loader::tool_result` still reads only `is_error`, so Pi's camelCase `isError`
  is dropped from `ToolResult`. Pi's *spawn* outcome reads the flag directly in
  `pi.rs`; the general fix belongs with a tool-result pass.
- Claude's `agent-<agentId>.meta.json` sidecar, which carries the spawning
  `toolUseId` and `spawnDepth` — a second, child-side linkage vector this does
  not need.
- Sidechain child sessions still appear unlabelled in the session list
  (244 locally); that is Journey 1 presentation.
- Per-spawn token splits, cache-versus-fresh accounting, and spend over time
  (Journey 4).
- Codex `session_meta.source.subagent.thread_spawn` as a child-side
  back-reference: the filename match is exact and verified, so this is only
  worth adding if a rollout is ever renamed.

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
- Extra index unit coverage: interrupt mid-batch; source edit then re-index; source delete then prune; `Unsupported`/`Malformed` field round-trip.

## Captured tool output — complete (2026-08-12-C-captured-tool-output)

- Model `ToolResult` (call id, output, error flag) and capture it in all three
  adapters; Claude's results are `tool_result` blocks on user records, which the
  adapter previously dropped entirely.
- Persist `tool_result_json` on `events` (`SCHEMA_VERSION` 3, drop and rebuild).
- Pair results to calls by recorded id only, and expand a call in the Summary
  pane or a `tool_result` row in the Conversation pane to read what it captured,
  clamped to 50 lines.

Deferred:

- Claude's `toolUseResult` detail: separate stderr, `interrupted`, and
  structured patches, which have no Codex or Pi counterpart.
- Exit codes; no harness records one as a field, and reading
  "Process exited with code N" out of Codex's output prose would be inference.
- Captured output in the FTS index, which belongs with the search journey.
- Image and `tool_reference` results (7 of 111 in one sampled Claude session)
  resolve `Unsupported` and read as "recorded in a form this view cannot show".

## Deferred from Journey 1

- List-row token totals and duration (need rollups or summary batching; avoid N+1).
- Date-range and rough-size filters beyond the current harness + text controls.
- Session labels when a harness records them.
- Virtualize the session list once real collections are large enough to measure.
- FTS content search over event text (belongs with read/investigate journeys).

## Deferred from the tool-call command/cwd bugfix (2026-08-12-B)

- Recover the command from Codex `custom_tool_call` records: they carry a
  JavaScript snippet (`const r = await tools.exec_command({cmd: "…"})`) rather
  than JSON, so the tool name and raw snippet surface but the command itself
  would be inferred. Locally these are a large slice of Codex activity (~6.5k
  `exec` and ~1.7k `apply_patch` records), so a parser for that snippet shape
  is worth revisiting.
- The argv-array `shell` call shape (`command: ["bash", "-lc", "…"]`) is
  handled defensively but does not appear anywhere in the local corpus, so it
  is unverified against real data.
- Per-call working directory for Pi stays `Absent`: Pi records `cwd` only on
  the session header, and borrowing it would report a fact the call never made.

## Deferred from Journey 6

- Paginate or virtualize the conversation once real sessions exceed the single
  bounded `getEventPage` page (limit 5000).
- Richer subagent nested activity in the Conversation lens. The row now reads
  the delegation's recorded prompt (Journey 5); the calls and files the child
  made live in the Subagents tab rather than inline here.
- Wire summary / drill-down views into the `event-*` anchors.

## Working rule

Do not add live streaming, session mutation, session resumption, cloud synchronization, or artifact review to this task list without revisiting the product boundary in `BRIEF.md`.
