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
  type ToolCall,
} from "../../src/tauri";
import { resetSessionsStore } from "../../src/ui/sessions/store";

let indexed: SessionListItem[] = [];
let eventsBySession: Record<string, AillyEvent[]> = {};
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
 * The delegation facts a harness may record, as the design specifies them: each
 * dimension recorded independently, so a harness that wrote no duration reads
 * as absent rather than zero. Declared here because the feature that adds them
 * to `AillyEvent` does not exist yet.
 */
interface SubagentTokenUsage {
  input: SourceValue<number>;
  output: SourceValue<number>;
  cache_read: SourceValue<number>;
  cache_write: SourceValue<number>;
  total: SourceValue<number>;
  scope: string;
}

interface Subagent {
  native_id: SourceValue<string>;
  agent_type: SourceValue<string>;
  prompt: SourceValue<string>;
  outcome: SourceValue<string>;
  nickname: SourceValue<string>;
  duration_ms: SourceValue<number>;
  token_usage: SourceValue<SubagentTokenUsage>;
  /** The indexed child session this spawn produced, when the source named one. */
  child_session_id: SourceValue<string>;
}

type SubagentEvent = AillyEvent & { subagent: SourceValue<Subagent> };

const SESSION: SessionListItem = {
  id: "claude_code:/home/parent.jsonl:one",
  harness: "claude_code",
  project: { Recorded: "ailly-analyzer" },
  event_count: 6,
  token_total: "Absent",
  last_activity: { Recorded: "2026-08-12T15:00:00Z" },
};

const CHILD_SESSION_ID = "claude_code:/home/subagents/agent-a3c304e0.jsonl:one";

const LEXER = "src/parser/lexer.rs";
const TOKENS = "src/parser/token.rs";

const EXPLORE_PROMPT = "Map the parser module and report every entry point";
const RESEARCH_PROMPT = "Trace the failing spec back to its fixture";

function baseEvent(id: string, ordinal: number, kind: EventKind, sessionId: string): SubagentEvent {
  return {
    id,
    session_id: sessionId,
    kind,
    source: { harness: "claude_code", path: "/home/parent.jsonl", line: ordinal, ordinal },
    native_id: "Absent",
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

function toolEvent(
  id: string,
  ordinal: number,
  sessionId: string,
  tool: Partial<ToolCall> & { name: string },
): SubagentEvent {
  return {
    ...baseEvent(id, ordinal, EventKind.ToolCall, sessionId),
    tool_call: {
      Recorded: {
        call_id: "Absent",
        input: "Absent",
        command: "Absent",
        path: "Absent",
        url: "Absent",
        cwd: "Absent",
        ...tool,
      } satisfies ToolCall,
    },
  };
}

function spawnEvent(id: string, ordinal: number, subagent: Subagent): SubagentEvent {
  return {
    ...baseEvent(id, ordinal, EventKind.SubagentSpawn, SESSION.id),
    subagent: { Recorded: subagent },
  };
}

/**
 * A Claude spawn, which records every dimension the journey asks for and names
 * the child session it produced.
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
      input: { Recorded: 4210 },
      output: { Recorded: 9330 },
      cache_read: { Recorded: 112000 },
      cache_write: { Recorded: 2910 },
      total: { Recorded: 128450 },
      scope: "subagent",
    },
  },
  child_session_id: { Recorded: CHILD_SESSION_ID },
};

/**
 * A Pi spawn, which records the delegation and its outcome but no duration, no
 * tokens, and no child transcript to open.
 */
const UNLINKED_SPAWN: Subagent = {
  native_id: "Absent",
  agent_type: { Recorded: "research" },
  prompt: { Recorded: RESEARCH_PROMPT },
  outcome: { Recorded: "error" },
  nickname: "Absent",
  duration_ms: "Absent",
  token_usage: "Absent",
  child_session_id: "Absent",
};

const PARENT_EVENTS: SubagentEvent[] = [
  {
    ...baseEvent("evt-1", 1, EventKind.UserTurn, SESSION.id),
    turn: { Recorded: { role: "user", text: { Recorded: "Why is the parser spec failing?" } } },
  },
  spawnEvent("evt-2", 2, LINKED_SPAWN),
  spawnEvent("evt-3", 3, UNLINKED_SPAWN),
  toolEvent("evt-4", 4, SESSION.id, { name: "Read", path: { Recorded: "README.md" } }),
  {
    ...baseEvent("evt-5", 5, EventKind.AssistantTurn, SESSION.id),
    turn: { Recorded: { role: "assistant", text: { Recorded: "The lexer drops a token." } } },
  },
];

/** What the linked child actually did: three calls across two files. */
const CHILD_EVENTS: SubagentEvent[] = [
  toolEvent("child-1", 1, CHILD_SESSION_ID, { name: "Read", path: { Recorded: LEXER } }),
  toolEvent("child-2", 2, CHILD_SESSION_ID, { name: "Read", path: { Recorded: TOKENS } }),
  toolEvent("child-3", 3, CHILD_SESSION_ID, {
    name: "Bash",
    command: { Recorded: "cargo test -p parser" },
  }),
];

beforeEach(() => {
  indexed = [SESSION];
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

describe("Journey 5: Review a session's subagents", () => {
  it("lists every recorded spawn and opens a linked one into the child's own investigation", async () => {
    await renderApp();

    // The session's own summary now admits that part of this work happened
    // somewhere else, which is what sends the user to the Subagents tab.
    const summary = await screen.findByRole("region", { name: /session summary/i });
    expect(within(field(summary, "Subagent spawns")).getByText("2")).toBeInTheDocument();

    await userEvent.click(await screen.findByRole("tab", { name: /subagents/i }));
    const pane = await screen.findByRole("region", { name: /session subagents/i });

    const spawns = within(pane).getByRole("list", { name: /subagent spawns/i });
    const rows = within(spawns).getAllByRole("listitem");
    expect(rows).toHaveLength(2);

    // A fully recorded delegation: what it was, what it was asked, how long it
    // took, how it ended, and what it cost.
    const linked = rows[0] as HTMLElement;
    expect(linked).toHaveTextContent(EXPLORE_PROMPT);
    expect(within(field(linked, "Agent type")).getByText("explore")).toBeInTheDocument();
    expect(within(field(linked, "Duration")).getByText("7m 47s")).toBeInTheDocument();
    expect(within(field(linked, "Outcome")).getByText(/completed/i)).toBeInTheDocument();
    expect(within(field(linked, "Tokens")).getByText("128,450")).toBeInTheDocument();

    // A delegation the harness recorded less about. The unrecorded dimensions
    // are named as unrecorded rather than shown as zero.
    const unlinked = rows[1] as HTMLElement;
    expect(unlinked).toHaveTextContent(RESEARCH_PROMPT);
    expect(within(field(unlinked, "Agent type")).getByText("research")).toBeInTheDocument();
    expect(within(field(unlinked, "Duration")).getByText(/not recorded/i)).toBeInTheDocument();
    expect(within(field(unlinked, "Tokens")).getByText(/not recorded/i)).toBeInTheDocument();
    expect(within(field(unlinked, "Outcome")).getByText(/error/i)).toBeInTheDocument();

    // Opening the suspect spawn answers "what did the agent I could not see
    // actually do", using the same Summary / Conversation / Subagents lenses
    // the parent session uses.
    await userEvent.click(within(linked).getByRole("button", { name: new RegExp(EXPLORE_PROMPT) }));
    const nested = await within(linked).findByRole("region", { name: /subagent session/i });
    expect(within(nested).getByRole("tab", { name: /^summary$/i })).toBeInTheDocument();
    expect(within(nested).getByRole("tab", { name: /^conversation$/i })).toBeInTheDocument();
    expect(within(nested).getByRole("tab", { name: /^subagents$/i })).toBeInTheDocument();

    const childSummary = within(nested).getByRole("region", { name: /session summary/i });
    expect(within(field(childSummary, "Tool calls")).getByText("3")).toBeInTheDocument();
    expect(within(field(childSummary, "Files touched")).getByText("2")).toBeInTheDocument();

    const byTool = within(childSummary).getByRole("list", { name: /calls by tool/i });
    const topTool = within(byTool).getAllByRole("listitem")[0] as HTMLElement;
    expect(topTool).toHaveTextContent(/Read/);
    expect(topTool).toHaveTextContent(/2 calls/);

    await userEvent.click(within(childSummary).getByRole("button", { name: /^Read/ }));
    const readCalls = within(childSummary).getByRole("list", { name: /^Read calls$/i });
    expect(within(readCalls).getByText(LEXER)).toBeInTheDocument();
    expect(within(readCalls).getByText(TOKENS)).toBeInTheDocument();

    // The child's events are read once, on demand, and the parent's page is not
    // re-read to show them.
    expect(getEventPage).toHaveBeenCalledWith(CHILD_SESSION_ID);

    // The spawn with no recorded child says so, and offers nothing it cannot
    // support. Nothing is linked by adjacency.
    await userEvent.click(
      within(unlinked).getByRole("button", { name: new RegExp(RESEARCH_PROMPT) }),
    );
    expect(await within(unlinked).findByText(/child transcript not recorded/i)).toBeInTheDocument();
    expect(within(unlinked).queryByRole("region", { name: /subagent session/i })).toBeNull();
  });

  it("folds descendant tool calls into Calls by Tool when Include subagent tools is on", async () => {
    await renderApp();

    const summary = await screen.findByRole("region", { name: /session summary/i });
    const spawns = field(summary, "Subagent spawns");
    expect(within(spawns).getByText("2")).toBeInTheDocument();

    // Parent-only breakdown before the toggle: one Read on the parent.
    const before = within(summary).getByRole("list", { name: /calls by tool/i });
    expect(within(before).getAllByRole("listitem")).toHaveLength(1);
    expect(within(before).getByText("Read", { selector: ".text-foreground-title" })).toBeTruthy();
    expect(within(before).queryByText("Bash", { selector: ".text-foreground-title" })).toBeNull();

    await userEvent.click(
      within(spawns).getByRole("checkbox", { name: /include subagent tools/i }),
    );

    const after = await within(summary).findByRole("list", { name: /calls by tool/i });
    const toolRows = within(after).getAllByRole("listitem");
    // Parent Read + child Reads (2) + child Bash → Read 3, Bash 1.
    expect(toolRows[0]).toHaveTextContent(/Read/);
    expect(toolRows[0]).toHaveTextContent(/3 calls/);
    expect(toolRows[1]).toHaveTextContent(/Bash/);
    expect(toolRows[1]).toHaveTextContent(/1 calls/);

    // The category split describes the same calls the list does, so exec shows
    // up only once the child's Bash call is folded in.
    const categories = within(summary).getByRole("group", { name: /tool category split/i });
    expect(within(categories).getByText(/read.*75%/i)).toBeInTheDocument();
    expect(within(categories).getByText(/exec.*25%/i)).toBeInTheDocument();

    // The tiles stay parent-only: including children changes the breakdown, not
    // how many calls this session made or how often it delegated.
    expect(within(field(summary, "Tool calls")).getByText("1")).toBeInTheDocument();
    expect(within(field(summary, "Subagent spawns")).getByText("2")).toBeInTheDocument();
  });
});
