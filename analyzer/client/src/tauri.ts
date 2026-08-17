import { invoke } from "@tauri-apps/api/core";
import { type UnlistenFn, listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

export interface AppCommands {
  app_ready: string;
}

export async function appReady(): Promise<AppCommands["app_ready"]> {
  return invoke("app_ready");
}

export async function chooseSessionsDirectory(): Promise<string | null> {
  const selected = await open({
    directory: true,
    multiple: false,
    recursive: true,
  });
  return typeof selected === "string" ? selected : null;
}

/** Mirrors the Rust `Harness` enum (`#[serde(rename_all = "snake_case")]`). */
export type Harness = "claude_code" | "codex" | "pi";

/**
 * Mirrors the Rust `SourceValue<T>`: a value is either `Recorded` with data or
 * one of three explicit "not observed" states. The UI must preserve these
 * distinctions rather than fabricating a value.
 */
export type SourceValue<T> = { Recorded: T } | "Absent" | "Unsupported" | "Malformed";

export function isRecorded<T>(value: SourceValue<T>): value is { Recorded: T } {
  return typeof value === "object" && value !== null && "Recorded" in value;
}

/**
 * One session's token and dollar figures, folded by the indexer when the
 * session was first read and stored on its row.
 *
 * The client does not compute these. A session's headline spend is read from
 * here rather than folded out of an event page, so a list row can show it
 * without opening a transcript, and so the rate table that turns tokens into
 * dollars lives in exactly one place — the Rust index.
 *
 * Scoped to the session's *own* events. A spawned child is its own indexed
 * session with its own figures; summing a subtree means summing the rows.
 */
export interface SessionTokenFigures {
  /**
   * Tokens the harness itself totalled. Claude writes no total anywhere, so
   * this is unrecorded for every Claude session and `estimated_tokens` is the
   * figure to fall back to.
   */
  token_total: SourceValue<number>;
  /** Millionths of a dollar the harness itself charged. Only Pi writes any. */
  recorded_price_micros: SourceValue<number>;
  /** The deduped four-bucket sum the estimate below priced. */
  estimated_tokens: SourceValue<number>;
  /**
   * Millionths of a dollar derived from the index's pinned rate table when the
   * session was first seen, and frozen from then on. Unrecorded when the
   * harness charged its own price, when the session was already more than a
   * month old at first discovery, or when no model it named has a public rate.
   */
  estimated_price_micros: SourceValue<number>;
  /**
   * The ISO date of the rate table that produced the estimate above. The index
   * refreshes its rate table while a written estimate stays frozen, so this is
   * the only thing that says how old the rates behind a price are — a surface
   * that showed an estimate without it would leave the reader to assume today's.
   */
  estimated_as_of: SourceValue<string>;
}

/** Mirrors the Rust `SessionListItem`. */
export interface SessionListItem extends SessionTokenFigures {
  id: string;
  harness: Harness;
  project: SourceValue<string>;
  event_count: number;
  last_activity: SourceValue<string>;
}

interface Paged<T> {
  items: T[];
}

interface DiscoveryRoots {
  home: string | null;
  pi_session_roots: string[];
}

interface ListSessionsQuery {
  limit: number;
  offset: number;
  harness: Harness | null;
  project: string | null;
}

/** Mirrors the Rust `EventKind` (`#[serde(rename_all = "snake_case")]`). */
export enum EventKind {
  UserTurn = "user_turn",
  AssistantTurn = "assistant_turn",
  ToolCall = "tool_call",
  ToolResult = "tool_result",
  SubagentSpawn = "subagent_spawn",
  SessionMetadata = "session_metadata",
  ModelChange = "model_change",
  ThinkingChange = "thinking_change",
  Summary = "summary",
  Unknown = "unknown",
}

/** Mirrors the Rust `Provenance`: where a normalized event came from. */
export interface Provenance {
  harness: Harness;
  path: string;
  line: number;
  ordinal: number;
}

/** Mirrors the Rust `Turn`. */
export interface Turn {
  role: string;
  text: SourceValue<string>;
}

/** Mirrors the Rust `ToolCall`. */
export interface ToolCall {
  name: string;
  call_id: SourceValue<string>;
  input: SourceValue<string>;
  command: SourceValue<string>;
  path: SourceValue<string>;
  url: SourceValue<string>;
  cwd: SourceValue<string>;
}

/** Mirrors the Rust `ToolResult`. */
export interface ToolResult {
  call_id: SourceValue<string>;
  output: SourceValue<string>;
  is_error: SourceValue<boolean>;
}

/**
 * Mirrors the Rust `TokenUsage`. Every dimension is independently
 * recorded-or-not, and the values are the harness's own: `input` is whatever
 * the source wrote, which for Codex already contains its cached portion. The
 * per-harness normalization into disjoint buckets lives in `tokens/rollup.ts`.
 */
export interface TokenUsage {
  input: SourceValue<number>;
  output: SourceValue<number>;
  cache_read: SourceValue<number>;
  cache_write: SourceValue<number>;
  total: SourceValue<number>;
  /**
   * What the harness itself charged for this record, in millionths of one US
   * dollar. Only Pi writes a dollar figure, so this is unrecorded for Claude
   * and Codex — whose price can only be a catalog estimate, labelled as one.
   */
  cost_total_micros: SourceValue<number>;
  scope: string;
}

/**
 * The name this record carried while only subagent payloads used it. Kept as an
 * alias because it is part of the module's published surface.
 */
export type SubagentTokenUsage = TokenUsage;

/**
 * Mirrors the Rust `Subagent`: the delegation facts one harness recorded, each
 * independently recorded-or-not. A harness that wrote no duration leaves
 * `duration_ms` unrecorded rather than zero.
 */
export interface Subagent {
  native_id: SourceValue<string>;
  agent_type: SourceValue<string>;
  prompt: SourceValue<string>;
  outcome: SourceValue<string>;
  nickname: SourceValue<string>;
  duration_ms: SourceValue<number>;
  token_usage: SourceValue<TokenUsage>;
  /** The indexed child session this spawn produced, when the source named one. */
  child_session_id: SourceValue<string>;
}

/** Mirrors the Rust `FileReference`. */
export interface FileReference {
  path: string;
  operation: SourceValue<string>;
}

/**
 * Mirrors the Rust `Event`. Named `AillyEvent` to avoid clashing with the DOM
 * `Event`.
 */
export interface AillyEvent {
  id: string;
  session_id: string;
  kind: EventKind;
  source: Provenance;
  native_id: SourceValue<string>;
  /**
   * The API response this event's usage belongs to, when the harness wrote one.
   * Several Claude records repeat one response's usage, so this is the key that
   * collapses them; Codex and Pi leave it unrecorded.
   */
  response_id: SourceValue<string>;
  /**
   * The model that produced this event, when a record of the same transcript
   * named one. Claude and Pi write it beside the usage it priced; Codex names
   * it on a separate record, so its usage events carry the last one named
   * before them. Per-event because a model can change mid-session.
   */
  model: SourceValue<string>;
  timestamp: SourceValue<string>;
  turn: SourceValue<Turn>;
  tool_call: SourceValue<ToolCall>;
  tool_result: SourceValue<ToolResult>;
  token_usage: SourceValue<TokenUsage>;
  files: SourceValue<FileReference[]>;
  detail: SourceValue<string>;
  subagent: SourceValue<Subagent>;
}

interface EventPage {
  events: AillyEvent[];
}

interface PageQuery {
  limit: number;
  offset: number;
}

/** How far a running reconcile has walked its discovered sources. */
export interface IndexProgress {
  indexed: number;
  total: number;
}

/** Mirrors the Rust `IndexStatus`. */
export type IndexStatus = "idle" | "running" | { error: { message: string } };

/**
 * Renders an unknown thrown value as a message. Tauri rejects commands with the
 * plain `String` from the Rust `Err`, so that string is the useful part.
 */
export function errorMessage(error: unknown): string {
  if (typeof error === "string") {
    return error;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

let initialized: Promise<void> | null = null;

/**
 * Opens the index once per process. The backend resolves its own app-data
 * location, so this needs no path capability in the webview.
 */
export async function initIndex(): Promise<void> {
  if (!initialized) {
    initialized = invoke<void>("index_init").catch((error: unknown) => {
      initialized = null;
      throw error;
    });
  }
  return initialized;
}

/**
 * Reads the currently indexed sessions, ordered most-recent-first. One high
 * ceiling (100_000) covers any realistic local collection for now;
 * pagination/virtualization of the session list is deferred until real
 * collections are large enough to measure.
 */
export async function listSessions(): Promise<SessionListItem[]> {
  await initIndex();
  const query: ListSessionsQuery = {
    limit: 100_000,
    offset: 0,
    harness: null,
    project: null,
  };
  const page = await invoke<Paged<SessionListItem>>("list_sessions", { query });
  return page.items;
}

/**
 * Reads one bounded page of a session's normalized events, ordered by native
 * record order (source ordinal). The whole conversation fits in one page for
 * now; pagination/virtualization is deferred until real collections are large.
 */
export async function getEventPage(sessionId: string): Promise<AillyEvent[]> {
  await initIndex();
  const query: PageQuery = { limit: 5000, offset: 0 };
  const page = await invoke<EventPage>("get_event_page", { sessionId, query });
  return page.events;
}

/**
 * Starts a background reconcile against the default local roots and resolves as
 * soon as it is running — the backend reads `$HOME` when `home` is null. Watch
 * progress with `onIndexProgress` and completion with `onIndexComplete`.
 */
export async function startRefresh(): Promise<void> {
  await initIndex();
  const roots: DiscoveryRoots = { home: null, pi_session_roots: [] };
  await invoke("index_refresh", { roots });
}

export async function onIndexProgress(
  handler: (progress: IndexProgress) => void,
): Promise<UnlistenFn> {
  return listen<IndexProgress>("index-progress", (event) => handler(event.payload));
}

export async function onIndexComplete(handler: (status: IndexStatus) => void): Promise<UnlistenFn> {
  return listen<IndexStatus>("index-complete", (event) => handler(event.payload));
}
