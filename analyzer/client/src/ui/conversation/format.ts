import {
  type AillyEvent,
  EventKind,
  type SourceValue,
  type ToolCall,
  isRecorded,
} from "../../tauri";

/**
 * Stable DOM anchor id for an event, so summary and drill-down views can later
 * link straight to a turn. Source ids can contain path separators and colons,
 * so anything outside a safe id charset collapses to a dash.
 */
export function eventAnchorId(eventId: string): string {
  return `event-${eventId.replace(/[^A-Za-z0-9_-]+/g, "-")}`;
}

/** Human role label for a turn event, keeping the raw role when unfamiliar. */
export function roleLabel(event: AillyEvent): string {
  if (event.kind === EventKind.UserTurn) {
    return "User";
  }
  if (event.kind === EventKind.AssistantTurn) {
    return "Assistant";
  }
  if (isRecorded(event.turn)) {
    const role = event.turn.Recorded.role;
    return role.length > 0 ? role : "Turn";
  }
  return "Turn";
}

/** The turn's text, or an explicit note when the source recorded none. */
export function turnText(event: AillyEvent): string {
  if (isRecorded(event.turn) && isRecorded(event.turn.Recorded.text)) {
    return event.turn.Recorded.text.Recorded;
  }
  return "No text recorded";
}

/** Display title for a tool call: its tool name. */
export function toolTitle(tool: ToolCall): string {
  return tool.name.length > 0 ? tool.name : "Tool call";
}

export interface DetailRow {
  label: string;
  value: string;
}

/** Only the recorded tool fields, as label/value rows for the expanded view. */
export function toolDetailRows(tool: ToolCall): DetailRow[] {
  const candidates: Array<[string, SourceValue<string>]> = [
    ["Command", tool.command],
    ["Path", tool.path],
    ["URL", tool.url],
    ["Input", tool.input],
  ];
  return candidates
    .filter((entry): entry is [string, { Recorded: string }] => isRecorded(entry[1]))
    .map(([label, value]) => ({ label, value: value.Recorded }));
}

/** A subagent spawn's recorded detail, or an explicit note when absent. */
export function subagentDetail(event: AillyEvent): string {
  return isRecorded(event.detail) ? event.detail.Recorded : "No delegation detail recorded";
}
