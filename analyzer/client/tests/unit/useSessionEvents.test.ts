// @vitest-environment jsdom
import { cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { type AillyEvent, EventKind } from "../../src/tauri";
import { LoadStatus, useSessionEvents } from "../../src/ui/useSessionEvents";

let getEventPage: (sessionId: string) => Promise<AillyEvent[]>;

vi.mock("../../src/tauri", async (importActual) => {
  const actual = await importActual<typeof import("../../src/tauri")>();
  return { ...actual, getEventPage: (sessionId: string) => getEventPage(sessionId) };
});

const EVENT: AillyEvent = {
  id: "evt-1",
  session_id: "one",
  kind: EventKind.UserTurn,
  source: { harness: "claude_code", path: "/home/a.jsonl", line: 1, ordinal: 1 },
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

beforeEach(() => {
  getEventPage = async () => [EVENT];
});

afterEach(cleanup);

describe("useSessionEvents", () => {
  it("reads the selected session's events once", async () => {
    const { result } = renderHook(() => useSessionEvents("one"));

    await waitFor(() => expect(result.current.status).toBe(LoadStatus.Ready));
    expect(result.current).toEqual({ status: LoadStatus.Ready, events: [EVENT] });
  });

  it("returns to idle when the selection is cleared", async () => {
    const { result, rerender } = renderHook(
      ({ id }: { id: string | null }) => useSessionEvents(id),
      {
        initialProps: { id: "one" as string | null },
      },
    );
    await waitFor(() => expect(result.current.status).toBe(LoadStatus.Ready));

    rerender({ id: null });

    expect(result.current.status).toBe(LoadStatus.Idle);
  });

  it("surfaces a failed read as an error state", async () => {
    getEventPage = async () => {
      throw "index is locked";
    };

    const { result } = renderHook(() => useSessionEvents("one"));

    await waitFor(() => expect(result.current.status).toBe(LoadStatus.Error));
    expect(result.current).toEqual({ status: LoadStatus.Error, message: "index is locked" });
  });
});
