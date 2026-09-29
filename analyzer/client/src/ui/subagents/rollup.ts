import { type AillyEvent, EventKind, type SourceValue, isRecorded } from "../../tauri";
import { durationLabel } from "../summary/rollup";

/**
 * One recorded spawn, projected for display. Each field keeps its
 * `SourceValue` rather than flattening to a string, so the pane can name what
 * the harness never wrote instead of showing a zero.
 */
export interface SubagentSpawnRow {
  eventId: string;
  agentType: SourceValue<string>;
  prompt: SourceValue<string>;
  outcome: SourceValue<string>;
  durationLabel: SourceValue<string>;
  /**
   * The harness's own token figure for the spawn: how large the child's final
   * turn's context was, which is a different fact from what the child spent.
   * Named for what it is — a bare "Tokens" invited reading it as the cost of the
   * delegation, which it undercounts by 62× in the sampled session.
   */
  finalContextLabel: SourceValue<string>;
  childSessionId: SourceValue<string>;
}

/**
 * Every delegation this session recorded, in source order. A spawn event whose
 * payload the index could not read is left out, the same way `recordedCalls`
 * already drops a tool call with no recorded detail.
 */
export function subagentSpawnRows(events: AillyEvent[]): SubagentSpawnRow[] {
  const rows: SubagentSpawnRow[] = [];
  for (const event of events) {
    if (event.kind !== EventKind.SubagentSpawn || !isRecorded(event.subagent)) {
      continue;
    }
    const spawn = event.subagent.Recorded;
    const total = isRecorded(spawn.token_usage) ? spawn.token_usage.Recorded.total : "Absent";
    rows.push({
      eventId: event.id,
      agentType: spawn.agent_type,
      prompt: spawn.prompt,
      outcome: spawn.outcome,
      durationLabel: isRecorded(spawn.duration_ms)
        ? { Recorded: durationLabel(spawn.duration_ms.Recorded) }
        : spawn.duration_ms,
      finalContextLabel: isRecorded(total) ? { Recorded: total.Recorded.toLocaleString() } : total,
      childSessionId: spawn.child_session_id,
    });
  }
  return rows;
}

/** How many lines of a prompt the collapsed spawn row shows before an ellipsis. */
export const PROMPT_PREVIEW_LINES = 3;

/**
 * The short form of a prompt for a collapsed spawn row. The full text stays on
 * the control's accessible name so a truncated display never loses the match.
 */
export function promptPreview(prompt: string, maxLines = PROMPT_PREVIEW_LINES): string {
  const lines = prompt.split("\n");
  if (lines.length <= maxLines) {
    return prompt;
  }
  return `${lines.slice(0, maxLines).join("\n")}…`;
}
