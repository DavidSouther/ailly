# Prior Art: Existing Tools for Agent-Session Review

Reference document, not a feature research draft. Captures the state of existing tools relevant to BRIEF.md's journeys as of August 2026, for consultation whenever design/plan work on those journeys begins.

## Summary

No single existing tool covers BRIEF.md's six journeys. Session discovery (Journey 1) and a traditional linear conversation view (Journey 6) are already well-served by existing tools, which should serve as the UX pattern. Token usage (Journey 4) is partially covered — the Claude Code JSONL format is proven to support cache-vs-fresh and per-subagent splits, but Codex's depth is unconfirmed. The backward "wrong turn" trace (Journey 2), cross-session pattern-finding (Journey 3), and subagent-to-parent-decision linkage (Journey 5) are genuinely greenfield relative to the surveyed ecosystem, and Journey 5 in particular runs into a real, currently-contested constraint in Claude Code's own log format (see Subagent Linkage below).

## Landscape

**Per-harness transcript renderers** (rendering-only, single harness):
- `claude-code-trace`, `claude-code-log`, `claude-code-transcripts`, `claude-devtools` — Claude Code (`~/.claude/projects/*.jsonl`).
- `codex-trace`, `codex-sessions`, `codex-transcript-viewer`, `@nogataka/codex-viewer`, `CodexMonitor`, VS Code "Codex CLI History Viewer" — Codex CLI (`~/.codex/sessions`).

**Cross-harness session browser:**
- `agent-sessions` (jazzyalex, macOS native) — unifies ten agent sources (Codex, Claude Code, Cursor Agent, Hermes, OpenClaw, Copilot CLI, and others) into one browsable/searchable list. Tool-call inspection, token visibility, per-model cost lensing, live quota-burn meter, read-only, no telemetry. Closest existing analogue to Journey 1.

**Token/cost dashboards:**
- `claude-code-stats`, `token-dashboard`, `claude-usage`, `claude-session-visualizer`, `Claude-Code-Agent-Monitor`, `claude-view` — several already implement cache-vs-fresh token splits, per-subagent cost attribution ("where quantifiable"), and file/tool hotspot heatmaps directly from Claude Code JSONL. `token-dashboard` is the strongest proof point that the format supports this granularity.

None of the surveyed tools implement a directed backward-provenance trace (walk from a suspect output to the specific read/fetch/subagent-report that fed it), nor cross-session recurring-pattern detection, nor named/saved session collections.

## Subagent Linkage in Claude Code's JSONL (open question, contested)

GitHub issue [anthropics/claude-code#32175](https://github.com/anthropics/claude-code/issues/32175) reports that a subagent's own transcript carries no back-reference to its parent session/turn/spawning tool-call — only the parent-side `tool_use`/`toolUseResult.agentId` records the spawn. The issue was closed as a duplicate, unresolved as of this research.

However, secondary sources disagree with the issue and with each other on the actual current field/layout. Candidate linkage vectors to check directly against real `~/.claude/projects/` data before any design commits to a subagent-linkage model:

1. **Directory-path linkage** — one source describes subagent files living at `{sessionId}/subagents/agent-{agentId}.jsonl`, i.e. the parent session ID is the directory name itself, independent of any in-file field. Conflicts with the issue's description of a flatter layout.
2. **`parentToolUseId` field** — one source claims subagent records carry `parentToolUseId` (the spawning tool call), plus `agentId`/`agentType`/`teamName`. If true, this directly closes the gap the issue asks for; may be version-dependent (shipped after the issue was filed) or may apply to some record types but not the `session_meta` entry the issue specifically checked.
3. **`cwd` + timestamp-window narrowing** — every record (parent and child) carries `cwd`; narrowing candidate parent sessions to matching `cwd` before applying timestamp-window correlation reduces false positives from unrelated concurrent sessions, though it doesn't fully resolve concurrent subagents within the same project.
4. **Timestamp-window correlation alone** — the established community fallback; weak under concurrent subagents.

**Action before design relies on any of this:** inspect real local `~/.claude/projects/` session files directly (not secondary sources, which disagree) to determine which vectors actually hold on the current Claude Code version.

## Journey Coverage Table

| Journey | Coverage | Notes |
|---|---|---|
| 1. Find a session | Substantial | `agent-sessions` already does cross-harness discovery, browse/filter/search, recency-sorted lists, no manual folder-pointing. |
| 2. Investigate one session's tool calls (backward "wrong turn" trace) | Greenfield | No surveyed tool walks backward from a suspect output to the tool call that introduced it, or groups by the brief's "Sources" (Bash+Read+WebFetch) taxonomy. |
| 3. Investigate a collection of sessions | Greenfield | Per-session dashboards exist; no surveyed tool does side-by-side or recurring-pattern comparison across sessions. |
| 4. Review token usage | Partial | Cache-vs-fresh + per-subagent attribution proven for Claude Code (`token-dashboard`); Codex depth unconfirmed. No tool correlates a spend spike to the tool calls that caused it. |
| 5. Review subagents | Partial, format-limited | Live subagent-tree visualizers exist but lean real-time, not post-hoc; parent-decision linkage blocked by the contested format gap above. |
| 6. Traditional conversation view | Substantial | Every per-harness renderer already does this well. |

## Reuse Candidates (evaluate licensing/architecture fit if pursued)

- `agent-sessions` — cross-harness discovery + traditional view approach.
- `token-dashboard` — proven token-accounting math and per-subagent/cache attribution logic.

Neither is a substitute for Journeys 2/3/5, which is where this brief's actual novelty lives — treat as prior art to learn from, not a base to fork, unless a later licensing/fit check says otherwise.

## Sources

[1] "GitHub - delexw/claude-code-trace," GitHub. https://github.com/delexw/claude-code-trace
[2] "GitHub - daaain/claude-code-log," GitHub. https://github.com/daaain/claude-code-log
[3] "GitHub - simonw/claude-code-transcripts," GitHub. https://github.com/simonw/claude-code-transcripts
[4] "Inside Claude Code: The Session File Format and How to Inspect It," Y. Huang, Medium. https://databunny.medium.com/inside-claude-code-the-session-file-format-and-how-to-inspect-it-b9998e66d56b
[5] "GitHub - PixelPaw-Labs/codex-trace," GitHub. https://github.com/PixelPaw-Labs/codex-trace
[6] "GitHub - masonc15/codex-transcript-viewer," GitHub. https://github.com/masonc15/codex-transcript-viewer
[7] "GitHub - Uri2001/codex-sessions," GitHub. https://github.com/Uri2001/codex-sessions
[8] "@nogataka/codex-viewer," npm. https://www.npmjs.com/package/@nogataka/codex-viewer
[9] "GitHub - Cocoanetics/CodexMonitor," GitHub. https://github.com/Cocoanetics/CodexMonitor
[10] "Codex CLI History Viewer for VS Code," ccassist.dev. https://ccassist.dev/codex-cli-history-viewer/
[11] "GitHub - jazzyalex/agent-sessions," GitHub. https://github.com/jazzyalex/agent-sessions
[12] "Agent Sessions — Session Management for Codex, Claude, OpenCode, Cursor, Copilot, Pi and Antigravity." https://jazzyalex.github.io/agent-sessions/
[13] "GitHub - AeternaLabsHQ/claude-code-stats," GitHub. https://github.com/AeternaLabsHQ/claude-code-stats
[14] "GitHub - nateherkai/token-dashboard," GitHub. https://github.com/nateherkai/token-dashboard
[15] "GitHub - phuryn/claude-usage," GitHub. https://github.com/phuryn/claude-usage
[16] "Costs," Anthropic Claude Code Docs. https://docs.anthropic.com/en/docs/claude-code/costs
[17] "GitHub - anaypaul/claude-session-visualizer," GitHub. https://github.com/anaypaul/claude-session-visualizer
[18] "GitHub - hoangsonww/Claude-Code-Agent-Monitor," GitHub. https://github.com/hoangsonww/Claude-Code-Agent-Monitor
[19] "claude-view: Mission Control for Claude Code," recca0120.github.io. https://recca0120.github.io/en/2026/04/07/claude-view-mission-control/
[20] "feat: write parent session context into subagent session_meta for distributed tracing · Issue #32175," anthropics/claude-code, GitHub. https://github.com/anthropics/claude-code/issues/32175
[21] "GitHub - Bububuger/spanory," GitHub. https://github.com/Bububuger/spanory
