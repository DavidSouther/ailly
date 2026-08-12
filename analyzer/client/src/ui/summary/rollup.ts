import {
  type AillyEvent,
  EventKind,
  type SourceValue,
  type ToolCall,
  type ToolResult,
  isRecorded,
} from "../../tauri";

export type ToolCategory = "exec" | "edit" | "read" | "other";
export type SourceKind = "shell" | "file" | "web";

export interface ToolCategoryTotal {
  category: ToolCategory;
  count: number;
  /** Rounded percentage of total tool calls, 0-100. */
  share: number;
}

export interface UnclassifiedTotal {
  count: number;
  share: number;
  /** Distinct raw tool names the category table does not know, in first-seen order. */
  toolNames: string[];
}

export interface ToolFrequency {
  name: string;
  count: number;
  share: number;
  category: ToolCategory | "unclassified";
}

export interface SourceCall {
  eventId: string;
  toolName: string;
  /** Command text, file path, or URL — whichever the group's kind cares about. */
  detail: string;
  /** True when the source data is missing, so detail reads e.g. "Target not recorded". */
  detailRecorded: boolean;
  /**
   * The directory this call ran in, or null when the harness recorded none.
   * Unlike `detail` there is no stand-in copy: an unrecorded directory is
   * shown as nothing at all rather than claimed to be the session's.
   */
  cwd: string | null;
  /**
   * What the call returned, or null when nothing in this session answered it.
   * The `SourceValue` is kept rather than flattened, because "recorded an
   * image" and "recorded nothing" are different facts about the call.
   */
  output: SourceValue<string> | null;
  /** True only when the harness marked the result as an error. */
  outputIsError: boolean;
}

export interface SourceGroup {
  kind: SourceKind;
  label: string;
  count: number;
  calls: SourceCall[];
  /**
   * Distinct paths ranked by how often they were touched — populated only when
   * kind === "file", and drawn from every call that recorded a path (reads and
   * edits), not only the group's own call list.
   */
  files: FileTouch[];
}

export interface FileTouch {
  path: string;
  touches: number;
  /** Distinct tool names that touched this file, in first-seen order. */
  tools: string[];
}

export interface SessionSummaryStats {
  toolCallCount: number;
  filesTouchedCount: number;
  duration: SourceValue<string>;
  subagentSpawnCount: SourceValue<number>;
  categories: ToolCategoryTotal[];
  unclassified: UnclassifiedTotal;
  toolsByFrequency: ToolFrequency[];
  sources: SourceGroup[];
  filesTouched: FileTouch[];
}

/**
 * Tool names are open harness strings, so this table is additive rather than
 * exhaustive: anything missing lands in the visible "unclassified" bucket
 * instead of being absorbed into "other".
 */
const CATEGORY_TABLE: Record<string, ToolCategory> = {
  Bash: "exec",
  Shell: "exec",
  shell: "exec",
  Edit: "edit",
  Write: "edit",
  MultiEdit: "edit",
  apply_patch: "edit",
  Read: "read",
  Glob: "read",
  Grep: "read",
  NotebookRead: "read",
  Agent: "other",
  Task: "other",
  TodoWrite: "other",
  Skill: "other",
  AskUserQuestion: "other",
  WebFetch: "other",
  WebSearch: "other",
  // Codex's names.
  exec_command: "exec",
  write_stdin: "exec",
  read_file: "read",
  update_plan: "other",
  // Pi's names, which are lowercase and so distinct from Claude's above.
  bash: "exec",
  read: "read",
  edit: "edit",
  write: "edit",
};

const CATEGORY_ORDER: ToolCategory[] = ["exec", "edit", "read", "other"];

const SOURCE_META: Record<SourceKind, { label: string; missingDetail: string }> = {
  shell: { label: "Shell output", missingDetail: "Command not recorded" },
  file: { label: "File access", missingDetail: "Path not recorded" },
  web: { label: "Web / API", missingDetail: "Target not recorded" },
};

/** Static, additive tool-name → category table. Unknown names fall to "unclassified". */
export function categoryForTool(name: string): ToolCategory | "unclassified" {
  return CATEGORY_TABLE[name] ?? "unclassified";
}

/** Name heuristic only: matches "WebFetch"-shaped tool names. */
export function isWebCall(name: string): boolean {
  return /web|fetch|http/i.test(name);
}

interface RecordedCall {
  event: AillyEvent;
  tool: ToolCall;
}

/** Tool-call events whose tool detail the source actually recorded. */
function recordedCalls(events: AillyEvent[]): RecordedCall[] {
  const calls: RecordedCall[] = [];
  for (const event of events) {
    if (event.kind === EventKind.ToolCall && isRecorded(event.tool_call)) {
      calls.push({ event, tool: event.tool_call.Recorded });
    }
  }
  return calls;
}

function share(count: number, total: number): number {
  return total === 0 ? 0 : Math.round((count / total) * 100);
}

function categoryTotals(calls: RecordedCall[]): {
  categories: ToolCategoryTotal[];
  unclassified: UnclassifiedTotal;
} {
  const counts = new Map<ToolCategory | "unclassified", number>();
  const unknownNames: string[] = [];
  for (const call of calls) {
    const category = categoryForTool(call.tool.name);
    counts.set(category, (counts.get(category) ?? 0) + 1);
    if (category === "unclassified" && !unknownNames.includes(call.tool.name)) {
      unknownNames.push(call.tool.name);
    }
  }

  const total = calls.length;
  const categories = CATEGORY_ORDER.filter((category) => (counts.get(category) ?? 0) > 0).map(
    (category) => {
      const count = counts.get(category) ?? 0;
      return { category, count, share: share(count, total) };
    },
  );
  const unknownCount = counts.get("unclassified") ?? 0;
  return {
    categories,
    unclassified: {
      count: unknownCount,
      share: share(unknownCount, total),
      toolNames: unknownNames,
    },
  };
}

function toolsByFrequency(calls: RecordedCall[]): ToolFrequency[] {
  const counts = new Map<string, number>();
  for (const call of calls) {
    counts.set(call.tool.name, (counts.get(call.tool.name) ?? 0) + 1);
  }
  return [...counts.entries()]
    .map(([name, count]) => ({
      name,
      count,
      share: share(count, calls.length),
      category: categoryForTool(name),
    }))
    .sort((a, b) => b.count - a.count);
}

/** The field each kind of source is about: a command, a path, or a URL. */
function detailField(tool: ToolCall, kind: SourceKind): SourceValue<string> {
  switch (kind) {
    case "shell":
      return tool.command;
    case "file":
      return tool.path;
    case "web":
      return tool.url;
  }
}

/**
 * Results indexed by the call they answer. Pairing is by recorded id only: a
 * result naming no call, and a call nothing answered, stay unpaired rather
 * than being matched by adjacency, which the transcript never asserts.
 */
export function resultsByCallId(events: AillyEvent[]): Map<string, ToolResult[]> {
  const byCall = new Map<string, ToolResult[]>();
  for (const event of events) {
    if (event.kind !== EventKind.ToolResult || !isRecorded(event.tool_result)) {
      continue;
    }
    const result = event.tool_result.Recorded;
    if (!isRecorded(result.call_id)) {
      continue;
    }
    const answers = byCall.get(result.call_id.Recorded) ?? [];
    answers.push(result);
    byCall.set(result.call_id.Recorded, answers);
  }
  return byCall;
}

function sourceCall(
  call: RecordedCall,
  kind: SourceKind,
  results: Map<string, ToolResult[]>,
): SourceCall {
  const field = detailField(call.tool, kind);
  const answers = isRecorded(call.tool.call_id)
    ? (results.get(call.tool.call_id.Recorded) ?? [])
    : [];
  // Codex chunks a long result across several records; they belong to the same
  // call and read as one output in source order.
  const outputs = answers.flatMap((result) =>
    isRecorded(result.output) ? [result.output.Recorded] : [],
  );
  // With nothing recorded to join, the first answer's own state is the honest
  // report: it may say the output was an image rather than nothing at all.
  const output =
    outputs.length > 0 ? { Recorded: outputs.join("\n") } : (answers[0]?.output ?? null);
  return {
    eventId: call.event.id,
    toolName: call.tool.name,
    detail: isRecorded(field) ? field.Recorded : SOURCE_META[kind].missingDetail,
    detailRecorded: isRecorded(field),
    cwd: isRecorded(call.tool.cwd) ? call.tool.cwd.Recorded : null,
    output,
    outputIsError: answers.some(
      (result) => isRecorded(result.is_error) && result.is_error.Recorded,
    ),
  };
}

function sourceGroup(
  kind: SourceKind,
  calls: RecordedCall[],
  files: FileTouch[],
  results: Map<string, ToolResult[]>,
): SourceGroup | null {
  if (calls.length === 0 && (kind !== "file" || files.length === 0)) {
    return null;
  }
  const sourceCalls = calls.map((call) => sourceCall(call, kind, results));
  return {
    kind,
    label: SOURCE_META[kind].label,
    count: kind === "file" ? files.length : sourceCalls.length,
    calls: sourceCalls,
    files: kind === "file" ? files : [],
  };
}

/** True when the call is one of the ways an outside fact enters the context window. */
function sourceKindOf(tool: ToolCall): SourceKind | null {
  if (isWebCall(tool.name)) {
    return "web";
  }
  const category = categoryForTool(tool.name);
  if (category === "exec") {
    return "shell";
  }
  if (category === "read" || category === "edit") {
    return "file";
  }
  return null;
}

function sourceGroups(
  calls: RecordedCall[],
  files: FileTouch[],
  results: Map<string, ToolResult[]>,
): SourceGroup[] {
  const kinds: SourceKind[] = ["shell", "file", "web"];
  return kinds
    .map((kind) =>
      sourceGroup(
        kind,
        calls.filter((call) => sourceKindOf(call.tool) === kind),
        files,
        results,
      ),
    )
    .filter((group): group is SourceGroup => group !== null);
}

function filesTouched(calls: RecordedCall[]): FileTouch[] {
  const byPath = new Map<string, FileTouch>();
  for (const call of calls) {
    if (!isRecorded(call.tool.path)) {
      continue;
    }
    const path = call.tool.path.Recorded;
    const touch = byPath.get(path) ?? { path, touches: 0, tools: [] };
    touch.touches += 1;
    if (!touch.tools.includes(call.tool.name)) {
      touch.tools.push(call.tool.name);
    }
    byPath.set(path, touch);
  }
  return [...byPath.values()].sort((a, b) => b.touches - a.touches);
}

/** A span between the first and last recorded timestamps, when there are two. */
function sessionDuration(events: AillyEvent[]): SourceValue<string> {
  const times = events
    .map((event) =>
      isRecorded(event.timestamp) ? Date.parse(event.timestamp.Recorded) : Number.NaN,
    )
    .filter((time) => !Number.isNaN(time));
  if (times.length < 2) {
    return "Absent";
  }
  return { Recorded: durationLabel(Math.max(...times) - Math.min(...times)) };
}

function durationLabel(milliseconds: number): string {
  const seconds = Math.round(milliseconds / 1000);
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (hours > 0) {
    return `${hours}h ${minutes}m`;
  }
  if (minutes > 0) {
    return `${minutes}m ${seconds % 60}s`;
  }
  return `${seconds}s`;
}

/** Absent rather than zero: no harness records spawns yet, so zero is not a fact. */
function subagentSpawnCount(events: AillyEvent[]): SourceValue<number> {
  const count = events.filter((event) => event.kind === EventKind.SubagentSpawn).length;
  return count === 0 ? "Absent" : { Recorded: count };
}

/** The single fold this feature performs over one session's event page. */
export function summarizeSession(events: AillyEvent[]): SessionSummaryStats {
  const calls = recordedCalls(events);
  const { categories, unclassified } = categoryTotals(calls);
  const files = filesTouched(calls);
  return {
    toolCallCount: calls.length,
    filesTouchedCount: files.length,
    duration: sessionDuration(events),
    subagentSpawnCount: subagentSpawnCount(events),
    categories,
    unclassified,
    toolsByFrequency: toolsByFrequency(calls),
    sources: sourceGroups(calls, files, resultsByCallId(events)),
    filesTouched: files,
  };
}
