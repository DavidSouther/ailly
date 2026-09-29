// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  type AillyEvent,
  EventKind,
  type IndexProgress,
  type IndexStatus,
  type SessionListItem,
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
  event_count: 5,
  token_total: "Absent",
  recorded_price_micros: "Absent",
  estimated_tokens: "Absent",
  estimated_price_micros: "Absent",
  estimated_as_of: "Absent",
  last_activity: { Recorded: "2026-08-12T15:00:00Z" },
};

function baseEvent(id: string, ordinal: number, kind: EventKind): AillyEvent {
  return {
    id,
    session_id: SESSION.id,
    kind,
    source: { harness: "claude_code", path: "/home/a.jsonl", line: ordinal, ordinal },
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

const EVENTS: AillyEvent[] = [
  {
    ...baseEvent("evt-1", 1, EventKind.UserTurn),
    turn: { Recorded: { role: "user", text: { Recorded: "Please refactor the parser" } } },
  },
  {
    ...baseEvent("evt-2", 2, EventKind.AssistantTurn),
    turn: {
      Recorded: { role: "assistant", text: { Recorded: "On it — running the tests first." } },
    },
  },
  {
    ...baseEvent("evt-3", 3, EventKind.ToolCall),
    tool_call: {
      Recorded: {
        name: "Shell",
        call_id: "Absent",
        input: { Recorded: '{"command":"cargo test"}' },
        command: { Recorded: "cargo test" },
        path: "Absent",
        url: "Absent",
        cwd: "Absent",
      },
    },
  },
  {
    ...baseEvent("evt-3-result", 4, EventKind.ToolResult),
    tool_result: {
      Recorded: {
        call_id: { Recorded: "call-1" },
        output: { Recorded: "test result: ok. 42 passed; 0 failed" },
        is_error: "Absent",
      },
    },
  },
  {
    ...baseEvent("evt-4", 4, EventKind.SubagentSpawn),
    detail: { Recorded: "explore: map the parser module" },
  },
  {
    ...baseEvent("evt-5", 5, EventKind.AssistantTurn),
    turn: { Recorded: { role: "assistant", text: { Recorded: "All tests pass." } } },
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

function conversation() {
  return screen.getByRole("region", { name: /conversation/i });
}

/** Selecting a session opens the Summary tab; the transcript is one click away. */
async function openConversation() {
  await userEvent.click(await screen.findByRole("tab", { name: /conversation/i }));
  return await screen.findByRole("region", { name: /conversation/i });
}

describe("Journey 6: Read the complete conversation", () => {
  it("renders user turns, assistant text, tool calls, and subagents in source order", async () => {
    await renderApp();

    const convo = await openConversation();
    await within(convo).findByText("Please refactor the parser");

    const text = convo.textContent ?? "";
    expect(text.indexOf("Please refactor the parser")).toBeGreaterThanOrEqual(0);
    expect(text.indexOf("Please refactor the parser")).toBeLessThan(text.indexOf("On it"));
    expect(text.indexOf("On it")).toBeLessThan(text.indexOf("Shell"));
    expect(text.indexOf("Shell")).toBeLessThan(text.indexOf("Tool result"));
    expect(text.indexOf("Tool result")).toBeLessThan(text.indexOf("All tests pass."));
  });

  it("keeps tool call parameters collapsed until expanded on demand", async () => {
    await renderApp();
    const convo = await openConversation();
    await within(convo).findByText("Please refactor the parser");

    expect(within(convo).getByText("Shell")).toBeInTheDocument();
    expect(within(convo).queryByText("cargo test")).not.toBeInTheDocument();

    const toggle = within(convo).getByRole("button", { name: /shell/i });
    expect(toggle).toHaveAttribute("aria-expanded", "false");

    await userEvent.click(toggle);

    expect(await within(convo).findByText("cargo test")).toBeInTheDocument();
    expect(toggle).toHaveAttribute("aria-expanded", "true");
  });

  it("keeps a tool result's captured output collapsed until expanded on demand", async () => {
    await renderApp();
    const convo = await openConversation();
    await within(convo).findByText("Please refactor the parser");

    expect(within(convo).queryByText(/42 passed/)).not.toBeInTheDocument();

    const toggle = within(convo).getByRole("button", { name: /tool result/i });
    await userEvent.click(toggle);

    expect(await within(convo).findByText(/42 passed/)).toBeInTheDocument();
  });

  it("represents a subagent spawn with expandable nested activity", async () => {
    await renderApp();
    const convo = await openConversation();
    await within(convo).findByText("Please refactor the parser");

    const toggle = within(convo).getByRole("button", { name: /subagent/i });
    expect(within(convo).queryByText(/map the parser module/)).not.toBeInTheDocument();

    await userEvent.click(toggle);

    expect(await within(convo).findByText(/map the parser module/)).toBeInTheDocument();
  });

  it("anchors each rendered event so other views can link to it", async () => {
    await renderApp();
    const convo = await openConversation();
    await within(convo).findByText("Please refactor the parser");

    expect(convo.querySelectorAll('[id^="event-"]')).toHaveLength(EVENTS.length);
  });

  it("shows a placeholder when no session is selected", async () => {
    indexed = [];
    await renderApp();
    await screen.findByText("Looking for sessions");

    await waitFor(() => expect(onComplete).not.toBeNull());
    onComplete?.("idle");

    await waitFor(() =>
      expect(within(conversation()).getByText(/select a session/i)).toBeInTheDocument(),
    );
  });
});
