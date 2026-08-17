import { Bot, ChevronDown, ChevronRight } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

import {
  type AillyEvent,
  type SourceValue,
  errorMessage,
  getEventPage,
  isRecorded,
} from "../../tauri";
import { SessionLenses } from "../SessionLenses";
import { eventAnchorId } from "../conversation/format";
import { recordedLabel } from "../summary/stats";
import { useLandingTarget } from "../useLandingTarget";
import { type LoadState, LoadStatus } from "../useSessionEvents";
import { type SubagentSpawnRow, promptPreview, subagentSpawnRows } from "./rollup";

/**
 * The delegation lens: every spawn this session recorded, with only the
 * dimensions its harness actually wrote. Expanding a spawn whose source named
 * a child opens that child in the same Summary / Conversation / Subagents
 * interface the top-level session uses.
 */
export function SubagentsPane({
  state,
  project = "Absent",
}: { state: LoadState; project?: SourceValue<string> }) {
  return (
    <section
      aria-label="Session subagents"
      className="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto overscroll-contain bg-background"
    >
      <SubagentsBody state={state} project={project} />
    </section>
  );
}

function SubagentsBody({ state, project }: { state: LoadState; project: SourceValue<string> }) {
  if (state.status === LoadStatus.Idle) {
    return (
      <Placeholder title="No session selected" hint="Select a session to see what it delegated." />
    );
  }
  if (state.status === LoadStatus.Loading) {
    return <Placeholder title="Loading subagents…" hint="Reading the session's events." />;
  }
  if (state.status === LoadStatus.Error) {
    return <ErrorNote message={state.message} />;
  }
  return <SubagentsContent events={state.events} project={project} />;
}

function SubagentsContent({
  events,
  project,
}: { events: AillyEvent[]; project: SourceValue<string> }) {
  const rows = useMemo(() => subagentSpawnRows(events), [events]);

  if (rows.length === 0) {
    return (
      <div className="px-6 py-4">
        <p className="text-foreground-muted">This session recorded no subagent spawns.</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-4 px-6 py-4">
      <ul aria-label="Subagent spawns" className="flex flex-col gap-2">
        {rows.map((row) => (
          <SubagentRow key={row.eventId} row={row} project={project} />
        ))}
      </ul>
    </div>
  );
}

function SubagentRow({ row, project }: { row: SubagentSpawnRow; project: SourceValue<string> }) {
  const [open, setOpen] = useState(false);
  const { ref, landed } = useLandingTarget<HTMLLIElement>(row.eventId);
  const childSessionId = isRecorded(row.childSessionId) ? row.childSessionId.Recorded : null;
  // A user handed here from another lens came for the child's own activity, not
  // for the parent's one-line mention of it, so the row opens itself — and can
  // still be closed again, which is why this sets the same state a click does.
  useEffect(() => {
    if (landed) {
      setOpen(true);
    }
  }, [landed]);
  const child = useChildSessionEvents(open ? childSessionId : null);
  const fullPrompt = recordedLabel(row.prompt, (prompt) => prompt);
  const visiblePrompt =
    open || !isRecorded(row.prompt) ? fullPrompt : promptPreview(row.prompt.Recorded);

  return (
    <li
      ref={ref}
      id={eventAnchorId(row.eventId)}
      tabIndex={landed ? -1 : undefined}
      aria-current={landed ? "location" : undefined}
      className={
        landed
          ? "focus-ring flex flex-col rounded-md border ring-2 ring-foreground"
          : "flex flex-col rounded-md border"
      }
    >
      <button
        type="button"
        aria-expanded={open}
        aria-label={fullPrompt}
        onClick={() => setOpen((value) => !value)}
        className="focus-ring flex w-full min-w-0 items-start gap-2 rounded-t-md px-3 py-2 text-left hover:bg-background-hover-solid"
      >
        {open ? (
          <ChevronDown size={14} className="mt-0.5 shrink-0 text-foreground-muted" />
        ) : (
          <ChevronRight size={14} className="mt-0.5 shrink-0 text-foreground-muted" />
        )}
        <Bot size={14} className="mt-0.5 shrink-0 text-foreground-muted" />
        <span
          className={
            isRecorded(row.prompt)
              ? "min-w-0 whitespace-pre-wrap text-foreground"
              : "min-w-0 text-foreground-muted italic"
          }
        >
          {visiblePrompt}
        </span>
      </button>

      <div className="grid grid-cols-2 divide-x divide-y divide-border border-t sm:grid-cols-4">
        <Field label="Agent type" value={recordedLabel(row.agentType, (type) => type)} />
        <Field label="Duration" value={recordedLabel(row.durationLabel, (label) => label)} />
        <Field label="Outcome" value={recordedLabel(row.outcome, (outcome) => outcome)} />
        <Field
          label="Final context"
          value={recordedLabel(row.finalContextLabel, (label) => label)}
        />
      </div>

      {open ? (
        <div className="min-h-0 border-t">
          {childSessionId === null ? (
            <p className="px-3 py-2 text-foreground-muted">Child transcript not recorded</p>
          ) : (
            <ChildSession state={child} project={project} />
          )}
        </div>
      ) : null}
    </li>
  );
}

/**
 * The child session in the same three-lens interface as the parent. A failed
 * or still-loading read stays in place so a broken child does not collapse the
 * rest of the spawn list.
 */
function ChildSession({ state, project }: { state: LoadState; project: SourceValue<string> }) {
  if (state.status === LoadStatus.Error) {
    return (
      <div className="px-3 py-2">
        <ErrorNote message={state.message} />
      </div>
    );
  }
  if (state.status !== LoadStatus.Ready) {
    return <p className="px-3 py-2 text-foreground-muted">Loading the child transcript…</p>;
  }
  return <NestedSession state={state} project={project} />;
}

/**
 * The child session, mounted a frame after the row that holds it.
 *
 * A spawn row can contain an entire session, whose own spawn rows can contain
 * another, so expanding one row can commit an arbitrarily deep tree. Painting
 * the row first keeps the expansion responsive, and keeps a programmatic
 * hand-off from another lens from landing a user on a row that is still
 * building underneath them.
 */
function NestedSession({ state, project }: { state: LoadState; project: SourceValue<string> }) {
  const [mounted, setMounted] = useState(false);

  useEffect(() => {
    const frame = requestAnimationFrame(() => setMounted(true));
    return () => cancelAnimationFrame(frame);
  }, []);

  if (!mounted) {
    return <p className="px-3 py-2 text-foreground-muted">Opening the child session…</p>;
  }
  return (
    <section aria-label="Subagent session" className="min-h-[24rem]">
      <SessionLenses state={state} project={project} />
    </section>
  );
}

/**
 * One bounded read of a child session's events, performed on first expansion
 * and never repeated: a row the user opens, closes, and reopens costs exactly
 * one round trip.
 */
function useChildSessionEvents(childSessionId: string | null): LoadState {
  const [state, setState] = useState<LoadState>({ status: LoadStatus.Idle });
  const requested = useRef<string | null>(null);

  useEffect(() => {
    if (childSessionId === null || requested.current === childSessionId) {
      return;
    }
    requested.current = childSessionId;
    let cancelled = false;
    setState({ status: LoadStatus.Loading });
    getEventPage(childSessionId)
      .then((events) => {
        if (!cancelled) {
          setState({ status: LoadStatus.Ready, events });
        }
      })
      .catch((cause: unknown) => {
        if (!cancelled) {
          setState({ status: LoadStatus.Error, message: errorMessage(cause) });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [childSessionId]);

  return state;
}

function Field({ label, value }: { label: string; value: string }) {
  return (
    <fieldset aria-label={label} className="flex flex-col gap-0.5 px-3 py-2">
      <span className="eyebrow-sm text-foreground-muted">{label}</span>
      <span className="min-w-0 truncate text-foreground">{value}</span>
    </fieldset>
  );
}

function ErrorNote({ message }: { message: string }) {
  return (
    <p className="rounded-md border border-status-error px-4 py-3 text-foreground-status-error">
      {message}
    </p>
  );
}

function Placeholder({ title, hint }: { title: string; hint: string }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 px-6 py-16 text-center">
      <div className="flex h-14 w-14 items-center justify-center rounded-2xl bg-background-active text-foreground-muted">
        <Bot size={26} strokeWidth={1.8} />
      </div>
      <h2 className="font-semibold text-foreground-title">{title}</h2>
      <p className="max-w-md text-foreground-muted leading-6">{hint}</p>
    </div>
  );
}
