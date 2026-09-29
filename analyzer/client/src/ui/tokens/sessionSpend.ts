import { type SessionTokenFigures, type SourceValue, isRecorded } from "../../tauri";

/**
 * What a set of sessions cost, kept in the two bases a dollar figure can rest
 * on plus the part nobody could price.
 *
 * The two bases are never merged into one number here. A price a harness
 * charged is a fact about the session; a price the indexer multiplied out of a
 * pinned rate table is arithmetic, and stating the second as the first is the
 * same fabrication the token buckets refuse. The split survives all the way to
 * the surfaces so a reader is told which they are looking at.
 *
 * Every figure comes off a session row. Nothing here folds an event page and
 * nothing here knows a rate: nudge either of those back in and the index and
 * the UI can start disagreeing about what one session cost.
 *
 * Amounts are millionths of a dollar because that is how Pi's fractional cents
 * reach the index, and because a session that cost fifteen millionths of a
 * dollar should not sum as zero over a hundred of them.
 */
export interface SessionSpend {
  /** Millionths of a dollar the harnesses charged, over the rows that said so. */
  recordedMicros: number;
  /** Millionths of a dollar the index estimated, over the rows it could estimate. */
  estimatedMicros: number;
  recordedCount: number;
  estimatedCount: number;
  /** Rows that spent tokens the index could put no price against at all. */
  unpricedCount: number;
  /**
   * The distinct catalog dates the estimated part was priced against, earliest
   * first.
   *
   * A list of dates rather than one, because a sum can span sessions the index
   * priced weeks apart and each keeps the rates it was given. Empty when
   * nothing was estimated, or when the rows that were predate the index storing
   * the date.
   */
  estimatedAsOf: string[];
}

export const NO_SPEND: SessionSpend = {
  recordedMicros: 0,
  estimatedMicros: 0,
  recordedCount: 0,
  estimatedCount: 0,
  unpricedCount: 0,
  estimatedAsOf: [],
};

function recorded(value: SourceValue<number>): number | null {
  return isRecorded(value) ? value.Recorded : null;
}

function withDate(dates: string[], value: SourceValue<string>): string[] {
  if (!isRecorded(value) || dates.includes(value.Recorded)) {
    return dates;
  }
  return [...dates, value.Recorded].sort();
}

/**
 * The best token figure a row can give: what the harness totalled, else the
 * sum the index derived to price it, else nothing.
 *
 * Both are recorded facts about the session — the second is the four disjoint
 * buckets added up rather than a rate applied — so preferring one over the
 * other is a question of which the source stated more directly, not of
 * confidence.
 */
export function rowTokens(row: SessionTokenFigures): number | null {
  return recorded(row.token_total) ?? recorded(row.estimated_tokens);
}

/**
 * One session row folded into a running account, in the order the design
 * settles: what the harness charged, else what the index estimated, else
 * nothing.
 *
 * The third outcome is not zero dollars. A row nobody could price may still
 * have spent tokens, so it is counted as unpriced and the surfaces say how much
 * of the spend the price covers rather than quietly reporting a total that is
 * missing a part of it.
 */
function addRow(spend: SessionSpend, row: SessionTokenFigures): SessionSpend {
  const recordedPrice = recorded(row.recorded_price_micros);
  if (recordedPrice !== null) {
    return {
      ...spend,
      recordedMicros: spend.recordedMicros + recordedPrice,
      recordedCount: spend.recordedCount + 1,
    };
  }
  const estimatedPrice = recorded(row.estimated_price_micros);
  if (estimatedPrice !== null) {
    return {
      ...spend,
      estimatedMicros: spend.estimatedMicros + estimatedPrice,
      estimatedCount: spend.estimatedCount + 1,
      estimatedAsOf: withDate(spend.estimatedAsOf, row.estimated_as_of),
    };
  }
  // A row with no tokens either recorded nothing at all, which is not a
  // shortfall in the price — it is a session with nothing to price.
  return rowTokens(row) === null ? spend : { ...spend, unpricedCount: spend.unpricedCount + 1 };
}

/** The spend of a set of session rows, each counted once. */
export function foldSessionSpend(rows: SessionTokenFigures[]): SessionSpend {
  return rows.reduce(addRow, NO_SPEND);
}

/**
 * A dollar amount at the precision that keeps it readable without inventing
 * digits it does not have. Sub-cent spend is the common case for a short
 * session, so a fixed two decimals would show most of this lens as `$0.00`.
 */
export function usdLabel(micros: number): string {
  const usd = micros / 1_000_000;
  if (usd === 0) {
    return "$0.00";
  }
  // The same rule the composition's "<1%" follows: a cost too small to print is
  // still a cost, and showing it as zero would round a real figure out of
  // existence.
  if (usd < 0.000_001) {
    return "<$0.000001";
  }
  if (usd >= 1) {
    return `$${usd.toLocaleString(undefined, {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    })}`;
  }
  return usd >= 0.01 ? `$${usd.toFixed(4)}` : `$${usd.toFixed(6)}`;
}

/**
 * Whether a session total's basis sentence would only repeat what its own
 * eyebrow already says.
 *
 * A total priced by exactly one basis, with nothing left unpriced, says
 * nothing the "Recorded total" / "Estimated total" label and the amount above
 * it have not already said — a note there would just restate the same dollar
 * figure. A total that mixes bases, or that left part of the spend unpriced,
 * still has something the label alone cannot: how much of it rests on which
 * basis, or how much the price does not cover.
 */
export function totalNoteRedundant(spend: SessionSpend): boolean {
  const recordedPresent = spend.recordedCount > 0;
  const estimatedPresent = spend.estimatedCount > 0;
  return spend.unpricedCount === 0 && recordedPresent !== estimatedPresent;
}

/**
 * A price as a surface is allowed to state it: the amount, and the sentence
 * that says where it came from.
 *
 * `approximate` is what carries the distinction into the amount itself, so a
 * reader who never reaches the basis note is still not told an estimate is a
 * receipt.
 */
export interface PriceReading {
  label: string;
  approximate: boolean;
  basis: string | null;
}

function sessionsClause(count: number): string {
  return count === 1 ? "1 session" : `${count} sessions`;
}

/**
 * How old the rates behind an estimate are, in the fewest words the row can
 * support.
 *
 * An estimate is only as good as the day it was priced, and the index can have
 * refreshed its rate table many times since. A sum priced across several dates
 * names its span rather than picking one of them, since neither end alone is
 * true of the whole figure.
 */
function estimatedDateSpan(dates: string[]): string | null {
  const earliest = dates[0];
  const latest = dates[dates.length - 1];
  if (earliest === undefined || latest === undefined) {
    return null;
  }
  return earliest === latest ? earliest : `${earliest} to ${latest}`;
}

/**
 * How a spend reads, given what could and could not be priced.
 *
 * Nothing priced reads "Not recorded" rather than `$0.00`: zero dollars is a
 * claim about the spend, and the honest claim here is that neither the harness
 * nor the index could say. A spend priced only in part still shows what is
 * known, with the shortfall named in the basis rather than folded silently into
 * the amount.
 */
export function priceReading(spend: SessionSpend): PriceReading {
  const priced = spend.recordedCount + spend.estimatedCount;
  if (priced === 0) {
    return {
      label: "Not recorded",
      approximate: false,
      basis:
        spend.unpricedCount === 0 ? null : `no price for ${sessionsClause(spend.unpricedCount)}`,
    };
  }

  const parts: string[] = [];
  if (spend.recordedCount > 0) {
    parts.push(`${usdLabel(spend.recordedMicros)} (Recorded)`);
  }
  if (spend.estimatedCount > 0) {
    const asOf = estimatedDateSpan(spend.estimatedAsOf);
    parts.push(`${usdLabel(spend.estimatedMicros)} (Est${asOf === null ? "" : `, ${asOf}`})`);
  }
  if (spend.unpricedCount > 0) {
    parts.push(`no price for ${sessionsClause(spend.unpricedCount)}`);
  }
  const estimated = spend.estimatedCount > 0;
  return {
    label: `${estimated ? "≈" : ""}${usdLabel(spend.recordedMicros + spend.estimatedMicros)}`,
    approximate: estimated,
    basis: parts.join("; "),
  };
}
