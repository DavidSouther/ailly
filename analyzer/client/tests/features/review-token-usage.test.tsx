// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  type AillyEvent,
  EventKind,
  type IndexProgress,
  type IndexStatus,
  type SessionListItem,
  type SourceValue,
  type Subagent,
  type SubagentTokenUsage,
} from "../../src/tauri";
import { resetSessionsStore } from "../../src/ui/sessions/store";

let indexed: SessionListItem[] = [];
let eventsBySession: Record<string, UsageEvent[]> = {};
const getEventPage = vi.fn<(sessionId: string) => Promise<AillyEvent[]>>();
const startRefresh = vi.fn<() => Promise<void>>();
let onProgress: ((progress: IndexProgress) => void) | null = null;
let onComplete: ((status: IndexStatus) => void) | null = null;

vi.mock("../../src/tauri", async (importActual) => {
  const actual = await importActual<typeof import("../../src/tauri")>();
  return {
    ...actual,
    listSessions: async () => indexed,
    getEventPage: (sessionId: string) => getEventPage(sessionId),
    startRefresh: () => startRefresh(),
    onIndexProgress: async (handler: (progress: IndexProgress) => void) => {
      onProgress = handler;
      return () => {
        onProgress = null;
      };
    },
    onIndexComplete: async (handler: (status: IndexStatus) => void) => {
      onComplete = handler;
      return () => {
        onComplete = null;
      };
    },
  };
});

/**
 * The response identity the normalized event needs, so several records carrying
 * one API response's identical `usage` object collapse to one response instead
 * of overcounting. Declared here because the field that carries it does not
 * exist on `AillyEvent` yet.
 */
type UsageEvent = AillyEvent & { response_id: SourceValue<string> };

/**
 * A message-scoped usage record. Aliased because the exported name is
 * `SubagentTokenUsage`, which the design proposes renaming to `TokenUsage` —
 * one of its open artifact decisions.
 */
type RecordedUsage = SubagentTokenUsage;

/**
 * The orchestrator's own row, as the indexer wrote it. Claude records no total
 * and no price of its own, so both dollar figures on this row are the index's
 * own estimate — computed once when the session was first scanned, not folded
 * out of the event page this journey also reads.
 */
const SESSION: SessionListItem = {
  id: "claude_code:/home/parent.jsonl:one",
  harness: "claude_code",
  project: { Recorded: "ailly-analyzer" },
  event_count: 7,
  token_total: "Absent",
  recorded_price_micros: "Absent",
  estimated_tokens: { Recorded: 54_907 },
  estimated_price_micros: { Recorded: 150_000 },
  estimated_as_of: { Recorded: "2026-08-14" },
  last_activity: { Recorded: "2026-08-14T10:06:00Z" },
};

const CHILD_SESSION_ID = "claude_code:/home/subagents/agent-a3c304e0.jsonl:one";

/**
 * The linked child is a peer row in the index, not a nested figure, so the
 * subagent half of the session's price is that row's own estimate.
 */
const CHILD_SESSION: SessionListItem = {
  ...SESSION,
  id: CHILD_SESSION_ID,
  event_count: 2,
  estimated_tokens: { Recorded: 193_005 },
  estimated_price_micros: { Recorded: 500_000 },
};

const EXPLORE_PROMPT = "Map the parser module and report every entry point";
const RESEARCH_PROMPT = "Trace the failing spec back to its fixture";

/**
 * The orchestrator's most expensive single response, and so the largest moment
 * the parent page alone can see — ranked below the explore spawn once the
 * child's own spend is folded in.
 */
const SPIKE_TEXT = "Synthesizing everything the explore agent found.";

/**
 * Every event here is `claude_code`, whose cache fields sit beside `input`, so
 * fresh input and cache read are read straight from the recorded parts. Codex's
 * cached figure sits *inside* its input and would double-count under this same
 * arithmetic; that branch belongs to the rollup unit tests, not this journey.
 */
function baseEvent(
  id: string,
  ordinal: number,
  kind: EventKind,
  sessionId: string,
  timestamp: SourceValue<string>,
): UsageEvent {
  return {
    id,
    session_id: sessionId,
    kind,
    source: { harness: "claude_code", path: "/home/parent.jsonl", line: ordinal, ordinal },
    native_id: "Absent",
    response_id: "Absent",
    model: "Absent",
    timestamp,
    turn: "Absent",
    tool_call: "Absent",
    tool_result: "Absent",
    token_usage: "Absent",
    files: "Absent",
    detail: "Absent",
    subagent: "Absent",
  };
}

/**
 * Claude's message usage as it is actually written: no `total_tokens` field at
 * all, and an `input` figure of almost nothing beside tens of thousands of
 * cache reads. The lens must derive the total from the recorded parts.
 */
function messageUsage(parts: {
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
}): SourceValue<RecordedUsage> {
  return {
    Recorded: {
      input: { Recorded: parts.input },
      output: { Recorded: parts.output },
      cache_read: { Recorded: parts.cacheRead },
      cache_write: { Recorded: parts.cacheWrite },
      total: "Absent",
      cost_total_micros: "Absent",
      scope: "message",
    },
  };
}

function responseEvent(
  id: string,
  ordinal: number,
  sessionId: string,
  responseId: string,
  timestamp: SourceValue<string>,
  text: string,
  usage: SourceValue<RecordedUsage>,
): UsageEvent {
  return {
    ...baseEvent(id, ordinal, EventKind.AssistantTurn, sessionId, timestamp),
    response_id: { Recorded: responseId },
    turn: { Recorded: { role: "assistant", text: { Recorded: text } } },
    token_usage: usage,
  };
}

function spawnEvent(
  id: string,
  ordinal: number,
  timestamp: SourceValue<string>,
  subagent: Subagent,
): UsageEvent {
  return {
    ...baseEvent(id, ordinal, EventKind.SubagentSpawn, SESSION.id, timestamp),
    subagent: { Recorded: subagent },
  };
}

/**
 * A Claude spawn whose recorded `totalTokens` is the final turn's context
 * footprint, not what the child spent: `CHILD_EVENTS` spends nearly five times
 * this figure, and the real gap is wider still — 62× in the sampled session.
 */
const LINKED_SPAWN: Subagent = {
  native_id: { Recorded: "a3c304e0299ebbabc" },
  agent_type: { Recorded: "explore" },
  prompt: { Recorded: EXPLORE_PROMPT },
  outcome: { Recorded: "completed" },
  nickname: "Absent",
  duration_ms: { Recorded: 467407 },
  token_usage: {
    Recorded: {
      input: { Recorded: 2 },
      output: { Recorded: 848 },
      cache_read: { Recorded: 34965 },
      cache_write: { Recorded: 4041 },
      total: { Recorded: 39856 },
      cost_total_micros: "Absent",
      scope: "subagent",
    },
  },
  child_session_id: { Recorded: CHILD_SESSION_ID },
};

/**
 * A spawn that recorded a final context but named no child transcript to
 * follow, which is the common shape rather than the exotic one. Its spend is
 * therefore unknowable, and the recorded 12,480 must not be substituted for it.
 */
const UNLINKED_SPAWN: Subagent = {
  native_id: "Absent",
  agent_type: { Recorded: "research" },
  prompt: { Recorded: RESEARCH_PROMPT },
  outcome: { Recorded: "error" },
  nickname: "Absent",
  duration_ms: "Absent",
  token_usage: {
    Recorded: {
      input: "Absent",
      output: "Absent",
      cache_read: "Absent",
      cache_write: "Absent",
      total: { Recorded: 12480 },
      cost_total_micros: "Absent",
      scope: "subagent",
    },
  },
  child_session_id: "Absent",
};

/**
 * The orchestrator's own spend. `msg-a` is written as two records carrying one
 * identical usage object, the way Claude actually writes a multi-block
 * response, so counting records rather than responses reports 64,109 where the
 * orchestrator spent 54,907. `msg-c` recorded no timestamp, which is why its
 * point on the message axis carries no clock time.
 */
const PARENT_EVENTS: UsageEvent[] = [
  {
    ...baseEvent("evt-1", 1, EventKind.UserTurn, SESSION.id, {
      Recorded: "2026-08-14T10:00:00Z",
    }),
    turn: { Recorded: { role: "user", text: { Recorded: "Why is the parser spec failing?" } } },
  },
  responseEvent(
    "evt-2",
    2,
    SESSION.id,
    "msg-a",
    { Recorded: "2026-08-14T10:00:30Z" },
    "Reading the parser module.",
    messageUsage({ input: 2, output: 200, cacheRead: 8000, cacheWrite: 1000 }),
  ),
  responseEvent(
    "evt-3",
    3,
    SESSION.id,
    "msg-a",
    { Recorded: "2026-08-14T10:00:30Z" },
    "Delegating the survey to an explore agent.",
    messageUsage({ input: 2, output: 200, cacheRead: 8000, cacheWrite: 1000 }),
  ),
  spawnEvent("evt-4", 4, { Recorded: "2026-08-14T10:01:00Z" }, LINKED_SPAWN),
  responseEvent(
    "evt-5",
    5,
    SESSION.id,
    "msg-b",
    { Recorded: "2026-08-14T10:05:00Z" },
    SPIKE_TEXT,
    messageUsage({ input: 4, output: 600, cacheRead: 40000, cacheWrite: 2000 }),
  ),
  spawnEvent("evt-6", 6, { Recorded: "2026-08-14T10:06:00Z" }, UNLINKED_SPAWN),
  responseEvent(
    "evt-7",
    7,
    SESSION.id,
    "msg-c",
    "Absent",
    "The lexer drops a token.",
    messageUsage({ input: 1, output: 100, cacheRead: 3000, cacheWrite: 0 }),
  ),
];

/** What the linked child actually spent: 65,903 + 127,102 = 193,005. */
const CHILD_EVENTS: UsageEvent[] = [
  responseEvent(
    "child-1",
    1,
    CHILD_SESSION_ID,
    "cmsg-a",
    { Recorded: "2026-08-14T10:01:30Z" },
    "Reading src/parser/lexer.rs.",
    messageUsage({ input: 3, output: 900, cacheRead: 60000, cacheWrite: 5000 }),
  ),
  responseEvent(
    "child-2",
    2,
    CHILD_SESSION_ID,
    "cmsg-b",
    { Recorded: "2026-08-14T10:03:00Z" },
    "Every entry point, with its callers.",
    messageUsage({ input: 2, output: 1100, cacheRead: 120000, cacheWrite: 6000 }),
  ),
];

beforeEach(() => {
  indexed = [SESSION, CHILD_SESSION];
  eventsBySession = {
    [SESSION.id]: PARENT_EVENTS,
    [CHILD_SESSION_ID]: CHILD_EVENTS,
  };
  getEventPage.mockImplementation(async (sessionId: string) => eventsBySession[sessionId] ?? []);
  startRefresh.mockResolvedValue(undefined);
  resetSessionsStore();
});

afterEach(() => {
  cleanup();
  getEventPage.mockReset();
  startRefresh.mockReset();
  onProgress = null;
  onComplete = null;
  resetSessionsStore();
});

async function renderApp() {
  const { App } = await import("../../src/App");
  render(<App />);
}

/** A labelled field or stat, as the region its value can be asserted within. */
function field(scope: HTMLElement, label: string) {
  return within(scope).getByRole("group", { name: new RegExp(label, "i") });
}

describe("Journey 4: Review a session's token usage", () => {
  it("accounts for a session's spend and links its biggest moment to what caused it", async () => {
    await renderApp();

    await userEvent.click(await screen.findByRole("tab", { name: /^tokens$/i }));
    const pane = await screen.findByRole("region", { name: /^session token usage$/i });

    // Subagent spend is folded from the child's own transcript, so it arrives
    // after the linked child is read. The figure appears three times once it
    // settles: the tile, the spawn's row, and the top ranked moment.
    await screen.findAllByText("193,005");
    expect(getEventPage).toHaveBeenCalledWith(CHILD_SESSION_ID);

    // The orchestrator figure counts each response once — counting the two
    // records that share `msg-a` would report 64,109 — and the pane says so, so
    // a user can see the basis of the number rather than trusting it.
    expect(within(field(pane, "Orchestrator spend")).getByText("54,907")).toBeInTheDocument();
    expect(within(pane).queryByText("64,109")).toBeNull();
    expect(within(pane).getByText(/3 orchestrator responses recorded usage/i)).toBeInTheDocument();
    expect(within(pane).getByText(/1 repeated record collapsed/i)).toBeInTheDocument();

    // One delegation's spend cannot be reached, so the subagent total names the
    // shortfall rather than reading as complete.
    const subagentSpend = field(pane, "Subagent spend");
    expect(within(subagentSpend).getByText("193,005")).toBeInTheDocument();
    expect(within(subagentSpend).getByText(/1 spawn's spend is not linkable/i)).toBeInTheDocument();
    const sessionTotal = field(pane, "Session total");
    expect(within(sessionTotal).getByText("247,912")).toBeInTheDocument();

    // Dollars are the index's, not this lens's: each tile's price is the sum of
    // the `estimated_price_micros` on the rows it covers, so the parent's own
    // $0.15 and the linked child's $0.50 settle at $0.65 for the session. Every
    // figure is marked approximate, because Claude charged nothing this session
    // could quote, and the total's own label says so without repeating the
    // dollar figure a second time.
    expect(within(field(pane, "Orchestrator spend")).getByText("≈$0.1500")).toBeInTheDocument();
    expect(within(subagentSpend).getByText("≈$0.5000")).toBeInTheDocument();
    expect(within(sessionTotal).getByText("≈$0.6500")).toBeInTheDocument();
    expect(within(sessionTotal).getByText(/^estimated total$/i)).toBeInTheDocument();

    // Cached versus fresh, over the whole session's spend, so the composition
    // sums to the Session total above it. 231,000 of 247,912 tokens arrived as
    // context re-served from cache; fresh input is real but under 1%, and says
    // so rather than rounding to nothing.
    const composition = field(pane, "Session token composition");
    const cacheRead = field(composition, "Cache read");
    expect(within(cacheRead).getByText("231,000")).toBeInTheDocument();
    expect(within(cacheRead).getByText("93%")).toBeInTheDocument();
    const freshInput = field(composition, "Fresh input");
    expect(within(freshInput).getByText("12")).toBeInTheDocument();
    expect(within(freshInput).getByText("<1%")).toBeInTheDocument();
    expect(within(field(composition, "Output")).getByText("2,900")).toBeInTheDocument();
    expect(within(field(composition, "Cache write")).getByText("14,000")).toBeInTheDocument();

    // The two per-spawn numbers are different facts: what the child spent, and
    // how large its final turn's context was.
    const spawns = within(pane).getByRole("list", { name: /^spend by spawn$/i });
    const spawnRows = within(spawns).getAllByRole("listitem");
    expect(spawnRows).toHaveLength(2);

    const explore = spawnRows[0] as HTMLElement;
    expect(explore).toHaveTextContent(EXPLORE_PROMPT);
    expect(within(field(explore, "Outcome")).getByText(/completed/i)).toBeInTheDocument();
    expect(within(field(explore, "Child spend")).getByText("193,005")).toBeInTheDocument();
    expect(within(field(explore, "Final context")).getByText("39,856")).toBeInTheDocument();

    // A spawn the harness recorded no child transcript for says its spend is
    // unreachable, rather than substituting the final context it did record.
    const research = spawnRows[1] as HTMLElement;
    expect(research).toHaveTextContent(RESEARCH_PROMPT);
    expect(within(field(research, "Outcome")).getByText(/error/i)).toBeInTheDocument();
    expect(within(field(research, "Child spend")).getByText(/not linkable/i)).toBeInTheDocument();
    expect(within(field(research, "Final context")).getByText("12,480")).toBeInTheDocument();

    // Every unit of spend has a place on the message axis now, timestamp or
    // not, so there is nothing to reconcile and no exclusion note.
    const trend = field(pane, "Spend by message");
    expect(within(trend).queryByText(/excluded from the chart/i)).toBeNull();

    // The ranked moments are the accessible route to the top marks, and the only
    // assertable one: Recharts renders no SVG under jsdom. One entry per
    // response and per spawn that spent something, largest first, each labelled
    // with its kind because a spawn's amount is cumulative and a response's is a
    // single delta. The repeated `msg-a` record does not earn a fifth entry.
    const moments = within(trend).getByRole("list", { name: /^top spend moments$/i });
    const momentRows = within(moments).getAllByRole("listitem");
    expect(momentRows).toHaveLength(4);
    expect(momentRows[0]).toHaveTextContent(/subagent spawn/i);
    expect(momentRows[0]).toHaveTextContent(/explore/i);
    expect(momentRows[0]).toHaveTextContent("193,005");
    expect(momentRows[1]).toHaveTextContent(/assistant response/i);
    expect(momentRows[1]).toHaveTextContent("42,604");
    expect(momentRows[1]).toHaveTextContent(SPIKE_TEXT);

    // Closing the loop the journey exists for. A spawn moment hands the user to
    // the Subagents lens, expanded onto the child's own investigation, because
    // the Conversation's spawn row shows only the prompt and never what the
    // subagent did. Landing means focused *and* marked as the current location,
    // so the row is visibly highlighted rather than silently focused.
    await userEvent.click(
      within(momentRows[0] as HTMLElement).getByRole("button", { name: /explore/i }),
    );

    expect(screen.getByRole("tab", { name: /^subagents$/i })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    const subagentsLens = await screen.findByRole("region", { name: /^session subagents$/i });
    const landed = within(subagentsLens).getByRole("listitem", { current: "location" });
    expect(landed).toHaveTextContent(EXPLORE_PROMPT);
    expect(landed).toHaveFocus();
    expect(
      await within(landed).findByRole("region", { name: /^subagent session$/i }),
    ).toBeInTheDocument();
  });
});
