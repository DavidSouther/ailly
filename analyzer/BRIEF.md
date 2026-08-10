# Ailly Analyzer — Product Brief

## Problem

Agent harnesses like Claude Code and Codex write session transcripts to disk (`~/.claude/sessions`, `~/.codex/sessions`, and similar directories for other tools). While these are great to come back to sessions later, there is no tool to review, analyze, or examine their individual and aggregate contents.

1. **"Where did the agent take a wrong turn? Where did the signal come from?"** — long linear transcripts bury the tool call, subagent spawn, or file read that introduced a bad fact or a bad plan. Finding it today means scrolling a raw log.
2. **"What did this actually cost, and where did the budget go?"** — token spend across an orchestrator and any subagents it spawned is invisible without manually reconstructing it from scattered call records.

This app is a standalone desktop tool that opens existing local session files after the fact — it does not run agents, has no live connection to a harness, and does not need one.

## Scope boundary

This brief covers investigation of **completed, existing sessions on disk**. It explicitly excludes:
- Live/streaming session views (the harness is not running while this app is open)
- Artifact review at the top-level conversation view (see below — deliberately omitted from the "traditional" layout, per instruction)
- Any action that mutates or resumes a session

## User journeys

### 1. Find a session

A user opens the app for the first time, or the hundredth. They need to get from "nothing loaded" to "looking at a specific session" fast, without knowing file paths.

- Discover which local harnesses have session data available (Claude Code, Codex, etc) without manually pointing at a folder.
- Browse sessions as a list: recency, working directory / project, a short label (first user prompt, or session name if the harness records one), rough size (call count, duration, token total).
- Search or filter that list — by project/directory, by date range, by harness/source, and by rough size ("show me the big ones").
- Distinguish sessions that are still informative from ones that are noise (e.g., a session that was two messages long vs. one that ran for two hours).
- Open a session and land somewhere useful immediately — not a blank state requiring further navigation.

### 2. Investigate a single session's tool calls

After a session has completed, review shows unexpected phrasing or citations. These indicate the session "took a wrong turn" early on, identifying an incorrect fact or inappropriate comment that got integrated into the context window. The user would like to identify where this came up, by reviewing the specific tool calls and responses.

- See a top-level summary immediately on opening a session: total calls, distinct files touched, duration, subagent spawn count — an at a glance, was this session big or small overview.
- See the tool category split (exec / edit / read / other) as a proportion, to sense the shape of what the session did before going into specific tool call details.
- See calls broken down by individual tool, ranked by frequency, each showing its share of total calls.
- Drill from a tool into its individual calls, each showing which agent (orchestrator or which subagent) made it, what the call's target was (a file, a command, a URL, a subagent), and enough of the call's detail to judge relevance without leaving this view.
- Follow the sketch's "Sources" grouping specifically: Bash, Read, and WebFetch rolled into one umbrella (every way a fact enters context from outside the conversation), with a further breakdown by source kind (shell / file / web) and, for file reads, the literal file paths consulted.
- Flag or highlight the tool calls that are "especially pertinent" — file reads, skill invocations, and API/web calls — as the calls most likely to be where an external fact entered context, distinct from calls that only rearrange things already in context (edits, todo updates).
- See which files were touched, ranked by touch count, with which tools touched each file — to answer "what did the agent think was central to this task" by volume of attention.
- Given a suspected wrong turn (a bad edit, a bad conclusion), walk backward from it to find the specific tool call(s) that introduced the fact behind it — this is the investigation the whole summary view exists to support, so it should be possible to go from "this file's final content looks wrong" to "this is the read/fetch/subagent-report that fed it" in a few clicks, not a manual scroll-and-search.
- Handle very large sessions (hundreds of calls, dozens of files) without the summary view degrading into an unusable wall. This might be capping what's shown by default with a visible "+N more" rather than silently truncating, or other techniques TBD.

### 3. Investigate a collection of sessions

Sometimes the question isn't about one session but a pattern across several — e.g., "does this agent always misread this file," or "how do my sessions on this project compare."

- Select multiple sessions (by project, by date range, by manual multi-select) and see an aggregated rollup: combined tool-call totals, combined files-touched, combined token spend — the same shape as the single-session summary, scaled up.
- See per-session breakdown within that collection, not just a blended total — so an outlier session doesn't get lost in an average.
- Compare sessions side by side on the dimensions that matter for the "wrong turn" investigation: which tools dominated, which files got the most attention, how many subagents were spawned, how long each ran.
- Identify recurring patterns across a collection — the same file touched across many sessions, the same subagent type spawned repeatedly, the same kind of tool call showing up as "especially pertinent" every time — as a way of noticing systemic issues rather than one-off mistakes.

### 4. Review token usage

Token spend is a named journey distinct from tool-call review — a user wants to answer "where did the budget go" even when the tool-call shape looks fine.

- See total token usage for a session (and, in the collection view, across a set of sessions) broken into meaningful categories — at minimum, distinguishing the orchestrator's own usage from usage attributable to subagents, and distinguishing usage that entered via cached/reused context from freshly-generated usage, to whatever granularity the underlying session logs actually record.
- See token usage attributed per subagent spawn, so a user can identify which delegated task was expensive relative to the value it returned.
- See token usage trend over the course of a single session — whether spend was front-loaded (a big upfront research pass), back-loaded (an expensive final synthesis), or spread evenly — to understand where in the timeline the cost accrued.
- Correlate a token-usage spike with the tool-call investigation above — jump from "this subagent spawn was expensive" to "here's what that subagent actually did" without re-navigating from scratch.
- In the collection view, compare token spend across sessions on the same or similar tasks, to notice when a session cost far more than a comparable one did.

### 5. Review subagents

Subagents are a distinct, currently-opaque layer. Claude still shows them inline, difficult to impossible to see which output came from which agent. This is its own journey, not a footnote to tool-call review.

- See, for a session, the full list of subagent spawns: what type each was, what task/prompt it was given, how long it ran, and a short signal of its outcome (completed, errored, skipped).
- Open a specific subagent spawn and see its own tool calls, files touched, and token usage — treating a subagent's activity as its own investigable mini-session nested inside the parent, rather than flattened into the parent's undifferentiated call list.
- See, at the parent level, which subagent's work fed into which subsequent orchestrator decision or tool call — closing the loop the design doc names directly: knowing which output came from which agent.
- In the collection view, see subagent usage patterns across sessions — which subagent types get spawned most often, which ones tend to run long or expensive, which ones tend to precede a wrong turn.

### 6. Read the complete conversation in a traditional layout

A single, explicit view of the whole session as a conversation — the fallback a user reaches for when the summary/drill-down views raise a question that's easier to answer by just reading the transcript in order.

- Render the session top to bottom as a conversation: user turns, assistant text, tool calls, and subagent spawns interleaved in the order they actually happened — this is the one view where tool calls and subagent activity sit together in natural reading order rather than regrouped by tool or by file.
- Represent a subagent spawn inline at the point it occurred, expandable to see that subagent's own turns and tool calls without leaving the parent conversation's flow.
- Represent each tool call inline at the point it occurred, collapsed by default (consistent with the "progressive disclosure, not in flow" principle from the design doc) but expandable to see its parameters and result.
- Explicitly do not surface artifact content (files created/edited, generated documents, rendered output) in this view — a user who wants to review what was produced, as opposed to how the conversation and its tool calls unfolded, is directed elsewhere. This is a deliberate omission, not an oversight: keeping this view to conversation + tool calls + subagents is what makes it read as the "traditional" transcript rather than a second copy of the artifact review experience.
- Let a user jump from this linear view into the summary/drill-down view (or vice versa) anchored on the same point in the session, so the two views feel like two lenses on one session rather than two disconnected tools.

## Open questions for a follow-up conversation

- How many distinct harness session formats need day-one support (Claude Code and Codex named explicitly; others "etc.") — this affects how much of the "discover sessions" journey can be harness-specific versus generic.
- Whether "collection" selection is expected to be ad hoc (manual multi-select each time) or something a user names and returns to (a saved set) — the brief above assumes ad hoc but ranking/comparison across a named, recurring collection is a plausible extension.
- Whether token-usage granularity in the source logs actually supports a cache-vs-fresh breakdown, or whether that has to degrade to a coarser total depending on what each harness's session format records.
