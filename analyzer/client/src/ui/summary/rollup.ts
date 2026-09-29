import {
  type AillyEvent,
  EventKind,
  type SourceValue,
  type ToolCall,
  type ToolResult,
  isRecorded,
} from "../../tauri";
import { toolPayloadText } from "../payload";

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
  /** Every call of this tool, in source order, with the detail Sources used to hold. */
  calls: SourceCall[];
}

export interface SourceCall {
  eventId: string;
  toolName: string;
  /** Command text, file path, or URL — whichever the group's kind cares about. */
  detail: string;
  /** True when the source data is missing, so detail reads e.g. "Target not recorded". */
  detailRecorded: boolean;
  /**
   * The path this call recorded, if any. Kept beside `detail` because a path
   * and a command are different facts to the working-directory decision, and
   * `detail` cannot tell them apart.
   */
  path: string | null;
  /**
   * The directory this call ran in, or null when the harness recorded none.
   * Unlike `detail` there is no stand-in copy: an unrecorded directory is
   * shown as nothing at all rather than claimed to be the session's.
   */
  cwd: string | null;
  /** The recorded parameters this call's own row does not already show. */
  payload: SourceValue<string> | null;
  /**
   * What the call returned, or null when nothing in this session answered it.
   * The `SourceValue` is kept rather than flattened, because "recorded an
   * image" and "recorded nothing" are different facts about the call.
   */
  output: SourceValue<string> | null;
  outputIsError: boolean;
}

export interface SourceGroup {
  kind: SourceKind;
  label: string;
  count: number;
  calls: SourceCall[];
}

/**
 * Aggregated accesses keyed by `(target, path, cwd, ambiguity)`. Ambiguous
 * command fragments remain separate from literal paths.
 */
export interface FileAccess {
  /** Row identity; the only key a caller should render rows by. */
  id: string;
  path: string;
  target: string;
  cwd: string | null;
  touches: number;
  /** First-seen order. */
  operations: string[];
  /** First-seen order. */
  provenances: string[];
  /** Unresolved command fragment, or null for a literal path. */
  ambiguity: string | null;
}

export interface SessionSummaryStats {
  toolCallCount: number;
  /**
   * Distinct files. Narrower than the Filesystem list on purpose: a directory
   * is a path the session reached but not a file it touched, and an ambiguous
   * fragment is an access but not a name that was reached.
   */
  filesTouchedCount: number;
  duration: SourceValue<string>;
  subagentSpawnCount: SourceValue<number>;
  categories: ToolCategoryTotal[];
  unclassified: UnclassifiedTotal;
  toolsByFrequency: ToolFrequency[];
  sources: SourceGroup[];
  fileAccesses: FileAccess[];
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
  // Codex's names. `exec` is the name on a `custom_tool_call`, whose command
  // text lives in a JavaScript snippet this product does not yet recover — the
  // call is still an exec, because the category is a claim about what the call
  // was rather than about what was read back out of it.
  exec: "exec",
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
  file: { label: "File tools", missingDetail: "Path not recorded" },
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

function toolsByFrequency(
  calls: RecordedCall[],
  results: Map<string, ToolResult[]>,
): ToolFrequency[] {
  const byName = new Map<string, RecordedCall[]>();
  for (const call of calls) {
    const named = byName.get(call.tool.name) ?? [];
    named.push(call);
    byName.set(call.tool.name, named);
  }
  return [...byName.entries()]
    .map(([name, named]) => ({
      name,
      count: named.length,
      share: share(named.length, calls.length),
      category: categoryForTool(name),
      calls: named.map((call) => callDetail(call, results)),
    }))
    .sort((a, b) => b.count - a.count);
}

/**
 * The detail a Calls-by-tool row expands to: command, path, or URL when the
 * tool is one of those kinds, otherwise whatever target field the harness
 * wrote. Missing targets stay labelled rather than blank.
 */
function callDetail(call: RecordedCall, results: Map<string, ToolResult[]>): SourceCall {
  const kind = sourceKindOf(call.tool);
  if (kind !== null) {
    return sourceCall(call, kind, results);
  }
  const field = firstRecordedTarget(call.tool);
  const answers = isRecorded(call.tool.call_id)
    ? (results.get(call.tool.call_id.Recorded) ?? [])
    : [];
  const outputs = answers.flatMap((result) =>
    isRecorded(result.output) ? [result.output.Recorded] : [],
  );
  const output =
    outputs.length > 0 ? { Recorded: outputs.join("\n") } : (answers[0]?.output ?? null);
  return {
    eventId: call.event.id,
    toolName: call.tool.name,
    detail: field ?? "Detail not recorded",
    detailRecorded: field !== null,
    path: isRecorded(call.tool.path) ? call.tool.path.Recorded : null,
    cwd: isRecorded(call.tool.cwd) ? call.tool.cwd.Recorded : null,
    payload: toolPayloadText(call.tool),
    output,
    outputIsError: answers.some(
      (result) => isRecorded(result.is_error) && result.is_error.Recorded,
    ),
  };
}

function firstRecordedTarget(tool: ToolCall): string | null {
  for (const field of [tool.command, tool.path, tool.url, tool.input] as const) {
    if (isRecorded(field)) {
      return field.Recorded;
    }
  }
  return null;
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
    path: isRecorded(call.tool.path) ? call.tool.path.Recorded : null,
    cwd: isRecorded(call.tool.cwd) ? call.tool.cwd.Recorded : null,
    payload: toolPayloadText(call.tool),
    output,
    outputIsError: answers.some(
      (result) => isRecorded(result.is_error) && result.is_error.Recorded,
    ),
  };
}

function sourceGroup(
  kind: SourceKind,
  calls: RecordedCall[],
  results: Map<string, ToolResult[]>,
): SourceGroup | null {
  if (calls.length === 0) {
    return null;
  }
  const sourceCalls = calls.map((call) => sourceCall(call, kind, results));
  return {
    kind,
    label: SOURCE_META[kind].label,
    count: sourceCalls.length,
    calls: sourceCalls,
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

function sourceGroups(calls: RecordedCall[], results: Map<string, ToolResult[]>): SourceGroup[] {
  const kinds: SourceKind[] = ["shell", "file", "web"];
  return kinds
    .map((kind) =>
      sourceGroup(
        kind,
        calls.filter((call) => sourceKindOf(call.tool) === kind),
        results,
      ),
    )
    .filter((group): group is SourceGroup => group !== null);
}

/**
 * Every file access the index attributed to this session's events, folded by
 * `FileAccess.id`.
 *
 * The index is the only thing that decides what a file access is: a tool that
 * names a path and a command whose operands imply one both arrive here already
 * attributed, so nothing is re-derived from tool arguments at read time. Events
 * with no attributed accesses contribute nothing, which keeps a session the
 * index has nothing to say about honestly empty rather than half-scanned.
 */
function fileAccesses(events: AillyEvent[]): FileAccess[] {
  const byIdentity = new Map<string, FileAccess>();
  for (const event of events) {
    if (!isRecorded(event.files)) {
      continue;
    }
    for (const file of event.files.Recorded) {
      if (file.path === "") {
        continue;
      }
      const cwd = isRecorded(file.cwd) ? file.cwd.Recorded : null;
      const ambiguity = isRecorded(file.ambiguity) ? file.ambiguity.Recorded : null;
      const target = isRecorded(file.target) ? file.target.Recorded : "file";
      const key = `${target}\0${file.path}\0${cwd ?? ""}\0${ambiguity ?? ""}`;
      const access = byIdentity.get(key) ?? {
        id: key,
        path: file.path,
        target,
        cwd,
        touches: 0,
        operations: [],
        provenances: [],
        // Part of the identity above, so every access folding into this row
        // carries the same reason — or the same absence of one.
        ambiguity,
      };
      access.touches += 1;
      addLabel(access.operations, file.operation);
      addLabel(access.provenances, file.provenance);
      byIdentity.set(key, access);
    }
  }
  return [...byIdentity.values()].sort((a, b) => b.touches - a.touches);
}

function addLabel(labels: string[], value: SourceValue<string>): void {
  if (!isRecorded(value) || value.Recorded === "" || labels.includes(value.Recorded)) {
    return;
  }
  labels.push(value.Recorded);
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

/** One form for every span the product shows, whole-session or per-spawn. */
export function durationLabel(milliseconds: number): string {
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

/**
 * Absent when the page has no spawn events. The Summary tile labels that as
 * "0" — the session recorded no delegations — rather than "Not recorded",
 * which would imply the harness omitted the field.
 */
function subagentSpawnCount(events: AillyEvent[]): SourceValue<number> {
  const count = events.filter((event) => event.kind === EventKind.SubagentSpawn).length;
  return count === 0 ? "Absent" : { Recorded: count };
}

/**
 * Every Summary statistic. The only entry point: a new statistic is added here
 * rather than derived inside a component, so two lenses over the same session
 * cannot disagree about it.
 */
export function summarizeSession(events: AillyEvent[]): SessionSummaryStats {
  const calls = recordedCalls(events);
  const { categories, unclassified } = categoryTotals(calls);
  const accesses = fileAccesses(events);
  const results = resultsByCallId(events);
  return {
    toolCallCount: calls.length,
    filesTouchedCount: accesses.filter(
      (access) => access.target === "file" && access.ambiguity === null,
    ).length,
    duration: sessionDuration(events),
    subagentSpawnCount: subagentSpawnCount(events),
    categories,
    unclassified,
    toolsByFrequency: toolsByFrequency(calls, results),
    sources: sourceGroups(calls, results),
    fileAccesses: accesses,
  };
}
