import { describe, expect, it, vi } from "vitest";

import { type AillyEvent, EventKind, type Subagent, type ToolCall } from "../../src/tauri";
import { loadDescendantEvents, namedChildSessionIds } from "../../src/ui/subagents/descendants";

const ROOT = "root";
const CHILD = "child";
const GRANDCHILD = "grandchild";

function base(id: string, sessionId: string, kind: EventKind): AillyEvent {
  return {
    id,
    session_id: sessionId,
    kind,
    source: { harness: "claude_code", path: "/x.jsonl", line: 1, ordinal: 1 },
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

function spawn(id: string, sessionId: string, childSessionId: string): AillyEvent {
  const subagent: Subagent = {
    native_id: "Absent",
    agent_type: { Recorded: "explore" },
    prompt: { Recorded: "go" },
    outcome: "Absent",
    nickname: "Absent",
    duration_ms: "Absent",
    token_usage: "Absent",
    child_session_id: { Recorded: childSessionId },
  };
  return { ...base(id, sessionId, EventKind.SubagentSpawn), subagent: { Recorded: subagent } };
}

function tool(id: string, sessionId: string, name: string): AillyEvent {
  const tool_call: ToolCall = {
    name,
    call_id: "Absent",
    input: "Absent",
    command: "Absent",
    path: "Absent",
    url: "Absent",
    cwd: "Absent",
  };
  return {
    ...base(id, sessionId, EventKind.ToolCall),
    tool_call: { Recorded: tool_call },
  };
}

describe("namedChildSessionIds", () => {
  it("returns each recorded child once, in source order", () => {
    const events = [
      spawn("a", ROOT, CHILD),
      tool("b", ROOT, "Read"),
      spawn("c", ROOT, GRANDCHILD),
      spawn("d", ROOT, CHILD),
    ];

    expect(namedChildSessionIds(events)).toEqual([CHILD, GRANDCHILD]);
  });

  it("skips a spawn that named no child", () => {
    const unlinked: Subagent = {
      native_id: "Absent",
      agent_type: { Recorded: "research" },
      prompt: { Recorded: "go" },
      outcome: "Absent",
      nickname: "Absent",
      duration_ms: "Absent",
      token_usage: "Absent",
      child_session_id: "Absent",
    };
    expect(
      namedChildSessionIds([
        { ...base("a", ROOT, EventKind.SubagentSpawn), subagent: { Recorded: unlinked } },
      ]),
    ).toEqual([]);
  });
});

describe("loadDescendantEvents", () => {
  it("walks spawn links recursively and returns every descendant event once", async () => {
    const pages: Record<string, AillyEvent[]> = {
      [CHILD]: [tool("c1", CHILD, "Bash"), spawn("c2", CHILD, GRANDCHILD)],
      [GRANDCHILD]: [tool("g1", GRANDCHILD, "Read")],
    };
    const fetchPage = vi.fn(async (id: string) => pages[id] ?? []);

    const events = await loadDescendantEvents([spawn("r1", ROOT, CHILD)], fetchPage);

    expect(fetchPage.mock.calls.map(([id]) => id)).toEqual([CHILD, GRANDCHILD]);
    expect(events.map((event) => event.id)).toEqual(["c1", "c2", "g1"]);
  });

  it("does not loop when a child names an already-visited ancestor", async () => {
    const pages: Record<string, AillyEvent[]> = {
      [CHILD]: [spawn("c1", CHILD, ROOT), tool("c2", CHILD, "Bash")],
      [ROOT]: [spawn("r1", ROOT, CHILD)],
    };
    const fetchPage = vi.fn(async (id: string) => pages[id] ?? []);

    const events = await loadDescendantEvents(pages[ROOT] ?? [], fetchPage);

    expect(fetchPage).toHaveBeenCalledTimes(1);
    expect(events.map((event) => event.id)).toEqual(["c1", "c2"]);
  });
});
