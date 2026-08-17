import type { AillyEvent } from "../../tauri";
import { Badge } from "../badges/badge";
import type { BadgeColor } from "../colors";
import { cn } from "../utils";
import { PriceAmount } from "./PriceAmount";
import {
  BUCKET_BAR_CLASS,
  BUCKET_COLOR,
  BUCKET_LABEL,
  BUCKET_ORDER,
  SPEND_COLOR,
  modelsLabel,
  tokenLabel,
  totalLabel,
} from "./palette";
import { settledLabel, settledPrice, subagentNote } from "./presentation";
import { type SessionTokenStats, recordedNothing, spendShare } from "./rollup";
import { type PriceReading, type SessionSpend, totalNoteRedundant } from "./sessionSpend";
import { sessionIdOf, useSessionSpend } from "./useSessionSpend";
import { useSessionTokenStats } from "./useSessionTokenStats";

/**
 * The Summary lens's compact account of the same spend the Tokens lens
 * details in full. It reads through `useSessionTokenStats`, the same
 * descendant-walking hook the Tokens lens itself calls, so the two surfaces
 * can never disagree about one session's spend — this card shows a subset of
 * the same figures, never a second computation of them.
 *
 * Deliberately narrow: session total, orchestrator against subagent, and the
 * four-bucket composition. The content-category breakdown (system prompts,
 * user messages, thinking, tool calls, responses) a user separately asked for
 * is out of scope here — no harness records those token counts, and building
 * it is tracked as a deferred item in this feature's design.
 */
export function TokensSummaryCard({
  events,
  className,
}: { events: AillyEvent[]; className?: string }) {
  const stats = useSessionTokenStats(events);
  const spend = useSessionSpend(sessionIdOf(events), stats.descendantSessionIds);

  if (recordedNothing(stats)) {
    return (
      <fieldset
        aria-label="Tokens"
        className={cn("flex flex-col gap-1.5 rounded-md border px-3 py-2.5", className)}
      >
        <Eyebrow />
        <p className="text-foreground-muted text-xs">This session recorded no token usage.</p>
      </fieldset>
    );
  }

  return (
    <fieldset
      aria-label="Tokens"
      className={cn("flex flex-col gap-3 rounded-md border px-3 py-2.5", className)}
    >
      <Eyebrow />

      <SessionTotal
        stats={stats}
        price={settledPrice(stats, spend.session)}
        spend={spend.session}
      />

      <CompositionBar stats={stats} />

      <div className="flex flex-col gap-1.5">
        <ContributionRow
          label="Orchestrator"
          color={SPEND_COLOR.orchestrator}
          value={tokenLabel(stats.orchestratorSpend)}
          share={spendShare(stats.orchestratorSpend, stats.sessionTotal).label}
          note={modelsLabel(stats.orchestratorModels)}
        />
        <ContributionRow
          label="Subagents"
          color={SPEND_COLOR.subagent}
          value={settledLabel(stats, stats.subagentSpend)}
          share={
            stats.pendingSpawnCount > 0
              ? null
              : spendShare(stats.subagentSpend, stats.sessionTotal).label
          }
          note={combineNotes(subagentNote(stats), modelsLabel(stats.subagentModels))}
        />
      </div>

      <div className="flex flex-col gap-1.5 border-t pt-2">
        {BUCKET_ORDER.map((bucket) => (
          <ContributionRow
            key={bucket}
            label={BUCKET_LABEL[bucket]}
            color={BUCKET_COLOR[bucket]}
            value={tokenLabel(stats.composition[bucket].amount)}
            share={stats.composition[bucket].label}
          />
        ))}
      </div>
    </fieldset>
  );
}

function Eyebrow() {
  return <span className="eyebrow-sm text-foreground-muted">Tokens</span>;
}

/** Joins two optional notes for one row, dropping whichever side is absent. */
function combineNotes(left: string | null, right: string | null): string | null {
  if (left && right) {
    return `${left}; ${right}`;
  }
  return left ?? right;
}

/**
 * What the session spent and what that cost, always together: a token count on
 * its own leaves a reader to guess at rates, and a dollar figure on its own
 * hides which of the two bases it rested on. The eyebrow itself carries that
 * distinction — "Estimated total" or "Recorded total" — rather than a second
 * line repeating the dollar figure already shown above it.
 */
function SessionTotal({
  stats,
  price,
  spend,
}: {
  stats: SessionTokenStats;
  price: PriceReading | null;
  spend: SessionSpend;
}) {
  return (
    <fieldset aria-label="Session total" className="flex flex-col gap-0.5">
      <div className="flex items-baseline gap-2">
        <span className="font-semibold text-2xl text-foreground">
          {settledLabel(stats, stats.sessionTotal)}
        </span>
        {price ? <PriceAmount reading={price} className="font-medium text-base" /> : null}
      </div>
      <span className="eyebrow-sm text-foreground-muted">{totalLabel(price)}</span>
      {!totalNoteRedundant(spend) && price?.basis ? (
        <span className="text-foreground-muted text-[11px]">{price.basis}</span>
      ) : null}
      {modelsLabel(stats.sessionModels) ? (
        <span className="text-foreground-muted text-[11px]">
          {modelsLabel(stats.sessionModels)}
        </span>
      ) : null}
    </fieldset>
  );
}

/**
 * The whole session's spend as one bar, fresh input through cache write in the
 * lens's fixed reading order, each segment's width its live share of the total
 * known so far. Purely decorative: every figure it draws also appears as text
 * in the rows beneath it, so colour is never the only way to read this card.
 */
function CompositionBar({ stats }: { stats: SessionTokenStats }) {
  const total = stats.sessionTotal;
  return (
    <div
      aria-hidden="true"
      className="flex h-2 w-full overflow-hidden rounded-sm bg-background-muted"
    >
      {BUCKET_ORDER.map((bucket) => {
        const amount = stats.composition[bucket].amount;
        const width = total === 0 ? 0 : (amount / total) * 100;
        return width <= 0 ? null : (
          <div key={bucket} className={BUCKET_BAR_CLASS[bucket]} style={{ width: `${width}%` }} />
        );
      })}
    </div>
  );
}

/**
 * One labelled figure with its share of the session total, the row shape every
 * contribution and every composition bucket below it shares — a colour swatch,
 * a label, an amount, and a percentage, so the two groups read as one table.
 */
function ContributionRow({
  label,
  value,
  color,
  share,
  note,
}: {
  label: string;
  value: string;
  color: BadgeColor;
  share: string | null;
  note?: string | null;
}) {
  return (
    <fieldset aria-label={label} className="flex flex-col gap-0.5">
      <div className="flex items-baseline gap-1.5">
        <Badge color={color} textSize="sm" className="w-2 shrink-0 px-0" aria-hidden="true" />
        <span className="min-w-0 flex-1 truncate text-foreground-muted text-xs">{label}</span>
        <span className="shrink-0 font-medium text-foreground text-xs">{value}</span>
        {share === null ? null : (
          <span className="shrink-0 text-foreground-muted text-xs">{share}</span>
        )}
      </div>
      {note ? <span className="pl-3.5 text-foreground-muted text-[11px]">{note}</span> : null}
    </fieldset>
  );
}
