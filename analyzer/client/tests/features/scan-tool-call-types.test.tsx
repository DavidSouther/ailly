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
  project: { Recorded: "/Users/dev/repo" },
  event_count: 7,
  token_total: "Absent",
  recorded_price_micros: "Absent",
  estimated_tokens: "Absent",
  estimated_price_micros: "Absent",
  estimated_as_of: "Absent",
  last_activity: { Recorded: "2026-08-13T15:00:00Z" },
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
 * One call of each kind the glyph has to tell apart: a shell command, a write
 * and a read of the same file (which share a category but not a shape), a
 * search, a plan update the category table calls "other", and one MCP tool the
 * product has never heard of.
 */
const EVENTS: AillyEvent[] = [
  {
    ...baseEvent("evt-1", 1, EventKind.UserTurn),
    turn: { Recorded: { role: "user", text: { Recorded: "Add the session TTL" } } },
  },
  toolEvent("evt-2", 2, { name: "Bash", command: { Recorded: "cargo test" } }),
  toolEvent("evt-3", 3, { name: "Write", path: { Recorded: "packages/auth/src/ttl.ts" } }),
  toolEvent("evt-4", 4, { name: "Read", path: { Recorded: "packages/auth/src/ttl.ts" } }),
  toolEvent("evt-5", 5, { name: "Grep", path: { Recorded: "packages/auth" } }),
  toolEvent("evt-6", 6, { name: "update_plan" }),
  toolEvent("evt-7", 7, { name: "mcp__acme__lookup" }),
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

const CHEVRON_GLYPHS = ["lucide-chevron-down", "lucide-chevron-right"];

/**
 * The type glyphs a row leads with, named as a reader would name them, ignoring
 * the disclosure chevron every row already carries. `lucide-react` stamps each
 * icon's own name into its class list, so the glyph on screen can be named here
 * without adding presentational test ids to the product.
 */
function glyphs(row: HTMLElement): string[] {
  return [...row.querySelectorAll("svg.lucide")].flatMap((icon) =>
    [...icon.classList].filter(
      (name) => name.startsWith("lucide-") && !CHEVRON_GLYPHS.includes(name),
    ),
  );
}

describe("Scanning a session by tool call type", () => {
  it("gives each tool call a glyph for its kind, and an unknown tool a neutral one", async () => {
    await renderApp();

    // Summary: the ranked "Calls by tool" list, where the tool name is the row's
    // identity and the glyph is what makes the list scannable.
    const summary = await screen.findByRole("region", { name: /session summary/i });
    const byTool = within(summary).getByRole("list", { name: /calls by tool/i });
    const summaryRow = (name: RegExp) => within(byTool).getByRole("button", { name });

    expect(glyphs(summaryRow(/^Bash/))).toEqual(["lucide-square-terminal"]);
    expect(glyphs(summaryRow(/^Grep/))).toEqual(["lucide-search"]);
    expect(glyphs(summaryRow(/^update_plan/))).toEqual(["lucide-list-checks"]);

    // Writing a file and reading it are the same file, and different work.
    expect(glyphs(summaryRow(/^Write/))).toEqual(["lucide-file-plus"]);
    expect(glyphs(summaryRow(/^Read/))).toEqual(["lucide-text"]);

    // A tool no table knows looks unknown rather than guessed from its spelling.
    expect(glyphs(summaryRow(/^mcp__acme__lookup/))).toEqual(["lucide-wrench"]);

    // Conversation: the same calls in transcript order, each collapsed row
    // recognizable before it is read.
    await userEvent.click(await screen.findByRole("tab", { name: /conversation/i }));
    const convo = await screen.findByRole("region", { name: /conversation/i });
    const convoRow = (name: RegExp) => within(convo).getByRole("button", { name });

    expect(glyphs(convoRow(/^Bash$/))).toEqual(["lucide-square-terminal"]);
    expect(glyphs(convoRow(/^Write$/))).toEqual(["lucide-file-plus"]);
    expect(glyphs(convoRow(/^Read$/))).toEqual(["lucide-text"]);
    expect(glyphs(convoRow(/^mcp__acme__lookup$/))).toEqual(["lucide-wrench"]);

    // The glyph is decoration beside a name that already says it, so it adds
    // nothing for a screen reader to announce.
    const bashRow = convoRow(/^Bash$/);
    expect(bashRow).toHaveAccessibleName("Bash");
    expect(bashRow.querySelector("svg.lucide-square-terminal")).toHaveAttribute(
      "aria-hidden",
      "true",
    );
    expect(summaryRow(/^mcp__acme__lookup/).querySelector("svg.lucide-wrench")).toHaveAttribute(
      "aria-hidden",
      "true",
    );
  });
});
