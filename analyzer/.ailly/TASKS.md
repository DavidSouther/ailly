# Ailly Analyzer Build Order

Build the product vertically in this order. Each task should leave a usable, tested seam for the next one. Keep the app local-first and read-only throughout.

Completed work (Tauri shell, harness loader, SQLite index, Journeys 1–2 and 4–6, captured tool output, tool-call edits, tool-call icons, rescan/harness chips, session timestamps, system light/dark, shell-derived file access) is omitted; only remaining work is listed.

## Cross-lens jump to transcript

Conversation already exposes `event-*` anchors and Tokens already lands on a response row. Treat **jump from any tool-summary detail to that event in Conversation** as one feature, not a pile of one-off links.

- From Summary: a tool-frequency row, an individual call, a Sources/file-touch row, and a file in File access. A File access row may be an ambiguous shell fragment rather than a path, so the jump target is the call that produced the evidence.
- From Tokens: a spend moment, spawn, or chart point.
- From Subagents: a spawn or a call inside a spawn.
- Landing reuses `useLandingTarget` / `event-*`: switch to Conversation, focus and highlight the row, do not replay on a later tab click.
- File-first: opening a path shows the calls that touched it, and each of those calls jumps to the same transcript point.

Replaces the deferred "Wire summary / drill-down views into the `event-*` anchors" and "File-first route from Summary File access" bullets.

## Full-app search (own design)

Session-list free text and `harness:` chips are not product search. Searching is **full app** and needs its own design pass before build (scope, what is indexed, result shape, how a hit opens a session and lands on an event).

In scope to decide during that design, not to pre-solve here:

- FTS over event text, captured tool output, and tool-call payloads (the deferred FTS bullets from Journeys 1, captured output, and tool-call edits belong here).
- Hits that use the same transcript jump as summary details.
- Collection / project scope vs one session; filters beyond `harness:`.

## Journey 3: Investigate a collection of sessions

- Add ad hoc multi-selection by project, date range, and manual selection.
- Show aggregate and per-session tool, file, duration, agent, and token rollups.
- Add side-by-side comparison for the dimensions relevant to wrong-turn investigation.
- Surface recurring files, subagent types, and pertinent call patterns.
- Preserve drill-down links from collection patterns to individual sessions.

## Deferred from shell-derived file access (2026-08-20-B-shell-io)

The `shell-access` crate reads recorded commands and the Summary Filesystem list
labels every row with a target, an operation, and its provenance. What that
design left open:

- **Operand-table maintenance has no path.** `shell-access/src/table.rs` was
  built by measuring one static corpus of local transcripts. A utility missing
  from it contributes no rows and lands in a visible unattributed count, which
  is a one-time display rather than a way to notice the table going stale as new
  harnesses, CLIs, and shell conventions appear. Decide how the table is
  revisited and how a miss is surfaced to a maintainer, not only to a reviewer.
- **A mislabelled target is quieter than a missing row.** A utility outside the
  table contributes nothing and is counted; a directory-taking utility the table
  does not know (`rsync`, `zip -r`, `git -C`, `stat`) has its operand reported as
  a *file*, which is a wrong claim with nothing counting it. Decide whether the
  table's default should be "unknown kind" rather than "file".
- **`find`'s expression is dropped whole, including its destructive half.**
  `find . -name '*.log' -delete` and `find . -exec rm {} \;` report one directory
  read of the root and no delete at all, so a destructive command reads as a
  read. The design's own survey named `find -exec` peeling and `find -delete` as
  a delete; neither is implemented. Bounded: the expression already stops the
  operand walk, so this is about reading `-delete` and `-exec` out of it.
- **Rows are ranked by nothing.** The list distinguishes common from rare not at
  all, so a single write to `.env`, `.ssh/`, a git hook, or a credentials file
  reads the same as the hundredth `cat`. Decide whether consequence should
  outrank frequency before adding any ordering or emphasis.
- **Filesystem filter state is not in the URL.** The text filter and the
  three-state chips live in component state, so a narrowed view is neither
  shareable nor durable across a reload — the same gap the session-list search
  chips have, and worth solving once for both. (`FilesystemList` is now keyed by
  session so a filter does not leak onto the next session; durability across a
  reload is what remains.)
- **A declared utility with an incomplete flag list fabricates rows, and
  nothing counts it.** When `find_flag` misses, the walk keeps reading and the
  flag's *value* lands in the positional list — so a missing value-flag
  declaration invents a file row and also shifts which operand
  `ReadThenWriteLast` calls the destination. `install` was fixed by declaring
  its mode and ownership flags; the class is not. This is worse than the
  unattributed under-report, because the row looks like every other row. Decide
  whether an undeclared flag on a *declared* utility should stop the operand
  walk (safe, under-reports) rather than be skipped (fabricates).
- **A redirect anywhere but the end of a command loses every operand after
  it.** `operands()` filters `file_redirect` out of a command's children, and
  tree-sitter-bash's `file_redirect` absorbs the words that follow it, so
  `cat 2>/dev/null a.txt` reports the write and drops `a.txt`, and
  `cat 2>&1 a.txt` reports nothing. The trailing form is correct and is the only
  form the corpus covers. Bounded: `redirect()` already reaches into the node
  for `destination`, so the remaining children are recoverable as operands.
- **`FileAccess.scripting` never reaches a reader.** The crate computes it and
  the corpus asserts it on every case, but `shell/src/index/files.rs` does not
  map it onto `FileReference`, so "this write is a literal redirect" and "this
  write came from around an inline interpreter" are indistinguishable in the UI.
  Either surface it or stop computing it. Relatedly, `attribute`'s `scripting`
  parameter is hardcoded `false` at its only call site and can only ever be
  false, because `INTERPRETER` is the only scripting utility and it attributes
  no positionals.
- **Shells other than POSIX.** `ShellLanguage` names the seam and
  `AutoDetect` refuses what it cannot read, so fish, zsh-specific, and PowerShell
  rules can arrive without changing the record shape. Nothing implements them.
- `client/src/ui/summary/rollup.ts` **dead code** — `stats.sources` is unread by
  the UI, and `SourceCall` shaping is also reachable through `toolsByFrequency`.
  Removing it means rewriting the eight unit tests that use it as their access
  point for call/result pairing, working directory, and chunked output; that is a
  change about those tests rather than about any feature.
- `shell-access/tests/posix.rs` **duplicate coverage** — several of its
  tests pin shapes the corpus also pins. Two copies, not three, and each reads
  differently: the unit test states the contract in place, the corpus test binds
  the checked-in data. Left as an aroma.
- **Comment trims a review named and cleanup did not take.** The false comments
  were fixed; these are judgement calls left for a reader with taste: roadmap
  phrasing ("does not yet recover") in `CATEGORY_TABLE` and in `files.rs`'s test
  DocBlock, the component inventory in `stats.tsx`'s module doc, the
  `CATEGORY_TABLE` cross-language pointer in `table.rs`, the "early probe"
  history in `tests/corpus.rs`, and three names (`AccessFilter`, `toggles`,
  `chips`) for one concept in `stats.tsx`.
- **Rebuilding an absolute path from a recorded working directory stays out.**
  `cwd` is carried as call context and never joined onto an operand. Reversing
  that would report a fact no command made; it needs a product-boundary decision,
  not an implementation.

## Deferred from Journey 4 (2026-08-14-A-review-token-usage)

- Summary Tokens card content-category breakdown (system prompt / user
  messages / thinking / tool calls / responses): no harness records a
  per-content-block token count, so only Codex/Pi's reasoning tokens are
  honestly available as a subset of output; the rest would need a
  characters-not-tokens axis. Decide the token-vs-character axis with the user
  before building.
- Refreshing a frozen estimate: an estimate is priced once at index time and
  kept forever; there is no user-initiated recompute if the rate catalog is
  ever refreshed in place.
- Codex `model_context_window` and the "% of context window" reading only
  Codex could support (Claude/Pi have no recorded denominator).
- A signal of what a delegation *returned*, beyond its recorded outcome, so
  cost can be judged against value.
- Ranking spawns and responses in separate lists rather than one, if
  comparing a cumulative subtree against a single response delta proves
  misleading in use.
- A dual-axis chart showing per-response and cumulative spend at once,
  instead of the control that switches between them.
- The `iterations[]` array, the `cache_creation` TTL split, and
  `reasoning_output_tokens` sub-breakdowns.

## Deferred from Journey 5 (2026-08-12-D-review-subagents)

- `loader::tool_result` still reads only `is_error`, so Pi's camelCase `isError`
  is dropped from `ToolResult`. Pi's *spawn* outcome reads the flag directly in
  `pi.rs`; the general fix belongs with a tool-result pass.
- Claude's `agent-<agentId>.meta.json` sidecar, which carries the spawning
  `toolUseId` and `spawnDepth` — a second, child-side linkage vector this does
  not need.
- Sidechain child sessions still appear unlabelled in the session list
  (244 locally); that is Journey 1 presentation.
- Codex `session_meta.source.subagent.thread_spawn` as a child-side
  back-reference: the filename match is exact and verified, so this is only
  worth adding if a rollout is ever renamed.

## Deferred from Task 3

- Index-time `session_rollups` (add only after a measured list/summary query is slow).
- Forward `schema_version` migrations beyond delete-and-rebuild.
- User-chosen index directory; content-hash incremental keys; widen FTS coverage (see Full-app search).
- Extra index unit coverage: interrupt mid-batch; source edit then re-index; source delete then prune; `Unsupported`/`Malformed` field round-trip.

## Deferred from captured tool output (2026-08-12-C-captured-tool-output)

- Claude's `toolUseResult` detail: separate stderr, `interrupted`, and
  structured patches, which have no Codex or Pi counterpart.
- Exit codes; no harness records one as a field, and reading
  "Process exited with code N" out of Codex's output prose would be inference.
- Captured output in the FTS index: see Full-app search.
- Image and `tool_reference` results (7 of 111 in one sampled Claude session)
  resolve `Unsupported` and read as "recorded in a form this view cannot show".

## Deferred from Journey 1

- List-row token totals and duration (need rollups or summary batching; avoid N+1).
- Date-range and rough-size filters beyond free-text search and `harness:` chips
  (the harness dropdown is gone; chips/`harness:` tokens are the harness filter).
- Session labels when a harness records them.
- Virtualize the session list once real collections are large enough to measure.
- FTS content search over event text: see Full-app search.

## Deferred from the tool-call command/cwd bugfix (2026-08-12-B)

- Recover the command from Codex `custom_tool_call` records: they carry a
  JavaScript snippet (`const r = await tools.exec_command({cmd: "…"})`) rather
  than JSON, so the tool name and raw snippet surface but the command itself
  would be inferred. Locally these are a large slice of Codex activity (~6.5k
  `exec` and ~1.7k `apply_patch` records), so a parser for that snippet shape
  is worth revisiting. Now the largest remaining coverage gap for shell-derived
  file access, and a bounded one: 524 local Codex sessions turn on it, and 6,563
  of 6,591 snippets carry a literal `cmd` string. Worth doing next as a sibling
  of 2026-08-20-B-shell-io.
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
- Wire summary / drill-down views into the `event-*` anchors: see Cross-lens jump to transcript.

## Deferred from tool-call write/edit payloads (2026-08-13-A-tool-call-edits)

- Structured interpretation of edit fields (diff `old_string`/`new_string`,
  type `edits[]`) and any diff dependency.
- Pair payload with applied-vs-failed result state in Conversation; Pi's
  camelCase `isError` still dropped by `loader::tool_result`.
- File-first route from Summary File access: see Cross-lens jump to transcript.
- `NotebookEdit` path promotion (`notebook_path`) and category-table entry.
- FTS coverage of payloads: see Full-app search.

## Deferred from tool call type icons (2026-08-13-B-tool-call-icons)

- Icons on Conversation tool *result* rows (needs call/result pairing in that
  lens first).
- Icons inside `Badge`-rendered surfaces (`CategorySplit`, `FileAccessList`) —
  `Badge` must forward `aria-hidden` first.

## Deferred from rescan + harness chips (2026-08-13-C-rescan-and-harness-chips)

- URL/query persistence of search chip state.
- Additional `key:` filter chips beyond `harness:` (unknown keys stay free-text;
  date/size remain under Journey 1 deferred).

## Working rule

Do not add live streaming, session mutation, session resumption, cloud synchronization, or artifact review to this task list without revisiting the product boundary in `BRIEF.md`. A recorded call parameter shown under that call is not artifact review; reconstructing a file on disk is.
