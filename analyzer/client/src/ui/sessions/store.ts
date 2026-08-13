import { create } from "zustand";

import {
  type IndexProgress,
  type IndexStatus,
  type SessionListItem,
  errorMessage,
  listSessions,
  onIndexComplete,
  onIndexProgress,
  startRefresh,
} from "../../tauri";
import { matchesQuery } from "./format";
import { parseHarnessFilter } from "./searchQuery";

export interface SessionsState {
  sessions: SessionListItem[];
  indexing: boolean;
  progress: IndexProgress | null;
  error: string | null;
  search: string;
  selectedId: string | null;
}

export enum SessionsActionType {
  SessionsLoaded = "sessions_loaded",
  Progress = "progress",
  Complete = "complete",
  RescanStarted = "rescan_started",
  ScanFailed = "scan_failed",
  SetSearch = "set_search",
  Select = "select",
  Reset = "reset",
}

export type SessionsAction =
  | { type: SessionsActionType.SessionsLoaded; sessions: SessionListItem[] }
  | { type: SessionsActionType.Progress; progress: IndexProgress }
  | { type: SessionsActionType.Complete; error: string | null }
  | { type: SessionsActionType.RescanStarted }
  | { type: SessionsActionType.ScanFailed; error: string }
  | { type: SessionsActionType.SetSearch; search: string }
  | { type: SessionsActionType.Select; id: string }
  | { type: SessionsActionType.Reset };

export const initialSessionsState: SessionsState = {
  sessions: [],
  indexing: true,
  progress: null,
  error: null,
  search: "",
  selectedId: null,
};

/** Sessions matching harness tokens + residual free-text from `search`. */
export function visibleSessions(state: SessionsState): SessionListItem[] {
  const { harnesses, residual } = parseHarnessFilter(state.search);
  return state.sessions.filter(
    (session) =>
      (harnesses === null || harnesses.includes(session.harness)) &&
      matchesQuery(session, residual),
  );
}

function withSelection(state: SessionsState): SessionsState {
  const visible = visibleSessions(state);
  if (visible.length === 0) {
    return state.selectedId === null ? state : { ...state, selectedId: null };
  }
  if (visible.some((session) => session.id === state.selectedId)) {
    return state;
  }
  return { ...state, selectedId: visible[0]?.id ?? null };
}

export function sessionsReducer(state: SessionsState, action: SessionsAction): SessionsState {
  switch (action.type) {
    case SessionsActionType.SessionsLoaded:
      return withSelection({ ...state, sessions: action.sessions });
    case SessionsActionType.Progress:
      return { ...state, progress: action.progress };
    case SessionsActionType.Complete:
      return { ...state, indexing: false, error: action.error };
    case SessionsActionType.RescanStarted:
      return { ...state, indexing: true, progress: null, error: null };
    case SessionsActionType.ScanFailed:
      return { ...state, indexing: false, error: action.error };
    case SessionsActionType.SetSearch:
      return withSelection({ ...state, search: action.search });
    case SessionsActionType.Select:
      return { ...state, selectedId: action.id };
    case SessionsActionType.Reset:
      return initialSessionsState;
  }
}

function statusError(status: IndexStatus): string | null {
  return typeof status === "object" ? status.error.message : null;
}

interface SessionsStore extends SessionsState {
  dispatch: (action: SessionsAction) => void;
}

export const useSessionsStore = create<SessionsStore>((set) => ({
  ...initialSessionsState,
  dispatch: (action) => set((state) => sessionsReducer(state, action)),
}));

/** Coalesce overlapping list reads so progress bursts cannot apply out of order. */
let listing = false;
let listAgain = false;

export async function pullSessions(): Promise<void> {
  if (listing) {
    listAgain = true;
    return;
  }
  listing = true;
  const { dispatch } = useSessionsStore.getState();
  try {
    do {
      listAgain = false;
      dispatch({ type: SessionsActionType.SessionsLoaded, sessions: await listSessions() });
    } while (listAgain);
  } catch (cause) {
    console.error("Reading the session index failed", cause);
    dispatch({
      type: SessionsActionType.ScanFailed,
      error: `Could not read the session index: ${errorMessage(cause)}`,
    });
  } finally {
    listing = false;
  }
}

async function kickRefresh(): Promise<void> {
  const { dispatch } = useSessionsStore.getState();
  try {
    await startRefresh();
  } catch (cause) {
    console.error("Starting the session scan failed", cause);
    dispatch({
      type: SessionsActionType.ScanFailed,
      error: `Could not start scanning local sessions: ${errorMessage(cause)}`,
    });
  }
}

/** Clears progress/error and starts a fresh background reconcile. */
export async function rescan(): Promise<void> {
  useSessionsStore.getState().dispatch({ type: SessionsActionType.RescanStarted });
  await kickRefresh();
}

/**
 * Subscribes to index events, lists whatever was previously indexed, then
 * starts a refresh. Returns an unsubscribe that drops the event listeners.
 */
export async function startSessionWatch(): Promise<() => void> {
  const { dispatch } = useSessionsStore.getState();
  const unlisten: Array<() => void> = [];

  try {
    unlisten.push(
      await onIndexProgress((progress) => {
        dispatch({ type: SessionsActionType.Progress, progress });
        void pullSessions();
      }),
    );
    unlisten.push(
      await onIndexComplete((status) => {
        dispatch({ type: SessionsActionType.Complete, error: statusError(status) });
        void pullSessions();
      }),
    );
  } catch (cause) {
    for (const off of unlisten) off();
    console.error("Subscribing to index events failed", cause);
    dispatch({
      type: SessionsActionType.ScanFailed,
      error: `Could not connect to the desktop shell: ${errorMessage(cause)}`,
    });
    return () => {};
  }

  await pullSessions();
  await kickRefresh();

  return () => {
    for (const off of unlisten) off();
  };
}

/** Test helper: restore the initial state and listing coalesce flags. */
export function resetSessionsStore(): void {
  listing = false;
  listAgain = false;
  useSessionsStore.getState().dispatch({ type: SessionsActionType.Reset });
}
