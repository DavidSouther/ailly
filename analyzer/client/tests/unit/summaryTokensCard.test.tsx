// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  type AillyEvent,
  EventKind,
  type SessionListItem,
  type SessionTokenFigures,
  type Subagent,
  type TokenUsage,
} from "../../src/tauri";
import {
  SessionsActionType,
  resetSessionsStore,
  useSessionsStore,
} from "../../src/ui/sessions/store";
import { SummaryPane } from "../../src/ui/summary/SummaryPane";
import { LoadStatus } from "../../src/ui/useSessionEvents";

const getEventPage = vi.fn<(sessionId: string) => Promise<AillyEvent[]>>();

vi.mock("../../src/tauri", async (importActual) => {
  const actual = await importActual<typeof import("../../src/tauri")>();
  return { ...actual, getEventPage: (sessionId: string) => getEventPage(sessionId) };
});

const SESSION_ID = "claude_code:/home/parent.jsonl:one";
const CHILD_SESSION_ID = "claude_code:/home/subagents/agent-1.jsonl:one";

function baseEvent(
  id: string,
  ordinal: number,
  kind: EventKind,
  sessionId: string = SESSION_ID,
): AillyEvent {
  return {
    id,
    session_id: sessionId,
    kind,
    source: { harness: "claude_code", path: "/home/parent.jsonl", line: ordinal, ordinal },
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
  };
}

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

/**
 * A response, in the session that recorded it. A child transcript's events name
 * the child's own session, which is how the card learns whose index row prices
 * the subagent half of the spend.
 */
function responseEvent(
  id: string,
  ordinal: number,
  responseId: string,
  parts: { input: number; output: number; cacheRead: number; cacheWrite: number },
  sessionId: string = SESSION_ID,
): AillyEvent {
  return {
    ...baseEvent(id, ordinal, EventKind.AssistantTurn, sessionId),
    response_id: { Recorded: responseId },
    turn: { Recorded: { role: "assistant", text: { Recorded: "talking" } } },
    token_usage: {
      Recorded: usage({
        input: { Recorded: parts.input },
        output: { Recorded: parts.output },
        cache_read: { Recorded: parts.cacheRead },
        cache_write: { Recorded: parts.cacheWrite },
      }),
    },
  };
}

const UNRECORDED: Subagent = {
  native_id: "Absent",
  agent_type: "Absent",
  prompt: "Absent",
  outcome: "Absent",
  nickname: "Absent",
  duration_ms: "Absent",
  token_usage: "Absent",
  child_session_id: "Absent",
};

function linkedSpawn(childSessionId: string): Subagent {
  return {
    ...UNRECORDED,
    agent_type: { Recorded: "explore" },
    prompt: { Recorded: "Map the parser" },
    outcome: { Recorded: "completed" },
    token_usage: { Recorded: usage({ total: { Recorded: 39856 } }) },
    child_session_id: { Recorded: childSessionId },
  };
}

function spawnEvent(id: string, ordinal: number, subagent: Subagent): AillyEvent {
  return { ...baseEvent(id, ordinal, EventKind.SubagentSpawn), subagent: { Recorded: subagent } };
}

function renderSummary(events: AillyEvent[]) {
  render(
    <SummaryPane
      state={{ status: LoadStatus.Ready, events }}
      project={{ Recorded: "ailly-analyzer" }}
    />,
  );
}

const NO_FIGURES: SessionTokenFigures = {
  token_total: "Absent",
  recorded_price_micros: "Absent",
  estimated_tokens: "Absent",
  estimated_price_micros: "Absent",
  estimated_as_of: "Absent",
};

/**
 * The session's own indexed row, which is where the card's dollars come from —
 * the events it also renders carry token usage but never a price.
 */
function indexRow(figures: Partial<SessionTokenFigures>): SessionListItem {
  return {
    id: SESSION_ID,
    harness: "claude_code",
    project: { Recorded: "ailly-analyzer" },
    event_count: 1,
    last_activity: "Absent",
    ...NO_FIGURES,
    ...figures,
  };
}

function indexed(...sessions: SessionListItem[]) {
  useSessionsStore.getState().dispatch({ type: SessionsActionType.SessionsLoaded, sessions });
}

async function summary() {
  return await screen.findByRole("region", { name: /^session summary$/i });
}

function group(scope: HTMLElement, label: string) {
  return within(scope).getByRole("group", { name: new RegExp(`^${label}$`, "i") });
}

async function tokensCard() {
  return group(await summary(), "Tokens");
}

beforeEach(() => {
  getEventPage.mockReset();
  resetSessionsStore();
});

afterEach(() => {
  cleanup();
  resetSessionsStore();
});

describe("Summary lens: Tokens card", () => {
  /**
   * The session's own spend: 100 (evt-1) + 4 fresh + 600 output + 40,000 cache
   * read + 2,000 cache write (evt-2) = 42,704, folded from the parent page
   * alone, with a linked spawn that has already resolved so the total is
   * settled at 42,704 + 5,000 = 47,704.
   */
  it("shows the tool tiles and a settled Tokens card in the same summary", async () => {
    getEventPage.mockResolvedValue([
      responseEvent(
        "child-1",
        1,
        "cmsg-a",
        { input: 0, output: 5000, cacheRead: 0, cacheWrite: 0 },
        CHILD_SESSION_ID,
      ),
    ]);
    renderSummary([
      responseEvent("evt-1", 1, "msg-a", { input: 100, output: 0, cacheRead: 0, cacheWrite: 0 }),
      responseEvent("evt-2", 2, "msg-b", {
        input: 4,
        output: 600,
        cacheRead: 40000,
        cacheWrite: 2000,
      }),
      spawnEvent("evt-3", 3, linkedSpawn(CHILD_SESSION_ID)),
    ]);

    const region = await summary();

    // The regular 2x2 stat block is unaffected by the new card beside it.
    expect(within(group(region, "Tool calls")).getByText("0")).toBeInTheDocument();
    expect(within(group(region, "Files touched")).getByText("0")).toBeInTheDocument();
    expect(within(group(region, "Duration")).getByText(/not recorded/i)).toBeInTheDocument();
    expect(within(group(region, "Subagent spawns")).getByText("1")).toBeInTheDocument();

    // The Tokens card lives in the same landmark region as the tiles.
    const tokens = await tokensCard();

    expect(await within(tokens).findByText("47,704")).toBeInTheDocument();
    expect(within(group(tokens, "Session total")).getByText("47,704")).toBeInTheDocument();

    const orchestrator = group(tokens, "Orchestrator");
    expect(within(orchestrator).getByText("42,704")).toBeInTheDocument();

    const subagents = group(tokens, "Subagents");
    expect(within(subagents).getByText("5,000")).toBeInTheDocument();

    // The exact composition Journey 4 already established, not a fork of it:
    // fresh input, output, cache read, and cache write, each with its amount.
    expect(within(group(tokens, "Fresh input")).getByText("104")).toBeInTheDocument();
    expect(within(group(tokens, "Output")).getByText("5,600")).toBeInTheDocument();
    expect(within(group(tokens, "Cache read")).getByText("40,000")).toBeInTheDocument();
    expect(within(group(tokens, "Cache write")).getByText("2,000")).toBeInTheDocument();

    // The deferred content-category breakdown must not appear anywhere in
    // this card: no per-content-block labels, only the four honest buckets.
    expect(within(tokens).queryByText(/thinking/i)).toBeNull();
    expect(within(tokens).queryByText(/system prompt/i)).toBeNull();
    expect(within(tokens).queryByText(/user messages?/i)).toBeNull();
    expect(within(tokens).queryByText(/^responses$/i)).toBeNull();
  });

  /**
   * The intermediate state the acceptance journey for Journey 4 deliberately
   * waits past: a total of 5,000 tokens while the child transcript is still
   * being read would be a number the session never spent, so the card must
   * say it is still reading rather than showing a partial final figure.
   */
  it("stays honest while a linked child transcript is still loading", async () => {
    getEventPage.mockReturnValue(new Promise(() => {}));
    renderSummary([
      responseEvent("evt-1", 1, "msg-a", { input: 100, output: 0, cacheRead: 0, cacheWrite: 0 }),
      spawnEvent("evt-2", 2, linkedSpawn(CHILD_SESSION_ID)),
    ]);

    const tokens = await tokensCard();
    // No premature final total: the orchestrator-only sum must not stand in
    // for the session or subagent totals while the child is still being read.
    expect(within(group(tokens, "Session total")).getByText(/reading/i)).toBeInTheDocument();
    expect(within(group(tokens, "Session total")).queryByText("100")).toBeNull();
    expect(within(group(tokens, "Subagents")).getByText(/reading/i)).toBeInTheDocument();
    expect(within(group(tokens, "Subagents")).queryByText(/^0$/)).toBeNull();
  });

  /**
   * A spawn the harness named no child transcript for costs no round trip and
   * never blocks the rest of the card: the session total settles immediately
   * from what is known, and the shortfall is named rather than hidden.
   */
  it("names an unlinkable spawn's spend rather than silently omitting it", async () => {
    renderSummary([
      responseEvent("evt-1", 1, "msg-a", { input: 100, output: 0, cacheRead: 0, cacheWrite: 0 }),
      spawnEvent("evt-2", 2, { ...linkedSpawn(CHILD_SESSION_ID), child_session_id: "Absent" }),
    ]);

    const tokens = await tokensCard();
    expect(within(group(tokens, "Session total")).getByText("100")).toBeInTheDocument();
    expect(
      within(group(tokens, "Subagents")).getByText(/spawn.*not linkable/i),
    ).toBeInTheDocument();
    await Promise.resolve();
    expect(getEventPage).not.toHaveBeenCalled();
  });

  /**
   * The card shows a token count and a price together, because either alone
   * invites a guess: the count at unknown rates, the price on an unknown basis.
   *
   * The price is the indexer's, read off the session's own row rather than
   * multiplied out here: Claude charges nothing this card could quote, so the
   * index priced the session against its pinned rate table when it first scanned
   * it. That this card owns no rate is the point — it and the session list
   * cannot quote different dollars for one session.
   */
  it("shows the session's price beside its tokens, labelled as an estimate", async () => {
    indexed(
      indexRow({
        estimated_tokens: { Recorded: 1000 },
        estimated_price_micros: { Recorded: 15_000 },
        estimated_as_of: { Recorded: "2026-08-14" },
      }),
    );
    renderSummary([
      responseEvent("evt-1", 1, "msg-a", { input: 0, output: 1000, cacheRead: 0, cacheWrite: 0 }),
    ]);

    const total = group(await tokensCard(), "Session total");
    expect(within(total).getByText("1,000")).toBeInTheDocument();
    expect(within(total).getByText("≈$0.0150")).toBeInTheDocument();
    // The label itself says this rests on an estimate; a lone estimate needs no
    // second line repeating the dollar figure with its rate-table date.
    expect(within(total).getByText(/^estimated total$/i)).toBeInTheDocument();
  });

  /**
   * A harness that charged for the session states the price outright, so the
   * card quotes it plainly and names the harness as its source. This is the
   * branch the index's recorded-wins rule exists to protect: an estimate must
   * never stand in front of a figure someone was actually billed.
   */
  it("quotes a recorded price as a fact rather than an estimate", async () => {
    indexed(
      indexRow({ token_total: { Recorded: 1000 }, recorded_price_micros: { Recorded: 17_870 } }),
    );
    renderSummary([
      responseEvent("evt-1", 1, "msg-a", { input: 0, output: 1000, cacheRead: 0, cacheWrite: 0 }),
    ]);

    const total = group(await tokensCard(), "Session total");
    expect(within(total).getByText("$0.0179")).toBeInTheDocument();
    expect(within(total).queryByText(/≈/)).toBeNull();
    expect(within(total).getByText(/^recorded total$/i)).toBeInTheDocument();
  });

  /**
   * A session the index could put no price against keeps its tokens and
   * withholds its dollars. `$0.00` would be a claim about the spend; the honest
   * claim is that neither the harness nor the index could say — whether because
   * no rate for the model is known, or because the session was already too old
   * to price when it was first scanned.
   */
  it("withholds a price it cannot derive rather than reporting zero dollars", async () => {
    indexed(indexRow({ token_total: { Recorded: 1000 } }));
    renderSummary([
      responseEvent("evt-1", 1, "msg-a", { input: 0, output: 1000, cacheRead: 0, cacheWrite: 0 }),
    ]);

    const total = group(await tokensCard(), "Session total");
    expect(within(total).getByText("1,000")).toBeInTheDocument();
    expect(within(total).getByText(/^not recorded$/i)).toBeInTheDocument();
    expect(within(total).queryByText("$0.00")).toBeNull();
    expect(within(total).getByText(/no price for 1 session/i)).toBeInTheDocument();
  });

  /** Nothing recorded reads as one sentence, in the Tokens card specifically. */
  it("says the Tokens card recorded no usage rather than composing zeros", async () => {
    renderSummary([{ ...baseEvent("evt-1", 1, EventKind.UserTurn) }]);

    const tokens = await tokensCard();
    expect(within(tokens).getByText(/recorded no token usage/i)).toBeInTheDocument();
    expect(within(tokens).queryByText(/session total/i)).toBeNull();
  });
});
