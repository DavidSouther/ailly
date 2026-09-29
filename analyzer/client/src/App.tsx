import { RefreshCw, Search, SearchX } from "lucide-react";
import { useEffect } from "react";
import { useShallow } from "zustand/react/shallow";

import { SessionPane } from "./ui/SessionPane";
import { HarnessFilterChip } from "./ui/sessions/HarnessFilterChip";
import { SessionList } from "./ui/sessions/SessionList";
import { parseSearchQuery, removeHarnessTokenAt } from "./ui/sessions/searchQuery";
import {
  SessionsActionType,
  rescan,
  startSessionWatch,
  useSessionsStore,
  visibleSessions,
} from "./ui/sessions/store";

export function App() {
  const { sessions, indexing, progress, error, search, selectedId, dispatch } = useSessionsStore(
    useShallow((state) => ({
      sessions: state.sessions,
      indexing: state.indexing,
      progress: state.progress,
      error: state.error,
      search: state.search,
      selectedId: state.selectedId,
      dispatch: state.dispatch,
    })),
  );
  const visible = useSessionsStore(useShallow(visibleSessions));
  const { harnessValues } = parseSearchQuery(search);

  useEffect(() => {
    let cancelled = false;
    let stop: (() => void) | undefined;
    void startSessionWatch().then((unsubscribe) => {
      if (cancelled) {
        unsubscribe();
        return;
      }
      stop = unsubscribe;
    });
    return () => {
      cancelled = true;
      stop?.();
    };
  }, []);

  const progressLabel =
    progress && progress.total > 0
      ? `Scanning ${progress.indexed} of ${progress.total} sources…`
      : "Scanning local sessions…";

  return (
    <main className="flex h-screen flex-col overflow-hidden bg-background text-foreground">
      <header className="flex shrink-0 items-center justify-between gap-4 border-b px-6 py-4">
        <div className="flex flex-col gap-0.5">
          <p className="eyebrow-sm text-foreground-muted">Ailly Analyzer</p>
          <h1 className="font-semibold text-foreground-title text-xl tracking-tight">Sessions</h1>
        </div>
      </header>

      <div className="flex shrink-0 flex-col gap-2 border-b px-6 py-3">
        <div className="flex items-center gap-3">
          <div className="relative flex-1">
            <Search
              size={14}
              className="-translate-y-1/2 absolute top-1/2 left-2.5 text-foreground-muted"
            />
            <input
              type="search"
              value={search}
              onChange={(event) =>
                dispatch({ type: SessionsActionType.SetSearch, search: event.target.value })
              }
              placeholder="Filter by project, id, or harness"
              aria-label="Filter sessions"
              className="w-full rounded-md border bg-input-background py-1.5 pr-3 pl-8 placeholder:text-placeholder focus-ring"
            />
          </div>
        </div>
        {harnessValues.length > 0 && (
          <div className="flex flex-wrap items-center gap-2">
            {harnessValues.map((value, index) => (
              <HarnessFilterChip
                key={`${value}-${index}`}
                label={`harness: ${value}`}
                onRemove={() =>
                  dispatch({
                    type: SessionsActionType.SetSearch,
                    search: removeHarnessTokenAt(search, index),
                  })
                }
              />
            ))}
          </div>
        )}
      </div>

      <div className="flex min-h-0 flex-1">
        <section
          className="flex min-h-0 w-[26rem] shrink-0 flex-col overflow-y-auto overscroll-contain border-r"
          aria-label="Sessions panel"
        >
          <div className="flex shrink-0 items-center justify-between gap-3 px-6 pt-3 pb-1">
            <p className="text-foreground-muted">
              {sessionsPanelSummary(sessions.length, visible.length, indexing, progressLabel)}
            </p>
            <button
              type="button"
              onClick={() => void rescan()}
              disabled={indexing}
              aria-busy={indexing}
              className="flex items-center gap-2 rounded-md border px-3 py-1.5 font-medium hover:bg-background-hover-solid disabled:opacity-60"
            >
              <RefreshCw size={14} className={indexing ? "animate-spin" : undefined} />
              {indexing ? "Scanning…" : "Rescan"}
            </button>
          </div>

          {error && (
            <p className="mx-6 mt-3 rounded-md border border-status-error px-4 py-3 text-foreground-status-error">
              {error}
            </p>
          )}

          {sessions.length === 0 &&
            (indexing ? (
              <EmptyState
                title="Looking for sessions"
                hint={`${progressLabel} Claude Code, Codex, and Pi folders under your home directory are scanned; sessions appear here as they are indexed.`}
              />
            ) : (
              <EmptyState
                title="No sessions found"
                hint="Ailly Analyzer scans Claude Code, Codex, and Pi session folders under your home directory. Run an agent session, then Rescan."
              />
            ))}

          {sessions.length > 0 && visible.length === 0 && (
            <EmptyState
              title="No sessions match your filters"
              hint="Clear the search box or remove a harness filter."
            />
          )}

          {visible.length > 0 && (
            <SessionList
              sessions={visible}
              selectedId={selectedId}
              onSelect={(id) => dispatch({ type: SessionsActionType.Select, id })}
            />
          )}
        </section>

        <SessionPane
          sessionId={selectedId}
          project={sessions.find((session) => session.id === selectedId)?.project ?? "Absent"}
        />
      </div>
    </main>
  );
}

function sessionsPanelSummary(
  sessionsCount: number,
  visibleCount: number,
  indexing: boolean,
  progressLabel: string,
): string {
  if (sessionsCount === 0) {
    return indexing ? progressLabel : "Sessions";
  }
  const count = `${visibleCount} of ${sessionsCount} sessions`;
  return indexing ? `${count} · ${progressLabel}` : count;
}

function EmptyState({ title, hint }: { title: string; hint: string }) {
  return (
    <div className="flex flex-col items-center gap-3 px-6 py-16 text-center">
      <div className="flex h-14 w-14 items-center justify-center rounded-2xl bg-background-active text-foreground-muted">
        <SearchX size={26} strokeWidth={1.8} />
      </div>
      <h2 className="font-semibold text-foreground-title">{title}</h2>
      <p className="max-w-md text-foreground-muted leading-6">{hint}</p>
    </div>
  );
}
