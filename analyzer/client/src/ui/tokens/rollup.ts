import {
  type AillyEvent,
  EventKind,
  type SourceValue,
  type Subagent,
  isRecorded,
} from "../../tauri";
import {
  EMPTY_BUCKETS,
  type TokenBuckets,
  addBuckets,
  bucketTotal,
  bucketsFromUsage,
} from "./buckets";

/** A share of the session's spend, never rounded down to "0%" while it is real. */
export interface SpendShare {
  amount: number;
  label: string;
}

/**
 * A share as this lens is allowed to state it. A cost that rounds to nothing is
 * still a cost, so it reads "<1%"; only an amount of exactly zero reads "0%".
 */
export function spendShare(amount: number, total: number): SpendShare {
  if (total === 0 || amount === 0) {
    return { amount, label: "0%" };
  }
  const percent = Math.round((amount / total) * 100);
  return { amount, label: percent === 0 ? "<1%" : `${percent}%` };
}

/** How many responses the orchestrator figure rests on, and how many repeats it collapsed. */
export interface CoverageStats {
  respondedCount: number;
  collapsedCount: number;
}

/** One usage-bearing event, kept beside the buckets its own harness's rules produced. */
export interface CountedResponse {
  event: AillyEvent;
  buckets: TokenBuckets;
}

/**
 * The usage-bearing events of one session, counted once per API response.
 *
 * Claude writes one response as several records that each repeat that response's
 * identical usage, so the response identity is the key that collapses them. A
 * record that carried no identity stands on its own: nothing in the transcript
 * asserts it is a repeat of its neighbour.
 */
function countedResponses(events: AillyEvent[]): { kept: CountedResponse[]; collapsed: number } {
  const seen = new Set<string>();
  const kept: CountedResponse[] = [];
  let collapsed = 0;
  for (const event of events) {
    if (!isRecorded(event.token_usage)) {
      continue;
    }
    if (isRecorded(event.response_id)) {
      if (seen.has(event.response_id.Recorded)) {
        collapsed += 1;
        continue;
      }
      seen.add(event.response_id.Recorded);
    }
    kept.push({
      event,
      buckets: bucketsFromUsage(event.token_usage.Recorded, event.source.harness),
    });
  }
  return { kept, collapsed };
}

/**
 * Sums one session's own usage, counting each response once. Scoped to exactly
 * the events passed in, so a parent and a child sharing a response id cannot
 * collide: they are never folded in the same call.
 */
export function foldOwnUsage(events: AillyEvent[]): {
  buckets: TokenBuckets;
  coverage: CoverageStats;
} {
  const { kept, collapsed } = countedResponses(events);
  return {
    buckets: kept.reduce((sum, counted) => addBuckets(sum, counted.buckets), EMPTY_BUCKETS),
    coverage: { respondedCount: kept.length, collapsedCount: collapsed },
  };
}

/**
 * A spawn's child spend is exactly one of these, never a flat object with
 * optional fields — "resolved but also unlinkable" is unrepresentable. `pending`
 * is the read the pane has started and not yet settled.
 */
export type ChildSpendResult =
  | { status: "pending" }
  | { status: "unlinkable" }
  | { status: "failed"; message: string }
  | { status: "resolved"; buckets: TokenBuckets; events: AillyEvent[] };

export interface SpawnSpendRow {
  eventId: string;
  agentType: SourceValue<string>;
  prompt: SourceValue<string>;
  outcome: SourceValue<string>;
  childSpend: ChildSpendResult;
  finalContext: SourceValue<number>;
  /** The distinct models the child transcript named, once its spend has resolved. */
  models: string[];
}

export type SpendMomentKind = "response" | "spawn";

export interface SpendMoment {
  eventId: string;
  kind: SpendMomentKind;
  label: string;
  amount: number;
  timestamp: SourceValue<string>;
  /** The model(s) that produced this moment's spend, when a source named one. */
  models: string[];
}

/** Which of the chart's two stacked series a unit of spend belongs to. */
export type SpendParty = "orchestrator" | "subagent";

/**
 * One unit of spend at the moment it was actually incurred.
 *
 * Deliberately not the same list as `moments`. A spawn ranks as one cumulative
 * amount, because that is the delegation's cost and what a user compares; but on
 * a time axis that same cost belongs at the child's *own* response timestamps,
 * spread across the minutes it really ran, rather than piled onto the instant
 * the parent delegated.
 */
export interface SpendUnit {
  party: SpendParty;
  amount: number;
  timestamp: SourceValue<string>;
}

export interface SessionTokenStats {
  orchestratorSpend: number;
  coverage: CoverageStats;
  subagentSpend: number;
  unlinkableSpawnCount: number;
  pendingSpawnCount: number;
  sessionTotal: number;
  /**
   * Every descendant session this fold actually reached, so a surface can look
   * up their index rows and add their prices. Prices are not folded here: they
   * are columns the indexer wrote, and re-deriving them from an event page is
   * how the two would start to disagree.
   */
  descendantSessionIds: string[];
  composition: Record<keyof TokenBuckets, SpendShare>;
  /** The distinct models that produced the orchestrator's own responses. */
  orchestratorModels: string[];
  /** The distinct models named across every resolved subagent's own transcript. */
  subagentModels: string[];
  /** The union of the two above, in first-seen order (orchestrator, then subagent). */
  sessionModels: string[];
  spawnRows: SpawnSpendRow[];
  moments: SpendMoment[];
  spendUnits: SpendUnit[];
  excludedFromChartCount: number;
}

/** One recorded spawn, with whatever child-spend result has settled so far. */
export interface SpawnSpend {
  eventId: string;
  subagent: Subagent;
  childSpend: ChildSpendResult;
}

/** How long a moment's label runs before it is elided in a ranked row. */
const MOMENT_LABEL_LENGTH = 120;

/** What a response moment is about: its own text, as far as a row can show it. */
function responseLabel(event: AillyEvent): string {
  const text =
    isRecorded(event.turn) && isRecorded(event.turn.Recorded.text)
      ? event.turn.Recorded.text.Recorded.trim()
      : "";
  if (text.length === 0) {
    return "Text not recorded";
  }
  const firstLine = firstLineOf(text);
  return firstLine.length > MOMENT_LABEL_LENGTH
    ? `${firstLine.slice(0, MOMENT_LABEL_LENGTH)}…`
    : firstLine;
}

function firstLineOf(text: string): string {
  return text.split("\n", 1).join("");
}

/**
 * Every model a set of events named, once each, in first-seen order.
 *
 * A session's own responses can name more than one model — the source model
 * can change mid-session — so this is a list rather than a single value, and
 * empty rather than a placeholder when no event in the set named one.
 */
function distinctModels(events: Array<{ model: SourceValue<string> }>): string[] {
  const models: string[] = [];
  for (const event of events) {
    if (isRecorded(event.model) && !models.includes(event.model.Recorded)) {
      models.push(event.model.Recorded);
    }
  }
  return models;
}

/** The union of two model lists, in first-seen order with no repeats. */
function unionModels(left: string[], right: string[]): string[] {
  return [...left, ...right.filter((model) => !left.includes(model))];
}

/** What a spawn moment is about: the kind of agent, and what it was asked to do. */
function spawnLabel(subagent: Subagent): string {
  const type = isRecorded(subagent.agent_type) ? subagent.agent_type.Recorded : "Not recorded";
  if (!isRecorded(subagent.prompt)) {
    return type;
  }
  return `${type} — ${firstLineOf(subagent.prompt.Recorded)}`;
}

/** The harness's own figure for the spawn: how large its final turn's context was. */
function finalContext(subagent: Subagent): SourceValue<number> {
  return isRecorded(subagent.token_usage) ? subagent.token_usage.Recorded.total : "Absent";
}

/**
 * The one entry point the Tokens pane calls, mirroring `summarizeSession` and
 * `subagentSpawnRows`. Recomputed on every render as child reads resolve.
 *
 * A spawn contributes to the totals only once its own spend is known. An
 * unlinkable one is counted as a named shortfall instead, because the final
 * context the harness did record is a different fact and standing it in would
 * report a number the session never spent.
 */
export function summarizeTokenUsage(
  parentEvents: AillyEvent[],
  spawns: SpawnSpend[],
): SessionTokenStats {
  const own = countedResponses(parentEvents);
  const timestamps = new Map(parentEvents.map((event) => [event.id, event.timestamp]));

  let buckets = own.kept.reduce((sum, counted) => addBuckets(sum, counted.buckets), EMPTY_BUCKETS);
  const orchestratorSpend = bucketTotal(buckets);
  const orchestratorModels = distinctModels(own.kept.map((counted) => counted.event));
  const moments: SpendMoment[] = own.kept.map((counted) => ({
    eventId: counted.event.id,
    kind: "response",
    label: responseLabel(counted.event),
    amount: bucketTotal(counted.buckets),
    timestamp: counted.event.timestamp,
    models: distinctModels([counted.event]),
  }));
  const spendUnits: SpendUnit[] = own.kept.map((counted) => ({
    party: "orchestrator",
    amount: bucketTotal(counted.buckets),
    timestamp: counted.event.timestamp,
  }));

  let subagentSpend = 0;
  const descendantSessionIds = new Set<string>();
  let unlinkableSpawnCount = 0;
  let pendingSpawnCount = 0;
  let subagentModels: string[] = [];
  const spawnRows: SpawnSpendRow[] = [];
  for (const spawn of spawns) {
    const childModels =
      spawn.childSpend.status === "resolved" ? distinctModels(spawn.childSpend.events) : [];
    spawnRows.push({
      eventId: spawn.eventId,
      agentType: spawn.subagent.agent_type,
      prompt: spawn.subagent.prompt,
      outcome: spawn.subagent.outcome,
      childSpend: spawn.childSpend,
      finalContext: finalContext(spawn.subagent),
      models: childModels,
    });
    if (spawn.childSpend.status === "unlinkable") {
      unlinkableSpawnCount += 1;
      continue;
    }
    if (spawn.childSpend.status === "pending") {
      pendingSpawnCount += 1;
      continue;
    }
    if (spawn.childSpend.status === "failed") {
      continue;
    }
    buckets = addBuckets(buckets, spawn.childSpend.buckets);
    subagentSpend += bucketTotal(spawn.childSpend.buckets);
    subagentModels = unionModels(subagentModels, childModels);
    moments.push({
      eventId: spawn.eventId,
      kind: "spawn",
      label: spawnLabel(spawn.subagent),
      amount: bucketTotal(spawn.childSpend.buckets),
      timestamp: timestamps.get(spawn.eventId) ?? "Absent",
      models: childModels,
    });
    for (const event of spawn.childSpend.events) {
      descendantSessionIds.add(event.session_id);
    }
    for (const counted of countedResponses(spawn.childSpend.events).kept) {
      spendUnits.push({
        party: "subagent",
        amount: bucketTotal(counted.buckets),
        timestamp: counted.event.timestamp,
      });
    }
  }

  const sessionTotal = bucketTotal(buckets);
  return {
    orchestratorSpend,
    coverage: { respondedCount: own.kept.length, collapsedCount: own.collapsed },
    subagentSpend,
    unlinkableSpawnCount,
    pendingSpawnCount,
    sessionTotal,
    descendantSessionIds: [...descendantSessionIds],
    composition: {
      freshInput: spendShare(buckets.freshInput, sessionTotal),
      output: spendShare(buckets.output, sessionTotal),
      cacheRead: spendShare(buckets.cacheRead, sessionTotal),
      cacheWrite: spendShare(buckets.cacheWrite, sessionTotal),
    },
    orchestratorModels,
    subagentModels,
    sessionModels: unionModels(orchestratorModels, subagentModels),
    spawnRows,
    moments,
    spendUnits,
    excludedFromChartCount: spendUnits.filter((unit) => !isRecorded(unit.timestamp)).length,
  };
}

/**
 * A session with no usage and nothing delegated has nothing to account for. A
 * session that only delegated still has its spawns to show, even when every
 * figure in them is unrecorded — every spawn keeps a row whatever its read did,
 * so a row's presence already covers the reads still in flight.
 *
 * Shared by the Tokens lens and the Summary lens's Tokens card, so the two
 * surfaces agree on when there is nothing to account for.
 */
export function recordedNothing(stats: SessionTokenStats): boolean {
  return (
    stats.sessionTotal === 0 && stats.coverage.respondedCount === 0 && stats.spawnRows.length === 0
  );
}

/**
 * The largest moments first, capped. A tie keeps source order so the same
 * session always reads the same way rather than depending on the sort.
 */
export function rankMoments(moments: SpendMoment[], cap: number): SpendMoment[] {
  return moments
    .map((moment, index) => ({ moment, index }))
    .sort((left, right) => right.moment.amount - left.moment.amount || left.index - right.index)
    .slice(0, cap)
    .map((entry) => entry.moment);
}

/** Which of the two readings the chart is showing. */
export type SpendReading = "per-response" | "cumulative";

/** One point on the time axis, with the two stacked series kept apart. */
export interface SpendSeriesPoint {
  time: number;
  orchestrator: number;
  subagent: number;
}

/**
 * The chart's data, prepared here rather than in the chart so it is testable:
 * Recharts renders nothing under jsdom.
 *
 * Only timestamped units can be placed. The count of those left out lives on
 * `SessionTokenStats.excludedFromChartCount`, and the note that renders it is
 * what reconciles this series' visible sum with the Session total.
 */
export function spendSeries(units: SpendUnit[], reading: SpendReading): SpendSeriesPoint[] {
  const points = units
    .flatMap((unit) =>
      isRecorded(unit.timestamp)
        ? [
            {
              time: Date.parse(unit.timestamp.Recorded),
              orchestrator: unit.party === "orchestrator" ? unit.amount : 0,
              subagent: unit.party === "subagent" ? unit.amount : 0,
            },
          ]
        : [],
    )
    .filter((point) => !Number.isNaN(point.time))
    // A child's own response timestamps interleave with the parent's, so the
    // order the fold produced is not time order.
    .sort((left, right) => left.time - right.time);

  if (reading === "per-response") {
    return points;
  }
  let orchestrator = 0;
  let subagent = 0;
  return points.map((point) => {
    orchestrator += point.orchestrator;
    subagent += point.subagent;
    return { time: point.time, orchestrator, subagent };
  });
}
