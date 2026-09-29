import {
  type AillyEvent,
  EventKind,
  type SourceValue,
  type ToolCall,
  type ToolResult,
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

/** Absolute paths already say where they are; a cwd next to them is noise. */
export function isAbsolutePath(path: string): boolean {
  return path.startsWith("/") || /^[A-Za-z]:[\\/]/.test(path);
}

/**
 * Whether a call's working directory is worth showing. Absolute paths locate
 * themselves; a directory matching the session's is already on the session
 * tile. Anything else is ambiguous without it.
 */
export function shouldShowToolCwd(args: {
  cwd: string | null;
  path: string | null;
  sessionCwd: string | null;
}): boolean {
  if (args.cwd === null) {
    return false;
  }
  if (args.path !== null && isAbsolutePath(args.path)) {
    return false;
  }
  if (args.sessionCwd !== null && args.cwd === args.sessionCwd) {
    return false;
  }
  return true;
}

/**
 * The recorded tool fields, as label/value rows for the expanded view. The raw
 * payload is not among them: it is a block of recorded text rather than a
 * one-line value, so `toolPayloadText` decodes it and `ToolPayload` renders it
 * beneath these rows.
 */
export function toolDetailRows(
  tool: ToolCall,
  sessionCwd: SourceValue<string> = "Absent",
): DetailRow[] {
  const showCwd = shouldShowToolCwd({
    cwd: isRecorded(tool.cwd) ? tool.cwd.Recorded : null,
    path: isRecorded(tool.path) ? tool.path.Recorded : null,
    sessionCwd: isRecorded(sessionCwd) ? sessionCwd.Recorded : null,
  });
  const candidates: Array<[string, SourceValue<string>]> = [
    ["Command", tool.command],
    ["Path", tool.path],
    ["URL", tool.url],
    ...(showCwd ? ([["Working directory", tool.cwd]] as Array<[string, SourceValue<string>]>) : []),
  ];
  return candidates
    .filter((entry): entry is [string, { Recorded: string }] => isRecorded(entry[1]))
    .map(([label, value]) => ({ label, value: value.Recorded }));
}

/**
 * Display title for a tool result. The call it answers is the only identity a
 * result record carries, so it goes in the title when the harness recorded one.
 */
export function toolResultTitle(result: ToolResult | null): string {
  if (result === null || !isRecorded(result.call_id)) {
    return "Tool result";
  }
  return `Tool result — ${result.call_id.Recorded}`;
}

/**
 * What a spawn delegated, as the transcript reads it: the prompt the harness
 * recorded, else whatever raw detail the record carried, else an explicit note.
 * The full picture lives in the Subagents tab; this is the one line the
 * transcript owes a reader passing through.
 */
export function subagentDetail(event: AillyEvent): string {
  if (isRecorded(event.subagent) && isRecorded(event.subagent.Recorded.prompt)) {
    return event.subagent.Recorded.prompt.Recorded;
  }
  return isRecorded(event.detail) ? event.detail.Recorded : "No delegation detail recorded";
}
