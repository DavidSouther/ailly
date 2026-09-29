import { BarChart3 } from "lucide-react";
import { type ReactNode, useEffect, useMemo, useState } from "react";

import {
  type AillyEvent,
  type SourceValue,
  errorMessage,
  getEventPage,
  isRecorded,
} from "../../tauri";
import { Badge } from "../badges/badge";
import { loadDescendantEvents, namedChildSessionIds } from "../subagents/descendants";
import { TokensSummaryCard } from "../tokens/TokensSummaryCard";
import { type LoadState, LoadStatus } from "../useSessionEvents";
import { type SessionSummaryStats, summarizeSession } from "./rollup";
import {
  CATEGORY_COLOR,
  CATEGORY_LABEL,
  CallsByTool,
  FilesystemList,
  SectionHeading,
  StatTile,
  recordedLabel,
} from "./stats";

/**
 * Renders summary statistics and tool activity for one session. Missing source
 * values remain explicitly unrecorded.
 */
export function SummaryPane({
  state,
  project,
}: { state: LoadState; project: SourceValue<string> }) {
  return (
    <section
      aria-label="Session summary"
      className="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto overscroll-contain bg-background"
    >
      <SummaryBody state={state} project={project} />
    </section>
  );
}

function SummaryBody({ state, project }: { state: LoadState; project: SourceValue<string> }) {
  if (state.status === LoadStatus.Idle) {
    return <Placeholder title="No session selected" hint="Select a session to summarize it." />;
  }
  if (state.status === LoadStatus.Loading) {
    return <Placeholder title="Loading summary…" hint="Reading the session's events." />;
  }
  if (state.status === LoadStatus.Error) {
    return (
      <p className="mx-6 mt-4 rounded-md border border-status-error px-4 py-3 text-foreground-status-error">
        {state.message}
      </p>
    );
  }
  return <SummaryContent events={state.events} project={project} />;
}

function SummaryContent({
  events,
  project,
}: { events: AillyEvent[]; project: SourceValue<string> }) {
  const stats = useMemo(() => summarizeSession(events), [events]);
  const canIncludeSubagentTools = namedChildSessionIds(events).length > 0;
  const [includeSubagentTools, setIncludeSubagentTools] = useState(false);
  const descendants = useDescendantEvents(events, includeSubagentTools && canIncludeSubagentTools);
  // Every breakdown below the tiles reads from this fold, so the category split
  // and the Tools list always describe the same set of calls.
  const breakdown = useMemo(
    () => statsIncludingDescendants(stats, events, descendants),
    [stats, events, descendants],
  );

  return (
    <div className="flex flex-col gap-4 px-6 py-4">
      <div className="flex flex-col gap-3 sm:flex-row sm:items-stretch">
        <div className="grid grid-cols-2 divide-x divide-y divide-border rounded-md border sm:flex-1">
          <StatTile label="Tool calls" value={String(stats.toolCallCount)} />
          <StatTile label="Files touched" value={String(stats.filesTouchedCount)} />
          <StatTile label="Duration" value={recordedLabel(stats.duration, (value) => value)} />
          <SubagentSpawnsTile
            value={subagentSpawnsLabel(stats.subagentSpawnCount)}
            showToggle={canIncludeSubagentTools}
            includeSubagentTools={includeSubagentTools}
            onIncludeSubagentToolsChange={setIncludeSubagentTools}
          />
        </div>
        <TokensSummaryCard events={events} className="sm:w-64 sm:shrink-0" />
      </div>

      <fieldset
        aria-label="Working directory"
        className="flex items-baseline gap-2 rounded-md border px-3 py-2"
      >
        <span className="eyebrow-sm shrink-0 text-foreground-muted">Working directory</span>
        <span className="min-w-0 truncate font-mono text-foreground text-xs">
          {recordedLabel(project, (value) => value)}
        </span>
      </fieldset>

      {breakdown.toolCallCount === 0 && descendants.status !== LoadStatus.Loading ? (
        <p className="text-foreground-muted">This session recorded no tool calls.</p>
      ) : (
        <>
          <CategorySplit stats={breakdown} />
          {breakdown.fileAccesses.length === 0 ? null : (
            // Keyed by session: the filter and chips are a view of one
            // session's paths, so they must not narrow the next one's.
            <FilesystemList key={events[0]?.session_id} files={breakdown.fileAccesses} />
          )}
          {descendants.status === LoadStatus.Loading ? (
            <p className="text-foreground-muted">Loading subagent tool calls…</p>
          ) : null}
          {descendants.status === LoadStatus.Error ? (
            <p className="rounded-md border border-status-error px-4 py-3 text-foreground-status-error">
              {descendants.message}
            </p>
          ) : null}
          <CallsByTool tools={breakdown.toolsByFrequency} project={project} />
        </>
      )}
    </div>
  );
}

/** Zero spawns read as "0" — the session recorded no delegations, not an unknown. */
function subagentSpawnsLabel(value: SourceValue<number>): string {
  return isRecorded(value) ? String(value.Recorded) : "0";
}

/**
 * The Subagent Spawns tile, with an optional toggle that pulls every descendant
 * session's tools into the Tools list. The tile's count stays parent-only —
 * including children changes the breakdown, not how many times this session
 * itself delegated.
 */
function SubagentSpawnsTile({
  value,
  showToggle,
  includeSubagentTools,
  onIncludeSubagentToolsChange,
}: {
  value: string;
  showToggle: boolean;
  includeSubagentTools: boolean;
  onIncludeSubagentToolsChange: (value: boolean) => void;
}) {
  return (
    <fieldset aria-label="Subagent spawns" className="flex flex-col gap-0.5 px-3 py-2">
      <span className="font-semibold text-2xl text-foreground">{value}</span>
      <span className="eyebrow-sm text-foreground-muted">Subagent spawns</span>
      {showToggle ? (
        <label className="mt-1 flex cursor-pointer items-start gap-1.5 text-foreground-muted text-xs leading-snug">
          <input
            type="checkbox"
            className="mt-0.5"
            checked={includeSubagentTools}
            onChange={(event) => onIncludeSubagentToolsChange(event.target.checked)}
          />
          <span>Include subagent tools</span>
        </label>
      ) : null}
    </fieldset>
  );
}

/**
 * The fold every breakdown reads from: the parent's own when subagent tools are
 * excluded, or one over the parent plus every descendant when they are included.
 */
function statsIncludingDescendants(
  parent: SessionSummaryStats,
  parentEvents: AillyEvent[],
  descendants: DescendantLoad,
): SessionSummaryStats {
  if (descendants.status !== LoadStatus.Ready || descendants.events.length === 0) {
    return parent;
  }
  return summarizeSession([...parentEvents, ...descendants.events]);
}

type DescendantLoad =
  | { status: LoadStatus.Idle }
  | { status: LoadStatus.Loading }
  | { status: LoadStatus.Error; message: string }
  | { status: LoadStatus.Ready; events: AillyEvent[] };

/**
 * One recursive walk of every named child session, performed when the summary
 * asks to include subagent tools and held until the parent events change.
 */
function useDescendantEvents(rootEvents: AillyEvent[], enabled: boolean): DescendantLoad {
  const [state, setState] = useState<DescendantLoad>({ status: LoadStatus.Idle });

  useEffect(() => {
    if (!enabled) {
      setState({ status: LoadStatus.Idle });
      return;
    }
    let cancelled = false;
    setState({ status: LoadStatus.Loading });
    loadDescendantEvents(rootEvents, getEventPage)
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
  }, [rootEvents, enabled]);

  return state;
}

function CategorySplit({ stats }: { stats: SessionSummaryStats }) {
  return (
    <Section label="Tool category split">
      <div className="flex flex-wrap gap-1.5">
        {stats.categories.map((total) => (
          <Badge key={total.category} color={CATEGORY_COLOR[total.category]} textSize="sm">
            {CATEGORY_LABEL[total.category]} — {total.share}%
          </Badge>
        ))}
        {stats.unclassified.count === 0 ? null : (
          <Badge color={CATEGORY_COLOR.unclassified} textSize="sm">
            {CATEGORY_LABEL.unclassified} — {stats.unclassified.share}%
          </Badge>
        )}
      </div>
    </Section>
  );
}

function Section({ label, children }: { label: string; children: ReactNode }) {
  return (
    <fieldset aria-label={label} className="flex flex-col gap-1.5">
      <SectionHeading>{label}</SectionHeading>
      {children}
    </fieldset>
  );
}

function Placeholder({ title, hint }: { title: string; hint: string }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 px-6 py-16 text-center">
      <div className="flex h-14 w-14 items-center justify-center rounded-2xl bg-background-active text-foreground-muted">
        <BarChart3 size={26} strokeWidth={1.8} />
      </div>
      <h2 className="font-semibold text-foreground-title">{title}</h2>
      <p className="max-w-md text-foreground-muted leading-6">{hint}</p>
    </div>
  );
}
