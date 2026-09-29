// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  type AillyEvent,
  EventKind,
  type SessionListItem,
  type SessionTokenFigures,
  type Subagent,
  type TokenUsage,
} from "../../src/tauri";
import { PanelTabs } from "../../src/ui/PanelTabs";
import {
  SessionsActionType,
  resetSessionsStore,
  useSessionsStore,
} from "../../src/ui/sessions/store";
import { TokensPane } from "../../src/ui/tokens/TokensPane";
import { LoadStatus } from "../../src/ui/useSessionEvents";

const getEventPage = vi.fn<(sessionId: string) => Promise<AillyEvent[]>>();

vi.mock("../../src/tauri", async (importActual) => {
  const actual = await importActual<typeof import("../../src/tauri")>();
  return { ...actual, getEventPage: (sessionId: string) => getEventPage(sessionId) };
});

const SESSION_ID = "claude_code:/home/parent.jsonl:one";
const FIRST_CHILD = "claude_code:/home/subagents/agent-1.jsonl:one";
const SECOND_CHILD = "claude_code:/home/subagents/agent-2.jsonl:one";

function baseEvent(id: string, ordinal: number, kind: EventKind): AillyEvent {
  return {
    id,
    session_id: SESSION_ID,
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
 * the child's own session, which is how the pane learns which rows to price for
 * the subagent half — a fixture that let them keep the parent's id would price
 * the parent twice.
 */
function responseEvent(
  id: string,
  ordinal: number,
  output: number,
  sessionId: string = SESSION_ID,
  model: string | null = null,
): AillyEvent {
  return {
    ...baseEvent(id, ordinal, EventKind.AssistantTurn),
    session_id: sessionId,
    response_id: { Recorded: `msg-${id}` },
    model: model === null ? "Absent" : { Recorded: model },
    turn: { Recorded: { role: "assistant", text: { Recorded: "talking" } } },
    token_usage: { Recorded: usage({ output: { Recorded: output } }) },
  };
}

/**
 * A session as the indexer wrote it. The pane's dollars come from these rows
 * alone: it folds the event page for the token split and never for a price, so
 * no model or rate appears in the events these tests build.
 */
function row(id: string, figures: Partial<SessionTokenFigures>): SessionListItem {
  return {
    id,
    harness: "claude_code",
    project: { Recorded: "ailly-analyzer" },
    event_count: 1,
    last_activity: "Absent",
    token_total: "Absent",
    recorded_price_micros: "Absent",
    estimated_tokens: "Absent",
    estimated_price_micros: "Absent",
    estimated_as_of: "Absent",
    ...figures,
  };
}

/** A row the index priced itself, having found no cost recorded on the events. */
function estimatedRow(id: string, tokens: number, micros: number): SessionListItem {
  return row(id, {
    estimated_tokens: { Recorded: tokens },
    estimated_price_micros: { Recorded: micros },
  });
}

function indexed(...sessions: SessionListItem[]) {
  useSessionsStore.getState().dispatch({ type: SessionsActionType.SessionsLoaded, sessions });
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

function linkedSpawn(prompt: string, childSessionId: string): Subagent {
  return {
    ...UNRECORDED,
    agent_type: { Recorded: "explore" },
    prompt: { Recorded: prompt },
    outcome: { Recorded: "completed" },
    token_usage: { Recorded: usage({ total: { Recorded: 39856 } }) },
    child_session_id: { Recorded: childSessionId },
  };
}

function spawnEvent(id: string, ordinal: number, subagent: Subagent): AillyEvent {
  return { ...baseEvent(id, ordinal, EventKind.SubagentSpawn), subagent: { Recorded: subagent } };
}

function renderPane(events: AillyEvent[]) {
  render(
    <PanelTabs defaultValue="tokens">
      <TokensPane state={{ status: LoadStatus.Ready, events }} />
    </PanelTabs>,
  );
}

async function pane() {
  return await screen.findByRole("region", { name: /^session token usage$/i });
}

function group(scope: HTMLElement, label: string) {
  return within(scope).getByRole("group", { name: new RegExp(label, "i") });
}

/** The per-spawn rows, scoped so the ranked-moments list cannot be mistaken for them. */
async function spawnRows() {
  const list = within(await pane()).getByRole("list", { name: /^spend by spawn$/i });
  return within(list).getAllByRole("listitem") as HTMLElement[];
}

beforeEach(() => {
  getEventPage.mockReset();
  resetSessionsStore();
});

afterEach(() => {
  cleanup();
  resetSessionsStore();
});

describe("TokensPane", () => {
  /**
   * The intermediate state the acceptance journey deliberately waits past. A
   * subagent total of zero while its transcript is still being read would be a
   * number the session never spent.
   */
  it("says it is still reading a child transcript rather than showing a premature zero", async () => {
    getEventPage.mockReturnValue(new Promise(() => {}));
    renderPane([
      responseEvent("evt-1", 1, 100),
      spawnEvent("evt-2", 2, linkedSpawn("Map", FIRST_CHILD)),
    ]);

    const subagentSpend = group(await pane(), "Subagent spend");
    expect(within(subagentSpend).getByText(/reading/i)).toBeInTheDocument();
    expect(within(subagentSpend).queryByText("0")).toBeNull();
  });

  it("settles to the child's own folded spend once its transcript resolves", async () => {
    getEventPage.mockResolvedValue([responseEvent("child-1", 1, 5000, FIRST_CHILD)]);
    renderPane([
      responseEvent("evt-1", 1, 100),
      spawnEvent("evt-2", 2, linkedSpawn("Map", FIRST_CHILD)),
    ]);

    const subagentSpend = group(await pane(), "Subagent spend");
    expect(await within(subagentSpend).findByText("5,000")).toBeInTheDocument();
    expect(within(group(await pane(), "Session total")).getByText("5,100")).toBeInTheDocument();
  });

  /**
   * A response's own model rides along beside its tokens and dollars, so a
   * session that mixes models never leaves a reader guessing which figure
   * came from which one.
   */
  it("names the model each response and its child transcript ran on", async () => {
    getEventPage.mockResolvedValue([
      responseEvent("child-1", 1, 5000, FIRST_CHILD, "claude-sonnet-5"),
    ]);
    renderPane([
      responseEvent("evt-1", 1, 100, SESSION_ID, "claude-opus-5"),
      spawnEvent("evt-2", 2, linkedSpawn("Map", FIRST_CHILD)),
    ]);

    const orchestratorSpend = group(await pane(), "Orchestrator spend");
    expect(within(orchestratorSpend).getByText(/model: claude-opus-5/i)).toBeInTheDocument();

    const [row] = await spawnRows();
    expect(await within(row as HTMLElement).findByText("claude-sonnet-5")).toBeInTheDocument();

    const moments = within(await pane()).getByRole("list", { name: /^top spend moments$/i });
    expect(within(moments).getByText("claude-opus-5")).toBeInTheDocument();
  });

  /**
   * Each linked spawn's subtree is read on its own, so one delegation's walk
   * never pulls in a sibling delegation's descendants and the two rows report
   * separate figures.
   */
  it("reads each linked spawn's own subtree rather than the whole page at once", async () => {
    getEventPage.mockImplementation(async (sessionId) =>
      sessionId === FIRST_CHILD
        ? [responseEvent("child-1", 1, 5000, FIRST_CHILD)]
        : [responseEvent("child-2", 1, 700, SECOND_CHILD)],
    );
    renderPane([
      spawnEvent("evt-1", 1, linkedSpawn("Map the parser", FIRST_CHILD)),
      spawnEvent("evt-2", 2, linkedSpawn("Trace the spec", SECOND_CHILD)),
    ]);

    const rows = await spawnRows();
    expect(await within(rows[0] as HTMLElement).findByText("5,000")).toBeInTheDocument();
    expect(await within(rows[1] as HTMLElement).findByText("700")).toBeInTheDocument();
    expect(getEventPage).toHaveBeenCalledWith(FIRST_CHILD);
    expect(getEventPage).toHaveBeenCalledWith(SECOND_CHILD);
  });

  /** A broken child collapses its own row's figure and nothing else. */
  it("reports a failed child read in that spawn's row only", async () => {
    getEventPage.mockImplementation(async (sessionId) => {
      if (sessionId === FIRST_CHILD) {
        throw new Error("no such session");
      }
      return [responseEvent("child-2", 1, 700, SECOND_CHILD)];
    });
    renderPane([
      responseEvent("evt-1", 1, 100),
      spawnEvent("evt-2", 2, linkedSpawn("Map the parser", FIRST_CHILD)),
      spawnEvent("evt-3", 3, linkedSpawn("Trace the spec", SECOND_CHILD)),
    ]);

    const rows = await spawnRows();
    expect(await within(rows[0] as HTMLElement).findByText(/no such session/i)).toBeInTheDocument();
    expect(await within(rows[1] as HTMLElement).findByText("700")).toBeInTheDocument();
    expect(within(group(await pane(), "Orchestrator spend")).getByText("100")).toBeInTheDocument();
  });

  /**
   * A spawn the harness named no child transcript for costs one round trip:
   * none. Its spend is unreachable, and the final context it did record is a
   * different fact that is never stood in for it.
   */
  it("marks a spawn with no named child unlinkable without attempting a read", async () => {
    renderPane([
      spawnEvent("evt-1", 1, {
        ...linkedSpawn("Map the parser", FIRST_CHILD),
        child_session_id: "Absent",
      }),
    ]);

    const [row] = await spawnRows();
    expect(
      within(group(row as HTMLElement, "Child spend")).getByText(/not linkable/i),
    ).toBeInTheDocument();
    expect(
      within(group(row as HTMLElement, "Final context")).getByText("39,856"),
    ).toBeInTheDocument();
    await waitFor(() => expect(getEventPage).not.toHaveBeenCalled());
  });

  /** Nothing recorded is one sentence, not a page of zeros and an empty composition. */
  it("says a session recorded no usage rather than composing zeros", async () => {
    renderPane([baseEvent("evt-1", 1, EventKind.UserTurn)]);

    const rendered = await pane();
    expect(within(rendered).getByText(/recorded no token usage/i)).toBeInTheDocument();
    expect(
      within(rendered).queryByRole("group", { name: /session token composition/i }),
    ).toBeNull();
  });

  /**
   * A token count on its own leaves a reader to do the rates in their head, so
   * every headline figure carries what it cost.
   *
   * The dollars are the index's, one row per party: the session's own row prices
   * the orchestrator half, and the linked child's row — a peer row in the index,
   * not a nested figure — prices the subagent half. The pane adds the rows it
   * covers and nothing else, which is why the two halves sum to the total
   * exactly rather than approximately.
   */
  it("shows what each half of the spend cost beside the tokens it bought", async () => {
    indexed(estimatedRow(SESSION_ID, 1000, 15_000), estimatedRow(FIRST_CHILD, 1000, 15_000));
    getEventPage.mockResolvedValue([responseEvent("child-1", 1, 1000, FIRST_CHILD)]);
    renderPane([
      responseEvent("evt-1", 1, 1000),
      spawnEvent("evt-2", 2, linkedSpawn("Map", FIRST_CHILD)),
    ]);

    const rendered = await pane();
    expect(
      await within(group(rendered, "Orchestrator spend")).findByText("≈$0.0150"),
    ).toBeInTheDocument();
    expect(
      await within(group(rendered, "Subagent spend")).findByText("≈$0.0150"),
    ).toBeInTheDocument();

    const total = group(rendered, "Session total");
    expect(await within(total).findByText("≈$0.0300")).toBeInTheDocument();
    // An estimate says it is one right in its own label, rather than a second
    // line repeating the dollar figure already shown above it.
    expect(within(total).getByText(/^estimated total$/i)).toBeInTheDocument();
  });

  /**
   * Pi is the only harness that writes what it charged, and the index keeps that
   * figure rather than replacing it with arithmetic. A price someone was billed
   * is a fact about the session, so it is stated without the approximation mark
   * an estimate carries and named as the harness's own.
   */
  it("states a harness-recorded price as a fact rather than as an estimate", async () => {
    indexed(
      row(SESSION_ID, {
        token_total: { Recorded: 1000 },
        recorded_price_micros: { Recorded: 17_900 },
      }),
    );
    renderPane([responseEvent("evt-1", 1, 1000)]);

    const total = group(await pane(), "Session total");
    expect(within(total).getByText("$0.0179")).toBeInTheDocument();
    expect(within(total).queryByText(/≈/)).toBeNull();
    expect(within(total).getByText(/^recorded total$/i)).toBeInTheDocument();
  });

  /**
   * A session priced by the harness and a child the index had to estimate do not
   * average into one basis. The total carries the approximation mark, because
   * part of it is arithmetic, and the note keeps the two bases separate so a
   * reader can see how much of the figure was actually billed.
   */
  it("keeps the two price bases apart when a session mixes them", async () => {
    indexed(
      row(SESSION_ID, {
        token_total: { Recorded: 1000 },
        recorded_price_micros: { Recorded: 17_900 },
      }),
      estimatedRow(FIRST_CHILD, 1000, 15_000),
    );
    getEventPage.mockResolvedValue([responseEvent("child-1", 1, 1000, FIRST_CHILD)]);
    renderPane([
      responseEvent("evt-1", 1, 1000),
      spawnEvent("evt-2", 2, linkedSpawn("Map", FIRST_CHILD)),
    ]);

    const total = group(await pane(), "Session total");
    expect(await within(total).findByText("≈$0.0329")).toBeInTheDocument();
    expect(
      within(total).getByText(/\$0\.0179 \(Recorded\); \$0\.0150 \(Est\)/i),
    ).toBeInTheDocument();
  });

  /**
   * A session the index could put no price against — an unpublished model, or a
   * session already too old to price when it was first scanned — keeps its
   * tokens and withholds its dollars. The tile names the shortfall rather than
   * reporting a fabricated `$0.00`.
   */
  it("names the spend it could not price rather than showing it as free", async () => {
    indexed(row(SESSION_ID, { token_total: { Recorded: 1000 } }));
    renderPane([responseEvent("evt-1", 1, 1000)]);

    const total = group(await pane(), "Session total");
    expect(within(total).getByText("1,000")).toBeInTheDocument();
    expect(within(total).getByText(/^not recorded$/i)).toBeInTheDocument();
    expect(within(total).getByText(/no price for 1 session/i)).toBeInTheDocument();
    expect(within(total).queryByText("$0.00")).toBeNull();
  });

  /** A figure the harness never wrote is named as absent, never rendered as zero. */
  it("names an unrecorded final context rather than showing it as zero", async () => {
    renderPane([
      spawnEvent("evt-1", 1, {
        ...linkedSpawn("Map the parser", FIRST_CHILD),
        token_usage: "Absent",
        child_session_id: "Absent",
      }),
    ]);

    const [row] = await spawnRows();
    expect(
      within(group(row as HTMLElement, "Final context")).getByText(/not recorded/i),
    ).toBeInTheDocument();
  });
});
