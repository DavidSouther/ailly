import { tokenLabel } from "./palette";
import type { SessionTokenStats } from "./rollup";
import { type PriceReading, type SessionSpend, priceReading } from "./sessionSpend";

/** Hide totals that still depend on child transcript reads. */
export function settledLabel(stats: SessionTokenStats, value: number): string {
  return stats.pendingSpawnCount > 0 ? "Reading…" : tokenLabel(value);
}

/**
 * A price only once every child read it rests on has settled. A part-read price
 * would be a real dollar figure for a session that is not all here yet, which
 * reads as a total rather than as a partial sum; the tokens beside it already
 * say the read is still going.
 */
export function settledPrice(stats: SessionTokenStats, spend: SessionSpend): PriceReading | null {
  return stats.pendingSpawnCount > 0 ? null : priceReading(spend);
}

/** Explain incomplete subagent attribution consistently on both token surfaces. */
export function subagentNote(stats: SessionTokenStats): string | null {
  if (stats.pendingSpawnCount > 0) {
    return `${stats.pendingSpawnCount} child ${
      stats.pendingSpawnCount === 1 ? "transcript" : "transcripts"
    } still to fold`;
  }
  if (stats.unlinkableSpawnCount === 0) {
    return null;
  }
  return stats.unlinkableSpawnCount === 1
    ? "1 spawn's spend is not linkable"
    : `${stats.unlinkableSpawnCount} spawns' spend is not linkable`;
}
