import { Coins } from "lucide-react";
import { type ReactNode, useMemo, useState } from "react";

import { type AillyEvent, isRecorded } from "../../tauri";
import { usePanelTabs } from "../PanelTabs";
import { Badge } from "../badges/badge";
import { LIST_CAP, MoreRow } from "../summary/stats";
import { type LoadState, LoadStatus } from "../useSessionEvents";
import { PriceAmount } from "./PriceAmount";
import { SpendChart } from "./SpendChart";
import { bucketTotal } from "./buckets";
import {
  BUCKET_COLOR,
  BUCKET_LABEL,
  BUCKET_ORDER,
  NOT_LINKABLE,
  SPAWN_FIGURE_COLOR,
  SPEND_COLOR,
  modelsLabel,
  recordedTokenLabel,
  tokenLabel,
  totalLabel,
} from "./palette";
import { settledLabel, settledPrice, subagentNote } from "./presentation";
import {
  type ChildSpendResult,
  type SessionTokenStats,
  type SpawnSpendRow,
  type SpendMoment,
  type SpendMomentKind,
  type SpendReading,
  rankMoments,
  recordedNothing,
  spendSeries,
} from "./rollup";
import { type PriceReading, priceReading, totalNoteRedundant } from "./sessionSpend";
import { sessionIdOf, useSessionSpend } from "./useSessionSpend";
import { useSessionTokenStats } from "./useSessionTokenStats";

/**
 * The budget lens: where one session's tokens went. Orchestrator against
 * subagent, cached against fresh, and which delegation cost the most — each
 * figure derived from what the harness recorded and labelled as absent when it
 * recorded nothing.
 */
export function TokensPane({ state }: { state: LoadState }) {
  return (
    <section
      aria-label="Session token usage"
      className="flex min-h-0 min-w-0 flex-1 flex-col overflow-y-auto overscroll-contain bg-background"
    >
      <TokensBody state={state} />
    </section>
  );
}

function TokensBody({ state }: { state: LoadState }) {
  if (state.status === LoadStatus.Idle) {
    return (
      <Placeholder title="No session selected" hint="Select a session to account for its spend." />
    );
  }
  if (state.status === LoadStatus.Loading) {
    return <Placeholder title="Loading token usage…" hint="Reading the session's events." />;
  }
  if (state.status === LoadStatus.Error) {
    return (
      <p className="mx-6 mt-4 rounded-md border border-status-error px-4 py-3 text-foreground-status-error">
        {state.message}
      </p>
    );
  }
  return <TokensContent events={state.events} />;
}

function TokensContent({ events }: { events: AillyEvent[] }) {
  const stats = useSessionTokenStats(events);
  // Tokens are folded here because the split by party and the composition need
  // per-event detail. Dollars are not: they are read off the indexed session
  // rows, so this lens and the session list can never quote different prices.
  const spend = useSessionSpend(sessionIdOf(events), stats.descendantSessionIds);

  if (recordedNothing(stats)) {
    return (
      <div className="px-6 py-4">
        <p className="text-foreground-muted">This session recorded no token usage.</p>
      </div>
    );
  }

  const sessionPrice = settledPrice(stats, spend.session);
  return (
    <div className="flex flex-col gap-4 px-6 py-4">
      <div className="grid grid-cols-1 divide-x divide-y divide-border rounded-md border sm:grid-cols-3">
        <SpendTile
          label="Orchestrator spend"
          value={tokenLabel(stats.orchestratorSpend)}
          price={priceReading(spend.orchestrator)}
          color={SPEND_COLOR.orchestrator}
          note={coverageNote(stats)}
          models={modelsLabel(stats.orchestratorModels)}
        />
        <SpendTile
          label="Subagent spend"
          value={settledLabel(stats, stats.subagentSpend)}
          price={settledPrice(stats, spend.subagent)}
          color={SPEND_COLOR.subagent}
          note={subagentNote(stats)}
          models={modelsLabel(stats.subagentModels)}
        />
        <SpendTile
          label="Session total"
          displayLabel={totalLabel(sessionPrice)}
          value={settledLabel(stats, stats.sessionTotal)}
          price={sessionPrice}
          note={totalNoteRedundant(spend.session) ? null : (sessionPrice?.basis ?? null)}
          models={modelsLabel(stats.sessionModels)}
        />
      </div>

      <Composition stats={stats} />
      <SpawnList rows={stats.spawnRows} />
      <SpendTrend stats={stats} />
    </div>
  );
}

/**
 * The basis of the orchestrator figure, so a user can see what it rests on
 * rather than trusting it — including how many repeated records it collapsed.
 */
function coverageNote(stats: SessionTokenStats): string | null {
  const { respondedCount, collapsedCount } = stats.coverage;
  const responses = `${respondedCount} orchestrator ${
    respondedCount === 1 ? "response" : "responses"
  } recorded usage`;
  if (collapsedCount === 0) {
    return responses;
  }
  return `${responses}, ${collapsedCount} repeated ${
    collapsedCount === 1 ? "record" : "records"
  } collapsed`;
}

function SpendTile({
  label,
  displayLabel,
  value,
  price,
  color,
  note,
  models,
}: {
  label: string;
  /** What the eyebrow actually reads, when it differs from the tile's fixed accessible name. */
  displayLabel?: string;
  value: string;
  price?: PriceReading | null;
  color?: (typeof SPEND_COLOR)[keyof typeof SPEND_COLOR];
  note?: string | null;
  models?: string | null;
}) {
  return (
    <fieldset aria-label={label} className="flex flex-col gap-0.5 px-3 py-2">
      <div className="flex items-baseline gap-2">
        <span className="font-semibold text-2xl text-foreground">{value}</span>
        {price ? <PriceAmount reading={price} className="font-medium text-base" /> : null}
      </div>
      <span className="eyebrow-sm flex items-center gap-1.5 text-foreground-muted">
        {color ? (
          <Badge color={color} textSize="sm" className="w-2 px-0" aria-hidden="true" />
        ) : null}
        {displayLabel ?? label}
      </span>
      {note === null || note === undefined ? null : (
        <span className="text-foreground-muted text-xs">{note}</span>
      )}
      {models ? <span className="text-foreground-muted text-xs">{models}</span> : null}
    </fieldset>
  );
}

/**
 * Cached against fresh over the session's whole spend. Every bucket comes from
 * the same fold as the Session total above, so the two cannot disagree.
 */
function Composition({ stats }: { stats: SessionTokenStats }) {
  return (
    <Section label="Session token composition">
      <div className="grid grid-cols-2 divide-x divide-y divide-border rounded-md border sm:grid-cols-4">
        {BUCKET_ORDER.map((bucket) => (
          <fieldset
            key={bucket}
            aria-label={BUCKET_LABEL[bucket]}
            className="flex flex-col gap-0.5 px-3 py-2"
          >
            <span className="font-semibold text-foreground text-lg">
              {tokenLabel(stats.composition[bucket].amount)}
            </span>
            <span className="eyebrow-sm text-foreground-muted">{BUCKET_LABEL[bucket]}</span>
            <Badge color={BUCKET_COLOR[bucket]} textSize="sm">
              {stats.composition[bucket].label}
            </Badge>
          </fieldset>
        ))}
      </div>
    </Section>
  );
}

/**
 * Every delegation with both of its numbers. Child spend is the whole subtree's
 * cost; Final context is the harness's own figure for the spawn's last turn.
 * They are different facts, so neither is ever shown in the other's place.
 */
function SpawnList({ rows }: { rows: SpawnSpendRow[] }) {
  if (rows.length === 0) {
    return null;
  }
  return (
    <Section label="Spend by spawn">
      <ul aria-label="Spend by spawn" className="flex flex-col gap-2">
        {rows.map((row) => (
          <SpawnRow key={row.eventId} row={row} />
        ))}
      </ul>
    </Section>
  );
}

function SpawnRow({ row }: { row: SpawnSpendRow }) {
  return (
    <li className="flex flex-col rounded-md border">
      <div className="flex min-w-0 items-baseline gap-2 px-3 py-2">
        <span className="eyebrow-sm shrink-0 text-foreground-muted">
          {isRecorded(row.agentType) ? row.agentType.Recorded : "Agent type not recorded"}
        </span>
        <span
          className={
            isRecorded(row.prompt)
              ? "min-w-0 whitespace-pre-wrap text-foreground"
              : "min-w-0 text-foreground-muted italic"
          }
        >
          {isRecorded(row.prompt) ? row.prompt.Recorded : "Not recorded"}
        </span>
      </div>
      <div className="grid grid-cols-1 divide-x divide-y divide-border border-t sm:grid-cols-4">
        <SpawnField
          label="Outcome"
          value={isRecorded(row.outcome) ? row.outcome.Recorded : "Not recorded"}
        />
        <SpawnField
          label="Child spend"
          value={childSpendLabel(row.childSpend)}
          color={SPAWN_FIGURE_COLOR.childSpend}
        />
        <SpawnField
          label="Final context"
          value={recordedTokenLabel(row.finalContext)}
          color={SPAWN_FIGURE_COLOR.finalContext}
        />
        <SpawnField label="Model" value={spawnModelLabel(row)} />
      </div>
    </li>
  );
}

/**
 * What this delegation's subtree cost, or which of the three reasons no figure
 * can be given. A read still in flight reads as reading, never as zero.
 */
function childSpendLabel(spend: ChildSpendResult): string {
  switch (spend.status) {
    case "resolved":
      return tokenLabel(bucketTotal(spend.buckets));
    case "pending":
      return "Reading…";
    case "unlinkable":
      return NOT_LINKABLE;
    case "failed":
      return spend.message;
  }
}

/**
 * The model(s) the child transcript named, once its spend has resolved.
 * "Reading…" while the child read this depends on is still in flight;
 * "Not recorded" for every outcome that left no child transcript to name one
 * — repeating the child spend column's own shortfall message here would be
 * the same fact stated twice, not a second one.
 */
function spawnModelLabel(row: SpawnSpendRow): string {
  if (row.childSpend.status === "pending") {
    return "Reading…";
  }
  return row.models.length === 0 ? "Not recorded" : row.models.join(", ");
}

function SpawnField({
  label,
  value,
  color,
}: {
  label: string;
  value: string;
  color?: (typeof SPAWN_FIGURE_COLOR)[keyof typeof SPAWN_FIGURE_COLOR];
}) {
  return (
    <fieldset aria-label={label} className="flex flex-col gap-0.5 px-3 py-2">
      <span className="eyebrow-sm flex items-center gap-1.5 text-foreground-muted">
        {color ? (
          <Badge color={color} textSize="sm" className="w-2 px-0" aria-hidden="true" />
        ) : null}
        {label}
      </span>
      <span className="min-w-0 truncate text-foreground">{value}</span>
    </fieldset>
  );
}

/**
 * Where the cost accrued. The chart is the overview; the ranked list beside it
 * is the accessible route to the same facts, and the only assertable one — the
 * chart draws nothing without a real layout to measure.
 */
function SpendTrend({ stats }: { stats: SessionTokenStats }) {
  const [reading, setReading] = useState<SpendReading>("per-response");
  const series = useMemo(() => spendSeries(stats.spendUnits, reading), [stats.spendUnits, reading]);
  const ranked = useMemo(() => rankMoments(stats.moments, LIST_CAP), [stats.moments]);

  return (
    <Section label="Spend by message">
      <div className="flex flex-wrap items-center gap-3">
        <fieldset aria-label="Chart reading" className="flex gap-1.5">
          {READINGS.map((option) => (
            <button
              key={option.value}
              type="button"
              aria-pressed={reading === option.value}
              onClick={() => setReading(option.value)}
              className={
                reading === option.value
                  ? "focus-ring rounded-sm border border-foreground px-2 py-0.5 text-foreground text-xs"
                  : "focus-ring rounded-sm border px-2 py-0.5 text-foreground-muted text-xs hover:bg-background-hover-solid"
              }
            >
              {option.label}
            </button>
          ))}
        </fieldset>
        <Badge color={SPEND_COLOR.orchestrator} textSize="sm">
          Orchestrator
        </Badge>
        <Badge color={SPEND_COLOR.subagent} textSize="sm">
          Subagent
        </Badge>
      </div>

      <SpendChart series={series} reading={reading} />

      <h3 className="eyebrow-sm text-foreground-muted">Top spend moments</h3>
      <ul aria-label="Top spend moments" className="flex flex-col gap-1 rounded-md border">
        {ranked.map((moment) => (
          <MomentRow key={`${moment.kind}-${moment.eventId}`} moment={moment} />
        ))}
      </ul>
      <MoreRow hidden={stats.moments.length - ranked.length} noun="moment" />
    </Section>
  );
}

const READINGS: Array<{ value: SpendReading; label: string }> = [
  { value: "per-response", label: "Per response" },
  { value: "cumulative", label: "Cumulative" },
];

const MOMENT_KIND_LABEL: Record<SpendMomentKind, string> = {
  response: "Assistant response",
  spawn: "Subagent spawn",
};

/**
 * One moment, and the hand-off to the lens that can explain it: a response is
 * read in the transcript, a spawn is read in the child's own activity.
 */
function MomentRow({ moment }: { moment: SpendMoment }) {
  const { navigateTo } = usePanelTabs();
  return (
    <li className="border-b last:border-b-0">
      <button
        type="button"
        onClick={() =>
          navigateTo(moment.kind === "spawn" ? "subagents" : "conversation", moment.eventId)
        }
        className="focus-ring flex w-full min-w-0 items-baseline gap-2 px-3 py-1.5 text-left hover:bg-background-hover-solid"
      >
        <Badge
          color={moment.kind === "spawn" ? SPEND_COLOR.subagent : SPEND_COLOR.orchestrator}
          textSize="sm"
          className="shrink-0"
        >
          {MOMENT_KIND_LABEL[moment.kind]}
        </Badge>
        <span className="min-w-0 truncate text-foreground">{moment.label}</span>
        {moment.models.length === 0 ? null : (
          <span className="shrink-0 truncate text-foreground-muted text-xs">
            {moment.models.join(", ")}
          </span>
        )}
        <span className="ml-auto shrink-0 font-medium text-foreground">
          {tokenLabel(moment.amount)}
        </span>
      </button>
    </li>
  );
}

function Section({ label, children }: { label: string; children: ReactNode }) {
  return (
    <fieldset aria-label={label} className="flex flex-col gap-1.5">
      <h2 className="eyebrow-sm text-foreground-muted">{label}</h2>
      {children}
    </fieldset>
  );
}

function Placeholder({ title, hint }: { title: string; hint: string }) {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-3 px-6 py-16 text-center">
      <div className="flex h-14 w-14 items-center justify-center rounded-2xl bg-background-active text-foreground-muted">
        <Coins size={26} strokeWidth={1.8} />
      </div>
      <h2 className="font-semibold text-foreground-title">{title}</h2>
      <p className="max-w-md text-foreground-muted leading-6">{hint}</p>
    </div>
  );
}
