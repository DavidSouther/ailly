// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { type AillyEvent, EventKind, type Subagent, type TokenUsage } from "../../src/tauri";
import {
  PanelTabs,
  PanelTabsContent,
  PanelTabsList,
  PanelTabsTrigger,
} from "../../src/ui/PanelTabs";
import { Conversation } from "../../src/ui/conversation/Conversation";
import { SubagentsPane } from "../../src/ui/subagents/SubagentsPane";
import { TokensPane } from "../../src/ui/tokens/TokensPane";
import { LoadStatus } from "../../src/ui/useSessionEvents";

const getEventPage = vi.fn<(sessionId: string) => Promise<AillyEvent[]>>();

vi.mock("../../src/tauri", async (importActual) => {
  const actual = await importActual<typeof import("../../src/tauri")>();
  return { ...actual, getEventPage: (sessionId: string) => getEventPage(sessionId) };
});

const SESSION_ID = "claude_code:/home/parent.jsonl:one";
const CHILD_ID = "claude_code:/home/subagents/agent-1.jsonl:one";
const EXPLORE_PROMPT = "Map the parser module";
const SPIKE_TEXT = "Synthesizing everything the explore agent found.";

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

function responseEvent(
  id: string,
  ordinal: number,
  output: number,
  text: string,
  timestamp: string | null = "2026-08-14T10:00:00Z",
): AillyEvent {
  return {
    ...baseEvent(id, ordinal, EventKind.AssistantTurn),
    response_id: { Recorded: `msg-${id}` },
    timestamp: timestamp === null ? "Absent" : { Recorded: timestamp },
    turn: { Recorded: { role: "assistant", text: { Recorded: text } } },
    token_usage: { Recorded: usage({ output: { Recorded: output } }) },
  };
}

const LINKED: Subagent = {
  native_id: "Absent",
  agent_type: { Recorded: "explore" },
  prompt: { Recorded: EXPLORE_PROMPT },
  outcome: { Recorded: "completed" },
  nickname: "Absent",
  duration_ms: "Absent",
  token_usage: { Recorded: usage({ total: { Recorded: 39856 } }) },
  child_session_id: { Recorded: CHILD_ID },
};

const PARENT_EVENTS: AillyEvent[] = [
  responseEvent("evt-1", 1, 500, SPIKE_TEXT),
  {
    ...baseEvent("evt-2", 2, EventKind.SubagentSpawn),
    timestamp: { Recorded: "2026-08-14T10:01:00Z" },
    subagent: { Recorded: LINKED },
  },
];

/** The three lenses a moment can hand a user to, sharing one tab context. */
function renderLenses(events: AillyEvent[] = PARENT_EVENTS) {
  const state = { status: LoadStatus.Ready as const, events };
  render(
    <PanelTabs defaultValue="tokens">
      <PanelTabsList>
        <PanelTabsTrigger value="conversation" label="Conversation" />
        <PanelTabsTrigger value="subagents" label="Subagents" />
        <PanelTabsTrigger value="tokens" label="Tokens" />
      </PanelTabsList>
      <PanelTabsContent value="conversation">
        <Conversation state={state} />
      </PanelTabsContent>
      <PanelTabsContent value="subagents">
        <SubagentsPane state={state} />
      </PanelTabsContent>
      <PanelTabsContent value="tokens">
        <TokensPane state={state} />
      </PanelTabsContent>
    </PanelTabs>,
  );
}

async function momentRows() {
  const list = await screen.findByRole("list", { name: /^top spend moments$/i });
  return within(list).getAllByRole("listitem") as HTMLElement[];
}

beforeEach(() => {
  getEventPage.mockResolvedValue([responseEvent("child-1", 1, 5000, "Reading the lexer.")]);
});

afterEach(() => {
  cleanup();
});

describe("the spike-to-cause link", () => {
  /**
   * The journey's closing move. The Conversation's spawn row shows only the
   * prompt, so a spawn moment has to open the Subagents lens on the child's own
   * investigation rather than the parent's mention of it.
   */
  it("hands a spawn moment to the Subagents lens, expanded and landed on", async () => {
    renderLenses();
    const rows = await momentRows();

    await userEvent.click(within(rows[0] as HTMLElement).getByRole("button", { name: /explore/i }));

    expect(screen.getByRole("tab", { name: /^subagents$/i })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    const landed = within(
      await screen.findByRole("region", { name: /^session subagents$/i }),
    ).getByRole("listitem", { current: "location" });
    expect(landed).toHaveTextContent(EXPLORE_PROMPT);
    expect(landed).toHaveFocus();
    expect(
      await within(landed).findByRole("region", { name: /^subagent session$/i }),
    ).toBeInTheDocument();
  });

  /** A response is explained by the turn itself, so it lands in the transcript. */
  it("hands a response moment to the Conversation, without touching Subagents", async () => {
    renderLenses();
    const rows = await momentRows();
    const response = rows.find((row) => /assistant response/i.test(row.textContent ?? ""));

    await userEvent.click(within(response as HTMLElement).getByRole("button"));

    expect(screen.getByRole("tab", { name: /^conversation$/i })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    const landed = within(await screen.findByRole("region", { name: /^conversation$/i })).getByRole(
      "listitem",
      { current: "location" },
    );
    expect(landed).toHaveTextContent(SPIKE_TEXT);
    expect(landed).toHaveFocus();
  });

  /**
   * A stale target would replay the landing on an ordinary tab click, so the
   * pane that consumed it clears it.
   */
  it("does not land again when the user comes back by clicking the tab", async () => {
    renderLenses();
    const rows = await momentRows();
    await userEvent.click(within(rows[0] as HTMLElement).getByRole("button", { name: /explore/i }));
    await screen.findByRole("region", { name: /^session subagents$/i });

    await userEvent.click(screen.getByRole("tab", { name: /^tokens$/i }));
    await userEvent.click(screen.getByRole("tab", { name: /^subagents$/i }));

    const subagents = await screen.findByRole("region", { name: /^session subagents$/i });
    expect(within(subagents).queryByRole("listitem", { current: "location" })).toBeNull();
  });
});

describe("Top spend moments", () => {
  /** Largest first, each labelled with its kind: the two amounts mean different things. */
  it("ranks every moment with a known amount, labelled by kind", async () => {
    renderLenses();
    const rows = await momentRows();

    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent(/subagent spawn/i);
    expect(rows[0]).toHaveTextContent("5,000");
    expect(rows[1]).toHaveTextContent(/assistant response/i);
    expect(rows[1]).toHaveTextContent("500");
  });

  /** An unlinkable spawn has no amount to rank, so it appears in the table only. */
  it("leaves a spawn with no reachable spend out of the ranking", async () => {
    renderLenses([
      PARENT_EVENTS[0] as AillyEvent,
      {
        ...baseEvent("evt-2", 2, EventKind.SubagentSpawn),
        subagent: { Recorded: { ...LINKED, child_session_id: "Absent" } },
      },
    ]);

    const rows = await momentRows();
    expect(rows).toHaveLength(1);
    expect(rows[0]).toHaveTextContent(/assistant response/i);
  });
});

describe("the chart's exclusion note", () => {
  /** Exactly one excluded unit reads in the singular. */
  it("names a single untimestamped unit in the singular", async () => {
    renderLenses([responseEvent("evt-1", 1, 500, SPIKE_TEXT, null)]);

    const trend = await screen.findByRole("group", { name: /^spend over session time$/i });
    expect(
      within(trend).getByText(/1 event without a recorded timestamp is excluded/i),
    ).toBeInTheDocument();
  });

  it("names several untimestamped units in the plural", async () => {
    renderLenses([
      responseEvent("evt-1", 1, 500, SPIKE_TEXT, null),
      responseEvent("evt-2", 2, 300, "Also untimed.", null),
    ]);

    const trend = await screen.findByRole("group", { name: /^spend over session time$/i });
    expect(
      within(trend).getByText(/2 events without a recorded timestamp are excluded/i),
    ).toBeInTheDocument();
  });

  /** Nothing to reconcile means no note at all, rather than a note saying zero. */
  it("says nothing when every unit could be placed on the axis", async () => {
    renderLenses([responseEvent("evt-1", 1, 500, SPIKE_TEXT)]);

    const trend = await screen.findByRole("group", { name: /^spend over session time$/i });
    expect(within(trend).queryByText(/excluded/i)).toBeNull();
  });
});
