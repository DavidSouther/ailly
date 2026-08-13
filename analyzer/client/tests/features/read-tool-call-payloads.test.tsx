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
  event_count: 5,
  token_total: "Absent",
  last_activity: { Recorded: "2026-08-13T15:00:00Z" },
};

const EDITED_FILE = "packages/auth/src/session.ts";
const OLD_SOURCE = "function expiry() {\n  return 3600;\n}";
const NEW_SOURCE = "function expiry() {\n  return SESSION_TTL;\n}";
const PATCH_BODY =
  "*** Begin Patch\n*** Update File: shell/src/loader/mod.rs\n-    let ttl = 3600;\n+    let ttl = SESSION_TTL;\n*** End Patch";

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
 * The four payload shapes this feature has to tell apart: an object payload
 * whose interesting fields are escaped strings (Claude `Edit`), a payload
 * recorded as a bare JSON string (a Codex `apply_patch`), a payload that only
 * repeats a field already on screen (`Bash`), and a payload carrying a genuine
 * extra beside its path (`Read`'s window).
 */
const EVENTS: AillyEvent[] = [
  toolEvent("evt-1", 1, {
    name: "Edit",
    path: { Recorded: EDITED_FILE },
    input: {
      Recorded: JSON.stringify({
        file_path: EDITED_FILE,
        old_string: OLD_SOURCE,
        new_string: NEW_SOURCE,
      }),
    },
  }),
  toolEvent("evt-2", 2, {
    name: "apply_patch",
    input: { Recorded: JSON.stringify(PATCH_BODY) },
  }),
  toolEvent("evt-3", 3, {
    name: "Bash",
    command: { Recorded: "cargo test" },
    input: { Recorded: JSON.stringify({ command: "cargo test" }) },
  }),
  toolEvent("evt-4", 4, {
    name: "Read",
    path: { Recorded: "docs/auth/runbook.md" },
    input: {
      Recorded: JSON.stringify({ file_path: "docs/auth/runbook.md", offset: 100, limit: 50 }),
    },
  }),
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

async function openConversation() {
  await userEvent.click(await screen.findByRole("tab", { name: /conversation/i }));
  return await screen.findByRole("region", { name: /conversation/i });
}

async function expand(scope: HTMLElement, name: RegExp) {
  await userEvent.click(within(scope).getByRole("button", { name }));
}

describe("Reading what a write or edit call actually did", () => {
  it("decodes recorded parameters in both lenses, without repeating what is already shown", async () => {
    await renderApp();

    // Summary: the lens Journey 2 starts a wrong-turn investigation from.
    const summary = await screen.findByRole("region", { name: /session summary/i });
    const byTool = within(summary).getByRole("list", { name: /calls by tool/i });

    await expand(byTool, /^Edit/);
    const editCalls = within(byTool).getByRole("list", { name: /^Edit calls$/i });
    await expand(editCalls, new RegExp(EDITED_FILE));

    // The edit's own strings, as source lines rather than one escaped line.
    const editPayload = within(editCalls).getByText(/old_string/);
    expect(editPayload).toHaveTextContent("return 3600;");
    expect(editPayload).toHaveTextContent("return SESSION_TTL;");
    expect(editPayload.textContent).not.toContain("\\n");
    // The path is already the row's own label, so it does not repeat below it.
    expect(editPayload.textContent).not.toContain("file_path");

    // Conversation: the same call, read in transcript order.
    const convo = await openConversation();
    await expand(convo, /^Edit$/);
    const conversationPayload = within(convo).getByText(/old_string/);
    expect(conversationPayload).toHaveTextContent("return SESSION_TTL;");
    expect(conversationPayload.textContent).not.toContain("\\n");

    // A patch recorded as a bare JSON string reads as the patch, not its encoding.
    await expand(convo, /^apply_patch$/);
    const patch = within(convo).getByText(/Begin Patch/);
    expect(patch.textContent).toContain("+    let ttl = SESSION_TTL;");
    expect(patch.textContent).not.toContain("\\n");

    // A shell call's payload is only its command, which the Command row already
    // shows, so expanding it adds no second copy.
    await expand(convo, /^Bash$/);
    const bashRow = within(convo).getByRole("button", { name: /^Bash$/ })
      .parentElement as HTMLElement;
    expect(within(bashRow).getAllByText("cargo test")).toHaveLength(1);
    expect(within(bashRow).queryByText(/^Parameters$/)).not.toBeInTheDocument();

    // A read's window is a recorded fact about what entered context, and it is
    // the part of the payload the Path row does not already carry.
    await expand(convo, /^Read$/);
    const readPayload = within(convo).getByText(/offset/);
    expect(readPayload).toHaveTextContent("offset: 100");
    expect(readPayload).toHaveTextContent("limit: 50");
  });
});
