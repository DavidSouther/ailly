// @vitest-environment jsdom
import "@testing-library/jest-dom/vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  type AillyEvent,
  EventKind,
  type SourceValue,
  type Subagent,
  type ToolCall,
} from "../../src/tauri";
import { SubagentsPane } from "../../src/ui/subagents/SubagentsPane";
import { promptPreview, subagentSpawnRows } from "../../src/ui/subagents/rollup";
import { LoadStatus } from "../../src/ui/useSessionEvents";

const getEventPage = vi.fn<(sessionId: string) => Promise<AillyEvent[]>>();

vi.mock("../../src/tauri", async (importActual) => {
  const actual = await importActual<typeof import("../../src/tauri")>();
  return { ...actual, getEventPage: (sessionId: string) => getEventPage(sessionId) };
});

const SESSION_ID = "claude_code:/home/parent.jsonl:one";
const FIRST_CHILD = "claude_code:/home/subagents/agent-1.jsonl:one";
const SECOND_CHILD = "claude_code:/home/subagents/agent-2.jsonl:one";

const EXPLORE_PROMPT = "Map the parser module";
const RESEARCH_PROMPT = "Trace the failing spec";

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
  } satisfies AillyEvent;
}

function spawnEvent(id: string, ordinal: number, subagent: SourceValue<Subagent>): AillyEvent {
  return { ...baseEvent(id, ordinal, EventKind.SubagentSpawn), subagent };
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
    duration_ms: { Recorded: 467407 },
    token_usage: {
      Recorded: {
        input: { Recorded: 4210 },
        output: { Recorded: 9330 },
        cache_read: "Absent",
        cache_write: "Absent",
        total: { Recorded: 128450 },
        cost_total_micros: "Absent",
        scope: "subagent",
      },
    },
    child_session_id: { Recorded: childSessionId },
  };
}

const UNLINKED_SPAWN: Subagent = {
  ...UNRECORDED,
  agent_type: { Recorded: "research" },
  prompt: { Recorded: RESEARCH_PROMPT },
  outcome: { Recorded: "error" },
};

function renderPane(events: AillyEvent[]) {
  render(<SubagentsPane state={{ status: LoadStatus.Ready, events }} />);
}

function rowNamed(prompt: string) {
  return screen.getByRole("button", { name: new RegExp(prompt) });
}

beforeEach(() => {
  getEventPage.mockResolvedValue([]);
});

afterEach(() => {
  cleanup();
  getEventPage.mockReset();
});

describe("subagentSpawnRows", () => {
  it("projects every recorded spawn and labels the dimensions a harness never wrote", () => {
    const events = [
      spawnEvent("evt-1", 1, { Recorded: linkedSpawn(EXPLORE_PROMPT, FIRST_CHILD) }),
      spawnEvent("evt-2", 2, { Recorded: UNLINKED_SPAWN }),
      toolEvent("evt-3", 3, { name: "Read" }),
    ];

    const rows = subagentSpawnRows(events);

    expect(rows).toHaveLength(2);
    expect(rows[0]?.durationLabel).toEqual({ Recorded: "7m 47s" });
    expect(rows[0]?.finalContextLabel).toEqual({ Recorded: "128,450" });
    expect(rows[1]?.durationLabel).toBe("Absent");
    expect(rows[1]?.finalContextLabel).toBe("Absent");
  });

  /// A payload the index could not read is not a delegation the product can
  /// describe, and "Malformed" is not a spawn with empty fields.
  it("leaves out a spawn whose payload was not recorded", () => {
    const rows = subagentSpawnRows([
      spawnEvent("evt-1", 1, "Malformed"),
      spawnEvent("evt-2", 2, "Absent"),
    ]);

    expect(rows).toEqual([]);
  });

  it("carries a spawn whose usage was recorded without a rolled-up total as unrecorded", () => {
    const partial: Subagent = {
      ...UNRECORDED,
      token_usage: {
        Recorded: {
          input: { Recorded: 10 },
          output: { Recorded: 2 },
          cache_read: "Absent",
          cache_write: "Absent",
          total: "Absent",
          cost_total_micros: "Absent",
          scope: "subagent",
        },
      },
    };

    const rows = subagentSpawnRows([spawnEvent("evt-1", 1, { Recorded: partial })]);

    expect(rows[0]?.finalContextLabel).toBe("Absent");
  });
});

describe("promptPreview", () => {
  it("keeps a short prompt intact", () => {
    expect(promptPreview("one\ntwo")).toBe("one\ntwo");
  });

  it("shows the first three lines of a longer prompt with an ellipsis", () => {
    expect(promptPreview("a\nb\nc\nd\ne")).toBe("a\nb\nc…");
  });
});

describe("SubagentsPane", () => {
  it("shows a truncated prompt until the row is expanded", async () => {
    const longPrompt = "line one\nline two\nline three\nline four\nline five";
    renderPane([spawnEvent("evt-1", 1, { Recorded: linkedSpawn(longPrompt, FIRST_CHILD) })]);

    const toggle = screen.getByRole("button", { name: longPrompt });
    expect(toggle).toHaveTextContent("line one");
    expect(toggle).toHaveTextContent("line three…");
    expect(toggle).not.toHaveTextContent("line four");

    await userEvent.click(toggle);
    expect(toggle).toHaveTextContent("line four");
    expect(toggle).toHaveTextContent("line five");
  });
  it("states the emptiness rather than showing an empty list", () => {
    renderPane([toolEvent("evt-1", 1, { name: "Read" })]);

    expect(screen.getByText(/recorded no subagent spawns/i)).toBeInTheDocument();
    expect(screen.queryByRole("list", { name: /subagent spawns/i })).toBeNull();
  });

  it("reads a child's events once however often its row is opened", async () => {
    renderPane([spawnEvent("evt-1", 1, { Recorded: linkedSpawn(EXPLORE_PROMPT, FIRST_CHILD) })]);

    await userEvent.click(rowNamed(EXPLORE_PROMPT));
    await screen.findByRole("region", { name: /subagent session/i });
    await userEvent.click(rowNamed(EXPLORE_PROMPT));
    await userEvent.click(rowNamed(EXPLORE_PROMPT));
    await screen.findByRole("region", { name: /subagent session/i });

    expect(getEventPage).toHaveBeenCalledTimes(1);
  });

  it("never reads anything for a spawn whose source named no child", async () => {
    renderPane([spawnEvent("evt-1", 1, { Recorded: UNLINKED_SPAWN })]);

    await userEvent.click(rowNamed(RESEARCH_PROMPT));

    expect(await screen.findByText(/child transcript not recorded/i)).toBeInTheDocument();
    expect(getEventPage).not.toHaveBeenCalled();
  });

  it("loads only the row the user opened, not its siblings", async () => {
    renderPane([
      spawnEvent("evt-1", 1, { Recorded: linkedSpawn(EXPLORE_PROMPT, FIRST_CHILD) }),
      spawnEvent("evt-2", 2, { Recorded: linkedSpawn(RESEARCH_PROMPT, SECOND_CHILD) }),
    ]);

    await userEvent.click(rowNamed(EXPLORE_PROMPT));
    await screen.findAllByRole("region", { name: /subagent session/i });
    await userEvent.click(rowNamed(EXPLORE_PROMPT));
    await userEvent.click(rowNamed(RESEARCH_PROMPT));
    await screen.findAllByRole("region", { name: /subagent session/i });

    expect(getEventPage.mock.calls.map(([id]) => id)).toEqual([FIRST_CHILD, SECOND_CHILD]);
  });

  /// One child that cannot be read is not a reason to break the list; the rest
  /// of the investigation stays available.
  it("reports a failed child read in place and leaves its siblings usable", async () => {
    getEventPage.mockImplementation(async (sessionId) => {
      if (sessionId === FIRST_CHILD) {
        throw "child transcript is gone";
      }
      return [];
    });
    renderPane([
      spawnEvent("evt-1", 1, { Recorded: linkedSpawn(EXPLORE_PROMPT, FIRST_CHILD) }),
      spawnEvent("evt-2", 2, { Recorded: linkedSpawn(RESEARCH_PROMPT, SECOND_CHILD) }),
    ]);

    await userEvent.click(rowNamed(EXPLORE_PROMPT));
    expect(await screen.findByText(/child transcript is gone/i)).toBeInTheDocument();

    await userEvent.click(rowNamed(RESEARCH_PROMPT));
    const nested = await screen.findAllByRole("region", {
      name: /subagent session/i,
    });
    expect(nested).toHaveLength(1);
  });

  it("opens a linked spawn into the same Summary / Conversation / Subagents lenses", async () => {
    renderPane([spawnEvent("evt-1", 1, { Recorded: linkedSpawn(EXPLORE_PROMPT, FIRST_CHILD) })]);

    await userEvent.click(rowNamed(EXPLORE_PROMPT));
    const nested = await screen.findByRole("region", { name: /subagent session/i });

    expect(within(nested).getByRole("tab", { name: /^summary$/i })).toBeInTheDocument();
    expect(within(nested).getByRole("tab", { name: /^conversation$/i })).toBeInTheDocument();
    expect(within(nested).getByRole("tab", { name: /^subagents$/i })).toBeInTheDocument();
  });

  it("names an unrecorded prompt rather than rendering a blank control", () => {
    renderPane([spawnEvent("evt-1", 1, { Recorded: UNRECORDED })]);

    const row = within(screen.getByRole("list", { name: /subagent spawns/i })).getByRole(
      "listitem",
    );
    expect(within(row).getByRole("button")).toHaveAccessibleName(/not recorded/i);
  });
});
