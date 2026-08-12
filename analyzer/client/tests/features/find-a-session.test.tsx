// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type { IndexProgress, IndexStatus, SessionListItem } from "../../src/tauri";
import { resetSessionsStore } from "../../src/ui/sessions/store";

/** Sessions currently visible to `listSessions`, grown to simulate streaming. */
let indexed: SessionListItem[] = [];
const startRefresh = vi.fn<() => Promise<void>>();
let onProgress: ((progress: IndexProgress) => void) | null = null;
let onComplete: ((status: IndexStatus) => void) | null = null;

vi.mock("../../src/tauri", async (importActual) => {
  const actual = await importActual<typeof import("../../src/tauri")>();
  return {
    ...actual,
    listSessions: async () => indexed,
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

const SESSIONS: SessionListItem[] = [
  {
    id: "claude_code:/home/a.jsonl:one",
    harness: "claude_code",
    project: { Recorded: "ailly-analyzer" },
    event_count: 42,
    token_total: "Absent",
    last_activity: { Recorded: "2026-08-12T15:00:00Z" },
  },
  {
    id: "codex:/home/b.jsonl:two",
    harness: "codex",
    project: { Recorded: "billing-service" },
    event_count: 7,
    token_total: "Absent",
    last_activity: "Absent",
  },
  {
    id: "pi:/home/c.jsonl:three",
    harness: "pi",
    project: "Absent",
    event_count: 1,
    token_total: "Absent",
    last_activity: { Recorded: "2026-08-11T09:30:00Z" },
  },
];

beforeEach(() => {
  indexed = [];
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

function rescanButton() {
  return screen.getByRole("button", { name: /scanning|rescan/i });
}

async function emitProgress(progress: IndexProgress) {
  await waitFor(() => expect(onProgress).not.toBeNull());
  await act(async () => {
    onProgress?.(progress);
  });
}

async function emitComplete(status: IndexStatus) {
  await waitFor(() => expect(onComplete).not.toBeNull());
  await act(async () => {
    onComplete?.(status);
  });
}

function visibleSessions() {
  return within(screen.getByRole("list", { name: "Sessions" })).getAllByRole("listitem");
}

describe("Journey 1: Find a session", () => {
  it("shows a zero state with a disabled, spinning scan button while indexing", async () => {
    await renderApp();

    expect(await screen.findByText("Looking for sessions")).toBeInTheDocument();
    expect(rescanButton()).toBeDisabled();
    expect(rescanButton()).toHaveAttribute("aria-busy", "true");
    await waitFor(() => expect(startRefresh).toHaveBeenCalled());
  });

  it("streams sessions into the list as indexing progresses", async () => {
    await renderApp();
    await screen.findByText("Looking for sessions");

    indexed = [SESSIONS[0]!];
    await emitProgress({ indexed: 1, total: 3 });
    await waitFor(() => expect(visibleSessions()).toHaveLength(1));
    expect(screen.getByText("Scanning 1 of 3 sources…", { exact: false })).toBeInTheDocument();

    indexed = SESSIONS;
    await emitProgress({ indexed: 3, total: 3 });
    await waitFor(() => expect(visibleSessions()).toHaveLength(3));
  });

  it("re-enables the scan button when indexing completes", async () => {
    await renderApp();
    await screen.findByText("Looking for sessions");
    expect(rescanButton()).toBeDisabled();

    await emitComplete("idle");

    await waitFor(() => expect(screen.getByRole("button", { name: "Rescan" })).toBeEnabled());
  });

  it("lists every discovered session across harnesses", async () => {
    indexed = SESSIONS;
    await renderApp();

    await waitFor(() => expect(visibleSessions()).toHaveLength(3));
    const list = screen.getByRole("list", { name: "Sessions" });
    expect(within(list).getByText("ailly-analyzer")).toBeInTheDocument();
    expect(within(list).getByText("billing-service")).toBeInTheDocument();
    expect(within(list).getByText("Unknown project")).toBeInTheDocument();
  });

  it("narrows the list to a single harness", async () => {
    indexed = SESSIONS;
    await renderApp();
    await waitFor(() => expect(visibleSessions()).toHaveLength(3));

    await userEvent.selectOptions(screen.getByLabelText("Filter by harness"), "codex");

    expect(visibleSessions()).toHaveLength(1);
    expect(screen.getByText("billing-service")).toBeInTheDocument();
    expect(screen.queryByText("ailly-analyzer")).not.toBeInTheDocument();
  });

  it("filters sessions by project text search", async () => {
    indexed = SESSIONS;
    await renderApp();
    await waitFor(() => expect(visibleSessions()).toHaveLength(3));

    await userEvent.type(screen.getByLabelText("Filter sessions"), "billing");

    await waitFor(() => expect(visibleSessions()).toHaveLength(1));
    expect(screen.getByText("billing-service")).toBeInTheDocument();
  });

  it("shows a clear empty state when indexing finds no sessions", async () => {
    await renderApp();
    await screen.findByText("Looking for sessions");

    await emitComplete("idle");

    await waitFor(() => expect(screen.getByText("No sessions found")).toBeInTheDocument());
    expect(screen.queryByRole("list", { name: "Sessions" })).not.toBeInTheDocument();
  });

  it("surfaces a scan failure reported by the backend", async () => {
    await renderApp();
    await screen.findByText("Looking for sessions");

    await emitComplete({ error: { message: "index is locked" } });

    await waitFor(() => expect(screen.getByText("index is locked")).toBeInTheDocument());
    expect(screen.getByRole("button", { name: "Rescan" })).toBeEnabled();
  });
});
