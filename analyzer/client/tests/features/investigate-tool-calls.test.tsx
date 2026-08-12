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
  type ToolCall,
} from "../../src/tauri";
import { resetSessionsStore } from "../../src/ui/sessions/store";

let indexed: SessionListItem[] = [];
let eventsBySession: Record<string, AillyEvent[]> = {};
const startRefresh = vi.fn<() => Promise<void>>();
let onProgress: ((progress: IndexProgress) => void) | null = null;
let onComplete: ((status: IndexStatus) => void) | null = null;

vi.mock("../../src/tauri", async (importActual) => {
  const actual = await importActual<typeof import("../../src/tauri")>();
  return {
    ...actual,
    listSessions: async () => indexed,
    getEventPage: async (sessionId: string) => eventsBySession[sessionId] ?? [],
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

const SESSION: SessionListItem = {
  id: "claude_code:/home/a.jsonl:one",
  harness: "claude_code",
  project: { Recorded: "ailly-analyzer" },
  event_count: 8,
  token_total: "Absent",
  last_activity: { Recorded: "2026-08-12T15:00:00Z" },
};

const SUSPECT_FILE = "packages/auth/src/session.ts";
const SHELL_OUTPUT = "packages/auth/src/session.ts\npackages/auth/src/legacy.ts";

function baseEvent(id: string, ordinal: number, kind: EventKind): AillyEvent {
  return {
    id,
    session_id: SESSION.id,
    kind,
    source: { harness: "claude_code", path: "/home/a.jsonl", line: ordinal, ordinal },
    native_id: "Absent",
    timestamp: "Absent",
    turn: "Absent",
    tool_call: "Absent",
    tool_result: "Absent",
    token_usage: "Absent",
    files: "Absent",
    detail: "Absent",
  };
}

function toolEvent(id: string, ordinal: number, tool: Partial<ToolCall> & { name: string }) {
  return {
    ...baseEvent(id, ordinal, EventKind.ToolCall),
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

/**
 * Six tool calls: two reads, one shell, one web fetch with an unrecorded URL,
 * one edit of the suspect file, and one tool the category table cannot know.
 * No timestamps and no subagent spawns, so duration and spawns are unavailable.
 */
const EVENTS: AillyEvent[] = [
  {
    ...baseEvent("evt-1", 1, EventKind.UserTurn),
    turn: { Recorded: { role: "user", text: { Recorded: "Migrate the legacy auth service" } } },
  },
  toolEvent("evt-2", 2, { name: "Read", path: { Recorded: SUSPECT_FILE } }),
  toolEvent("evt-3", 3, {
    name: "Bash",
    command: { Recorded: "rg -l LegacySession" },
    cwd: { Recorded: "/Users/dev/other-repo" },
    call_id: { Recorded: "call-bash" },
  }),
  {
    ...baseEvent("evt-3-result", 4, EventKind.ToolResult),
    tool_result: {
      Recorded: {
        call_id: { Recorded: "call-bash" },
        output: { Recorded: SHELL_OUTPUT },
        is_error: "Absent",
      },
    },
  },
  toolEvent("evt-4", 4, {
    name: "WebFetch",
    input: { Recorded: '{"url":"https://api.example.com/openapi.json"}' },
  }),
  toolEvent("evt-5", 5, { name: "Edit", path: { Recorded: SUSPECT_FILE } }),
  toolEvent("evt-6", 6, { name: "Read", path: { Recorded: "docs/auth/runbook.md" } }),
  toolEvent("evt-7", 7, { name: "mcp__acme__lookup" }),
  {
    ...baseEvent("evt-8", 8, EventKind.AssistantTurn),
    turn: { Recorded: { role: "assistant", text: { Recorded: "Migration complete." } } },
  },
];

beforeEach(() => {
  indexed = [SESSION];
  eventsBySession = { [SESSION.id]: EVENTS };
  startRefresh.mockResolvedValue(undefined);
  resetSessionsStore();
});

afterEach(() => {
  cleanup();
  startRefresh.mockReset();
  onProgress = null;
  onComplete = null;
  resetSessionsStore();
});

async function renderApp() {
  const { App } = await import("../../src/App");
  render(<App />);
}

function summary() {
  return screen.getByRole("region", { name: /session summary/i });
}

/** The tile showing `label`, as the region a value can be asserted within. */
function tile(label: string) {
  return within(summary()).getByRole("group", { name: new RegExp(label, "i") });
}

async function expand(scope: HTMLElement, name: RegExp) {
  await userEvent.click(within(scope).getByRole("button", { name }));
}

describe("Journey 2: Investigate a single session's tool calls", () => {
  it("summarizes a session's tool calls, sources, and files touched", async () => {
    await renderApp();

    // Selecting a session opens the investigation lens first, with the
    // transcript one tab away.
    expect(await screen.findByRole("tab", { name: /summary/i })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByRole("tab", { name: /conversation/i })).toHaveAttribute(
      "aria-selected",
      "false",
    );

    // At a glance: how big was this session, and what could it not tell us?
    expect(within(tile("Tool calls")).getByText("6")).toBeInTheDocument();
    expect(within(tile("Files touched")).getByText("2")).toBeInTheDocument();
    expect(within(tile("Duration")).getByText(/not recorded/i)).toBeInTheDocument();
    expect(within(tile("Subagent spawns")).getByText(/not recorded/i)).toBeInTheDocument();

    // Where this session ran, as a session-level fact.
    expect(within(tile("Working directory")).getByText("ailly-analyzer")).toBeInTheDocument();

    // The shape of the work, as proportions, with unknown tools kept visible
    // instead of folded into "other".
    const categories = within(summary()).getByRole("group", { name: /tool category split/i });
    expect(within(categories).getByText(/read.*33%/i)).toBeInTheDocument();
    expect(within(categories).getByText(/exec.*17%/i)).toBeInTheDocument();
    expect(within(categories).getByText(/edit.*17%/i)).toBeInTheDocument();
    expect(within(categories).getByText(/unclassified.*17%/i)).toBeInTheDocument();

    // Calls by tool, ranked, with raw harness names preserved.
    const byTool = within(summary()).getByRole("list", { name: /calls by tool/i });
    const toolRows = within(byTool).getAllByRole("listitem");
    expect(toolRows[0]).toHaveTextContent(/Read/);
    expect(toolRows[0]).toHaveTextContent(/2 calls/);
    expect(within(byTool).getByText("mcp__acme__lookup")).toBeInTheDocument();

    // Sources: every way an outside fact entered context, with the unrecorded
    // web target admitted as unrecorded.
    const sources = within(summary()).getByRole("group", { name: /sources/i });
    await expand(sources, /shell output/i);
    expect(within(sources).getByText("rg -l LegacySession")).toBeInTheDocument();
    // The call ran somewhere other than the session's own directory, which is
    // exactly the kind of thing that explains a surprising result.
    expect(within(sources).getByText(/\/Users\/dev\/other-repo/)).toBeInTheDocument();

    // The command itself expands to what it captured — the fact that decides
    // whether the session went wrong here.
    await expand(sources, /rg -l LegacySession/);
    expect(within(sources).getByText(/packages\/auth\/src\/legacy\.ts/)).toBeInTheDocument();

    const fileAccess = within(sources).getByRole("group", { name: /file access/i });
    await expand(fileAccess, /file access/i);
    // File access expands to the ranked files-touched list: path, tools, and
    // how often each was touched.
    const files = within(fileAccess).getByRole("list", { name: /files touched/i });
    const topFile = within(files).getAllByRole("listitem")[0] as HTMLElement;
    expect(topFile).toHaveTextContent(SUSPECT_FILE);
    expect(topFile).toHaveTextContent(/2 touches/);
    expect(within(topFile).getByText("Read")).toBeInTheDocument();
    expect(within(topFile).getByText("Edit")).toBeInTheDocument();
    expect(within(fileAccess).getByText("docs/auth/runbook.md")).toBeInTheDocument();

    const web = within(sources).getByRole("group", { name: /web/i });
    await expand(web, /web/i);
    expect(within(web).getByText(/target not recorded/i)).toBeInTheDocument();

    // Nothing in this session answered the web call, and the pane says so
    // rather than showing an empty box.
    await expand(web, /target not recorded/i);
    expect(within(web).getByText(/output not recorded/i)).toBeInTheDocument();
  });
});
