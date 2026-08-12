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

/** Mirrors the Rust `SessionListItem`. */
export interface SessionListItem {
  id: string;
  harness: Harness;
  project: SourceValue<string>;
  event_count: number;
  token_total: SourceValue<number>;
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

/** Reads the currently indexed sessions, ordered most-recent-first. */
export async function listSessions(): Promise<SessionListItem[]> {
  await initIndex();
  const query: ListSessionsQuery = {
    limit: 500,
    offset: 0,
    harness: null,
    project: null,
  };
  const page = await invoke<Paged<SessionListItem>>("list_sessions", { query });
  return page.items;
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
