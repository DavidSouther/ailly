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

async function emitComplete(status: IndexStatus) {
  await waitFor(() => expect(onComplete).not.toBeNull());
  await act(async () => {
    onComplete?.(status);
  });
}

function visibleSessions() {
  return within(screen.getByRole("list", { name: "Sessions" })).getAllByRole("listitem");
}

/**
 * Given indexed sessions across harnesses,
 * When the user finds Rescan in the sessions panel, sees no harness selector,
 *   types `harness: codex`, then removes that chip,
 * Then Rescan busy behavior works in-panel, the list narrows to Codex with a
 *   removable chip, and removing the chip restores the full list.
 */
describe("Rescan in sessions panel and harness search chips", () => {
  it("keeps Rescan in the sessions panel, drops the harness select, and filters via harness: chips", async () => {
    indexed = SESSIONS;
    await renderApp();
    await waitFor(() => expect(visibleSessions()).toHaveLength(3));
    await emitComplete("idle");

    const panel = screen.getByLabelText("Sessions panel");
    const rescan = within(panel).getByRole("button", { name: "Rescan" });
    expect(rescan).toBeEnabled();
    expect(screen.queryByLabelText("Filter by harness")).not.toBeInTheDocument();
    expect(screen.queryByRole("combobox", { name: /harness/i })).not.toBeInTheDocument();

    await userEvent.click(rescan);
    await waitFor(() => {
      const busy = within(panel).getByRole("button", { name: /scanning|rescan/i });
      expect(busy).toBeDisabled();
      expect(busy).toHaveAttribute("aria-busy", "true");
    });
    await emitComplete("idle");
    await waitFor(() => {
      expect(within(panel).getByRole("button", { name: "Rescan" })).toBeEnabled();
    });

    await userEvent.clear(screen.getByLabelText("Filter sessions"));
    await userEvent.type(screen.getByLabelText("Filter sessions"), "harness: codex");

    await waitFor(() => expect(visibleSessions()).toHaveLength(1));
    expect(screen.getByText("billing-service")).toBeInTheDocument();
    expect(screen.queryByText("ailly-analyzer")).not.toBeInTheDocument();

    const removeChip = screen.getByRole("button", { name: /remove harness:\s*codex filter/i });
    expect(removeChip).toBeInTheDocument();
    await userEvent.click(removeChip);

    await waitFor(() => expect(visibleSessions()).toHaveLength(3));
    expect(
      screen.queryByRole("button", { name: /remove harness:\s*codex filter/i }),
    ).not.toBeInTheDocument();
    expect(screen.getByText("ailly-analyzer")).toBeInTheDocument();
    expect(screen.getByText("billing-service")).toBeInTheDocument();
  });
});
