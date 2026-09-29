import { describe, expect, it } from "vitest";

import {
  type AillyEvent,
  EventKind,
  type Harness,
  type SourceValue,
  type Subagent,
  type TokenUsage,
} from "../../src/tauri";
import { type TokenBuckets, bucketTotal, bucketsFromUsage } from "../../src/ui/tokens/buckets";
import {
  type ChildSpendResult,
  type SpendMoment,
  type SpendUnit,
  foldOwnUsage,
  rankMoments,
  spendSeries,
  spendShare,
  summarizeTokenUsage,
} from "../../src/ui/tokens/rollup";

function usage(parts: Partial<TokenUsage>): TokenUsage {
  return {
    input: "Absent",
    output: "Absent",
    cache_read: "Absent",
    cache_write: "Absent",
    total: "Absent",
    cost_total_micros: "Absent",
    scope: "message",
    ...parts,
  };
}

/** Usage as a harness writes it: every figure recorded, none derived. */
function recordedUsage(parts: {
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
}): TokenUsage {
  return usage({
    input: { Recorded: parts.input },
    output: { Recorded: parts.output },
    cache_read: { Recorded: parts.cacheRead },
    cache_write: { Recorded: parts.cacheWrite },
  });
}

function event(parts: Partial<AillyEvent> & { id: string }): AillyEvent {
  return {
    session_id: "session-1",
    kind: EventKind.AssistantTurn,
    source: { harness: "claude_code", path: "/home/parent.jsonl", line: 1, ordinal: 1 },
    native_id: "Absent",
    response_id: "Absent",
    model: "Absent",
    timestamp: "Absent",
    turn: "Absent",
    tool_call: "Absent",
    tool_result: "Absent",
    token_usage: "Absent",
    files: "Absent",
    detail: "Absent",
    subagent: "Absent",
    ...parts,
  };
}

function response(
  id: string,
  responseId: SourceValue<string>,
  amounts: { input: number; output: number; cacheRead: number; cacheWrite: number },
  timestamp: SourceValue<string> = "Absent",
  text = "talking",
): AillyEvent {
  return event({
    id,
    response_id: responseId,
    timestamp,
    turn: { Recorded: { role: "assistant", text: { Recorded: text } } },
    token_usage: { Recorded: recordedUsage(amounts) },
  });
}

function spawnSubagent(parts: Partial<Subagent>): Subagent {
  return {
    native_id: "Absent",
    agent_type: { Recorded: "explore" },
    prompt: { Recorded: "Map the parser" },
    outcome: { Recorded: "completed" },
    nickname: "Absent",
    duration_ms: "Absent",
    token_usage: "Absent",
    child_session_id: "Absent",
    ...parts,
  };
}

function buckets(parts: Partial<TokenBuckets>): TokenBuckets {
  return { freshInput: 0, output: 0, cacheRead: 0, cacheWrite: 0, ...parts };
}

function moment(parts: Partial<SpendMoment> & { eventId: string; amount: number }): SpendMoment {
  return { kind: "response", label: parts.eventId, timestamp: "Absent", models: [], ...parts };
}

describe("bucketsFromUsage", () => {
  /**
   * Claude and Pi write four figures that sit beside one another, so each is
   * already the disjoint bucket it looks like. Proven in the Rust adapters'
   * membership tests, which is what this branch depends on.
   */
  it.each<Harness>(["claude_code", "pi"])(
    "reads %s's figures straight, because none of them contains another",
    (harness) => {
      const result = bucketsFromUsage(
        recordedUsage({ input: 2, output: 848, cacheRead: 34965, cacheWrite: 4041 }),
        harness,
      );

      expect(result).toEqual(
        buckets({ freshInput: 2, output: 848, cacheRead: 34965, cacheWrite: 4041 }),
      );
    },
  );

  /**
   * Codex counts both cache figures inside `input_tokens`, so reading its input
   * straight would count the cached portion twice — once as fresh input and
   * again as cache read.
   */
  it("subtracts both of Codex's cache figures out of its input", () => {
    const result = bucketsFromUsage(
      recordedUsage({ input: 5000, output: 200, cacheRead: 3000, cacheWrite: 800 }),
      "codex",
    );

    expect(result).toEqual(
      buckets({ freshInput: 1200, output: 200, cacheRead: 3000, cacheWrite: 800 }),
    );
    expect(bucketTotal(result)).toBe(5200);
  });

  /**
   * A Codex record whose cache figures exceed its input is not something to
   * report as negative spend. The cache figures are what the source recorded, so
   * they are kept and only the derived remainder is floored.
   */
  it("never derives a negative fresh input from a cache figure larger than the input", () => {
    const result = bucketsFromUsage(
      recordedUsage({ input: 100, output: 10, cacheRead: 3000, cacheWrite: 0 }),
      "codex",
    );

    expect(result.freshInput).toBe(0);
    expect(result.cacheRead).toBe(3000);
  });

  /** An unrecorded figure contributes nothing; it is not a zero the source wrote. */
  it("treats an unrecorded figure as contributing nothing", () => {
    const result = bucketsFromUsage(usage({ output: { Recorded: 40 } }), "claude_code");

    expect(result).toEqual(buckets({ output: 40 }));
  });

  /**
   * Claude writes no rolled-up total on a message, so every figure the lens shows
   * is derived from the parts. The harness's own `total` is deliberately ignored
   * even when present, so the composition can never disagree with the total
   * above it.
   */
  it("derives the total from the parts rather than the harness's own total", () => {
    const claimed = usage({
      input: { Recorded: 10 },
      output: { Recorded: 5 },
      total: { Recorded: 999 },
    });

    expect(bucketTotal(bucketsFromUsage(claimed, "claude_code"))).toBe(15);
  });
});

describe("spendShare", () => {
  it("rounds a share to a whole percent", () => {
    expect(spendShare(231000, 247912).label).toBe("93%");
  });

  /** A real cost that rounds to nothing must not read as nothing. */
  it("reports a nonzero share too small to round as <1%", () => {
    expect(spendShare(12, 247912).label).toBe("<1%");
  });

  it("reports an amount of exactly zero as 0%", () => {
    expect(spendShare(0, 247912).label).toBe("0%");
  });

  /** No spend at all is not a division to perform. */
  it("reports every share of an empty total as 0%", () => {
    expect(spendShare(0, 0)).toEqual({ amount: 0, label: "0%" });
  });
});

describe("foldOwnUsage", () => {
  /**
   * Claude writes one API response as several records carrying that response's
   * identical usage, so counting records instead of responses inflates the
   * figure — by 56% in the sampled session.
   */
  it("counts one response once however many records repeated it", () => {
    const amounts = { input: 2, output: 200, cacheRead: 8000, cacheWrite: 1000 };
    const { buckets: summed, coverage } = foldOwnUsage([
      response("evt-2", { Recorded: "msg-a" }, amounts),
      response("evt-3", { Recorded: "msg-a" }, amounts),
      response("evt-5", { Recorded: "msg-b" }, { ...amounts, output: 600 }),
    ]);

    expect(bucketTotal(summed)).toBe(9202 + 9602);
    expect(coverage).toEqual({ respondedCount: 2, collapsedCount: 1 });
  });

  /**
   * Nothing in a transcript that recorded no response identity asserts that one
   * record is a repeat of another, so each stands on its own.
   */
  it("never merges two records that recorded no response identity", () => {
    const amounts = { input: 1, output: 10, cacheRead: 100, cacheWrite: 0 };
    const { buckets: summed, coverage } = foldOwnUsage([
      response("evt-1", "Absent", amounts),
      response("evt-2", "Absent", amounts),
    ]);

    expect(bucketTotal(summed)).toBe(222);
    expect(coverage).toEqual({ respondedCount: 2, collapsedCount: 0 });
  });

  /** The kept record is the first one, so its timestamp and text are the earliest. */
  it("keeps the first record of a response, not the last", () => {
    const { buckets: summed } = foldOwnUsage([
      response(
        "evt-2",
        { Recorded: "msg-a" },
        {
          input: 1,
          output: 1,
          cacheRead: 0,
          cacheWrite: 0,
        },
      ),
      response(
        "evt-3",
        { Recorded: "msg-a" },
        {
          input: 500,
          output: 500,
          cacheRead: 0,
          cacheWrite: 0,
        },
      ),
    ]);

    expect(bucketTotal(summed)).toBe(2);
  });

  it("ignores an event that recorded no usage at all", () => {
    const { buckets: summed, coverage } = foldOwnUsage([
      event({ id: "evt-1", kind: EventKind.UserTurn }),
    ]);

    expect(bucketTotal(summed)).toBe(0);
    expect(coverage).toEqual({ respondedCount: 0, collapsedCount: 0 });
  });

  /**
   * Each harness's own arithmetic, chosen per event rather than per page: a fold
   * over a parent and its child reads whatever each event recorded about itself.
   */
  it("normalizes each event by the harness that recorded it", () => {
    const codex = event({
      id: "evt-1",
      source: { harness: "codex", path: "/home/rollout.jsonl", line: 1, ordinal: 1 },
      token_usage: {
        Recorded: recordedUsage({ input: 5000, output: 200, cacheRead: 3000, cacheWrite: 800 }),
      },
    });

    expect(bucketTotal(foldOwnUsage([codex]).buckets)).toBe(5200);
  });
});

const LINKED: Subagent = spawnSubagent({
  agent_type: { Recorded: "explore" },
  child_session_id: { Recorded: "child-session" },
  token_usage: { Recorded: usage({ total: { Recorded: 39856 } }) },
});

const UNLINKED: Subagent = spawnSubagent({
  agent_type: { Recorded: "research" },
  outcome: { Recorded: "error" },
  token_usage: { Recorded: usage({ total: { Recorded: 12480 } }) },
});

/** The acceptance journey's own fixture, folded here without a UI in the way. */
function journeyEvents(): AillyEvent[] {
  const repeated = { input: 2, output: 200, cacheRead: 8000, cacheWrite: 1000 };
  return [
    event({ id: "evt-1", kind: EventKind.UserTurn }),
    response("evt-2", { Recorded: "msg-a" }, repeated, { Recorded: "2026-08-14T10:00:30Z" }),
    response("evt-3", { Recorded: "msg-a" }, repeated, { Recorded: "2026-08-14T10:00:30Z" }),
    event({
      id: "evt-4",
      kind: EventKind.SubagentSpawn,
      timestamp: { Recorded: "2026-08-14T10:01:00Z" },
      subagent: { Recorded: LINKED },
    }),
    response(
      "evt-5",
      { Recorded: "msg-b" },
      { input: 4, output: 600, cacheRead: 40000, cacheWrite: 2000 },
      { Recorded: "2026-08-14T10:05:00Z" },
      "Synthesizing.",
    ),
    event({
      id: "evt-6",
      kind: EventKind.SubagentSpawn,
      timestamp: { Recorded: "2026-08-14T10:06:00Z" },
      subagent: { Recorded: UNLINKED },
    }),
    response(
      "evt-7",
      { Recorded: "msg-c" },
      {
        input: 1,
        output: 100,
        cacheRead: 3000,
        cacheWrite: 0,
      },
    ),
  ];
}

/** One token of output, so a fold's arithmetic is easy to read off. */
const ONE_TOKEN = { input: 0, output: 1, cacheRead: 0, cacheWrite: 0 };

/** What the linked child spent, folded from its own transcript: 193,005. */
const CHILD_SPEND: ChildSpendResult = {
  status: "resolved",
  buckets: buckets({ freshInput: 5, output: 2000, cacheRead: 180000, cacheWrite: 11000 }),
  events: [],
};

function journeyStats(childSpend: ChildSpendResult = CHILD_SPEND) {
  return summarizeTokenUsage(journeyEvents(), [
    { eventId: "evt-4", subagent: LINKED, childSpend },
    { eventId: "evt-6", subagent: UNLINKED, childSpend: { status: "unlinkable" } },
  ]);
}

describe("summarizeTokenUsage", () => {
  it("separates what the orchestrator spent from what its subagents did", () => {
    const stats = journeyStats();

    expect(stats.orchestratorSpend).toBe(54907);
    expect(stats.subagentSpend).toBe(193005);
    expect(stats.sessionTotal).toBe(247912);
    expect(stats.coverage).toEqual({ respondedCount: 3, collapsedCount: 1 });
  });

  /**
   * The composition is derived from the same buckets as the total above it, so
   * the two can never disagree — the invariant that rules out reporting the
   * harness's own `total` beside a composition built from parts.
   */
  it("breaks the whole session's spend into buckets that sum back to its total", () => {
    const stats = journeyStats();

    expect(stats.composition.cacheRead.amount).toBe(231000);
    expect(stats.composition.cacheRead.label).toBe("93%");
    expect(stats.composition.freshInput.amount).toBe(12);
    expect(stats.composition.freshInput.label).toBe("<1%");
    expect(stats.composition.output.amount).toBe(2900);
    expect(stats.composition.cacheWrite.amount).toBe(14000);

    const summed = Object.values(stats.composition).reduce((sum, part) => sum + part.amount, 0);
    expect(summed).toBe(stats.sessionTotal);
  });

  /**
   * A spawn the harness named no child transcript for has an unknowable spend.
   * The final context it did record is a different fact and is never substituted
   * for it.
   */
  it("counts an unlinkable spawn as a shortfall rather than folding in its final context", () => {
    const stats = journeyStats();

    expect(stats.unlinkableSpawnCount).toBe(1);
    const [explore, research] = stats.spawnRows;
    expect(explore?.childSpend.status).toBe("resolved");
    expect(explore?.finalContext).toEqual({ Recorded: 39856 });
    expect(research?.childSpend).toEqual({ status: "unlinkable" });
    expect(research?.finalContext).toEqual({ Recorded: 12480 });
    expect(research?.outcome).toEqual({ Recorded: "error" });
  });

  /** A read still in flight is not a zero, and the pane needs to know how many. */
  it("counts a spawn whose read has not settled without adding it to the total", () => {
    const stats = journeyStats({ status: "pending" });

    expect(stats.pendingSpawnCount).toBe(1);
    expect(stats.subagentSpend).toBe(0);
    expect(stats.sessionTotal).toBe(54907);
  });

  /**
   * A read that failed is a different fact from a spawn that named no child at
   * all: the row says which, and neither is counted as spend.
   */
  it("keeps a failed child read out of the totals and out of the shortfall count", () => {
    const failed: ChildSpendResult = { status: "failed", message: "no such session" };
    const stats = summarizeTokenUsage(journeyEvents(), [
      { eventId: "evt-4", subagent: LINKED, childSpend: failed },
    ]);

    expect(stats.subagentSpend).toBe(0);
    expect(stats.unlinkableSpawnCount).toBe(0);
    expect(stats.spawnRows[0]?.childSpend).toEqual(failed);
  });

  /**
   * A grandchild's spend is still this delegation's cost, so a resolved subtree
   * arrives already folded. Proven here without a real walk: the pane's read
   * supplies the whole subtree's buckets.
   */
  it("takes a resolved spawn's whole subtree as its child spend", () => {
    const subtree = buckets({ freshInput: 10, output: 20, cacheRead: 30, cacheWrite: 40 });
    const stats = summarizeTokenUsage(
      [],
      [
        {
          eventId: "evt-4",
          subagent: LINKED,
          childSpend: { status: "resolved", buckets: subtree, events: [] },
        },
      ],
    );

    expect(stats.subagentSpend).toBe(100);
  });

  /**
   * One moment per response and per spawn that spent something. The repeated
   * `msg-a` record earns no second entry, and an unlinkable spawn earns none at
   * all — there is no amount to rank it by.
   */
  it("names one rankable moment per response and per spawn with a known amount", () => {
    const stats = journeyStats();

    expect(stats.moments).toHaveLength(4);
    expect(stats.moments.filter((entry) => entry.kind === "spawn")).toHaveLength(1);
    const ranked = rankMoments(stats.moments, 10);
    expect(ranked.map((entry) => entry.amount)).toEqual([193005, 42604, 9202, 3101]);
    expect(ranked[0]?.kind).toBe("spawn");
    expect(ranked[0]?.label).toContain("explore");
    expect(ranked[1]?.label).toContain("Synthesizing.");
  });

  /**
   * A delegation ranks as one amount but is spent over the minutes the child
   * really ran, so the time axis places the child's own responses at their own
   * timestamps rather than piling the subtree onto the instant of the spawn.
   */
  it("places a delegation's spend at the child's own response timestamps", () => {
    const child = [
      response("child-1", { Recorded: "cmsg-a" }, ONE_TOKEN, {
        Recorded: "2026-08-14T10:01:30Z",
      }),
      response("child-2", { Recorded: "cmsg-b" }, ONE_TOKEN, {
        Recorded: "2026-08-14T10:03:00Z",
      }),
    ];
    const stats = summarizeTokenUsage(journeyEvents(), [
      {
        eventId: "evt-4",
        subagent: LINKED,
        childSpend: { status: "resolved", buckets: foldOwnUsage(child).buckets, events: child },
      },
    ]);

    const subagent = stats.spendUnits.filter((unit) => unit.party === "subagent");
    expect(subagent).toHaveLength(2);
    expect(subagent.map((unit) => unit.timestamp)).toEqual([
      { Recorded: "2026-08-14T10:01:30Z" },
      { Recorded: "2026-08-14T10:03:00Z" },
    ]);
    // The spawn still ranks once, as the whole subtree's cost.
    expect(stats.moments.filter((entry) => entry.kind === "spawn")).toHaveLength(1);
    expect(stats.subagentSpend).toBe(2);
  });

  /** A session that recorded nothing reports nothing, not a page of zeros. */
  it("reports no recorded spend for a session that recorded none", () => {
    const stats = summarizeTokenUsage([event({ id: "evt-1", kind: EventKind.UserTurn })], []);

    expect(stats.sessionTotal).toBe(0);
    expect(stats.moments).toHaveLength(0);
    expect(stats.composition.cacheRead.label).toBe("0%");
  });
});

describe("rankMoments", () => {
  /** Two equal amounts keep source order, so the same session always reads the same. */
  it("keeps source order when two moments tie on amount", () => {
    const ranked = rankMoments(
      [
        moment({ eventId: "first", amount: 5 }),
        moment({ eventId: "second", amount: 5 }),
        moment({ eventId: "third", amount: 9 }),
      ],
      10,
    );

    expect(ranked.map((entry) => entry.eventId)).toEqual(["third", "first", "second"]);
  });

  it("caps the list at the cap it was given", () => {
    const many = Array.from({ length: 14 }, (_, index) =>
      moment({ eventId: `evt-${index}`, amount: index }),
    );

    expect(rankMoments(many, 10)).toHaveLength(10);
  });
});

describe("spendSeries", () => {
  const timed: SpendUnit[] = [
    { party: "orchestrator", amount: 100, timestamp: { Recorded: "2026-08-14T10:00:00Z" } },
    { party: "subagent", amount: 400, timestamp: { Recorded: "2026-08-14T10:01:00Z" } },
    { party: "orchestrator", amount: 50, timestamp: { Recorded: "2026-08-14T10:02:00Z" } },
    { party: "orchestrator", amount: 999, timestamp: "Absent" },
  ];

  /** Per-response spend, orchestrator and subagent as separate stacked series, densely indexed. */
  it("places each unit on a dense message index under its own series", () => {
    const points = spendSeries(timed, "per-response");

    expect(points).toEqual([
      { message: 1, time: Date.parse("2026-08-14T10:00:00Z"), orchestrator: 100, subagent: 0 },
      { message: 2, time: Date.parse("2026-08-14T10:01:00Z"), orchestrator: 0, subagent: 400 },
      { message: 3, time: Date.parse("2026-08-14T10:02:00Z"), orchestrator: 50, subagent: 0 },
      { message: 4, time: null, orchestrator: 999, subagent: 0 },
    ]);
  });

  /** The reading that answers front-loaded versus back-loaded directly. */
  it("accumulates each series when asked for the cumulative reading", () => {
    const points = spendSeries(timed, "cumulative");

    expect(points.map((point) => point.orchestrator)).toEqual([100, 100, 150, 1149]);
    expect(points.map((point) => point.subagent)).toEqual([0, 400, 400, 400]);
  });

  /** Conversation order is preserved, not reordered by timestamp. */
  it("keeps the fold's own order rather than sorting by time", () => {
    const points = spendSeries(
      [
        { party: "orchestrator", amount: 1, timestamp: { Recorded: "2026-08-14T10:05:00Z" } },
        { party: "orchestrator", amount: 2, timestamp: { Recorded: "2026-08-14T10:00:00Z" } },
      ],
      "per-response",
    );

    expect(points.map((point) => point.orchestrator)).toEqual([1, 2]);
  });

  /**
   * The chart reads as the conversation ran, one step per thing that spent, so
   * the axis is the message and not the clock. Run through the real fold,
   * because the order the units arrive in is half the fact under test: the
   * delegation's two child responses belong at the spawn's own position, and the
   * one of them the harness never timestamped still keeps its slot between two
   * that it did. Nothing is dropped for a missing timestamp, which is why the
   * series' visible sum is the whole session's reachable spend and there is no
   * excluded remainder left to reconcile.
   */
  it("places every spend unit on a dense message axis in conversation order", () => {
    const child = [
      response("child-1", { Recorded: "cmsg-a" }, ONE_TOKEN, {
        Recorded: "2026-08-14T10:01:30Z",
      }),
      response("child-2", { Recorded: "cmsg-b" }, ONE_TOKEN),
    ];
    const stats = summarizeTokenUsage(journeyEvents(), [
      {
        eventId: "evt-4",
        subagent: LINKED,
        childSpend: { status: "resolved", buckets: foldOwnUsage(child).buckets, events: child },
      },
      { eventId: "evt-6", subagent: UNLINKED, childSpend: { status: "unlinkable" } },
    ]);

    const points = spendSeries(stats.spendUnits, "per-response");

    expect(points).toEqual([
      {
        message: 1,
        time: Date.parse("2026-08-14T10:00:30Z"),
        orchestrator: 9202,
        subagent: 0,
      },
      { message: 2, time: Date.parse("2026-08-14T10:01:30Z"), orchestrator: 0, subagent: 1 },
      { message: 3, time: null, orchestrator: 0, subagent: 1 },
      { message: 4, time: Date.parse("2026-08-14T10:05:00Z"), orchestrator: 42604, subagent: 0 },
      { message: 5, time: null, orchestrator: 3101, subagent: 0 },
    ]);
    const charted = points.reduce((sum, point) => sum + point.orchestrator + point.subagent, 0);
    expect(charted).toBe(stats.orchestratorSpend + stats.subagentSpend);
  });
});
