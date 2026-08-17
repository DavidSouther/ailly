import { type SourceValue, isRecorded } from "../../tauri";
import { BadgeColor } from "../colors";
import type { TokenBuckets } from "./buckets";

/**
 * One palette for every surface in this lens that shows spend, so a reader
 * learns the colours once: the headline tiles, the composition, the per-spawn
 * columns, the chart's stacked series, and the ranked moments.
 *
 * Colour is never the sole carrier. Every figure keeps its own text label, so
 * the lens reads correctly in monochrome and to a screen reader; changing these
 * values touches styling only.
 */

/** The two halves of the session's spend, wherever they appear together. */
export const SPEND_COLOR = {
  orchestrator: BadgeColor.SKY,
  subagent: BadgeColor.PLUM,
} as const;

/** Hex equivalents of the two series, for the chart, which cannot take classes. */
export const SPEND_STROKE = {
  orchestrator: "#0284c7",
  subagent: "#9333ea",
} as const;

/**
 * The two readings of one delegation. A clearly different pairing from the two
 * above, because Child spend and Final context are not two halves of a whole.
 */
export const SPAWN_FIGURE_COLOR = {
  childSpend: BadgeColor.AMBER,
  finalContext: BadgeColor.MINT,
} as const;

export const BUCKET_LABEL: Record<keyof TokenBuckets, string> = {
  freshInput: "Fresh input",
  output: "Output",
  cacheRead: "Cache read",
  cacheWrite: "Cache write",
};

export const BUCKET_COLOR: Record<keyof TokenBuckets, BadgeColor> = {
  freshInput: BadgeColor.SKY,
  output: BadgeColor.PLUM,
  cacheRead: BadgeColor.MINT,
  cacheWrite: BadgeColor.AMBER,
};

/**
 * The same four hues as `BUCKET_COLOR`, as a solid fill rather than a badge's
 * pale background — for the one place this lens draws a bar instead of a
 * label: the Summary card's segmented composition bar. Kept beside
 * `BUCKET_COLOR` rather than computed from it, since a bar segment and a badge
 * ask a colour to do a different job (fill an area versus tint a chip).
 */
export const BUCKET_BAR_CLASS: Record<keyof TokenBuckets, string> = {
  freshInput: "bg-sky-400",
  output: "bg-purple-400",
  cacheRead: "bg-emerald-400",
  cacheWrite: "bg-amber-400",
};

/** The order the composition reads in: fresh context first, reused context last. */
export const BUCKET_ORDER: Array<keyof TokenBuckets> = [
  "freshInput",
  "output",
  "cacheRead",
  "cacheWrite",
];

/** One form for every token figure this lens shows. */
export function tokenLabel(value: number): string {
  return value.toLocaleString();
}

/**
 * A spawn whose source named no child transcript. Deliberately not
 * `recordedLabel`'s "Not recorded": the spend is real and the harness simply
 * left no way to reach it, which is a different absence from a field nobody
 * wrote.
 */
export const NOT_LINKABLE = "Not linkable";

/** A recorded token figure, or "Not recorded" — never a substituted zero. */
export function recordedTokenLabel(value: SourceValue<number>): string {
  return isRecorded(value) ? tokenLabel(value.Recorded) : "Not recorded";
}

/**
 * The session total's own label, carrying whether it rests on a recorded
 * price or an estimate — the fact the basis sentence used to spell out —
 * without a second line repeating the dollar figure already shown above it.
 */
export function totalLabel(price: { label: string; approximate: boolean } | null): string {
  if (price === null || price.label === "Not recorded") {
    return "Session total";
  }
  return price.approximate ? "Estimated total" : "Recorded total";
}

/**
 * Which model (or models) a figure's spend was run on, in the one phrasing
 * every surface in this lens uses. Empty rather than "Not recorded", since a
 * figure with no named model simply has nothing to say here — it is not a
 * gap the way an absent token count is.
 */
export function modelsLabel(models: string[]): string | null {
  if (models.length === 0) {
    return null;
  }
  return models.length === 1 ? `Model: ${models[0]}` : `Models: ${models.join(", ")}`;
}
