import { describe, expect, it } from "vitest";

import { type AillyEvent, EventKind, type Subagent, type ToolCall } from "../../src/tauri";
import {
  shouldShowToolCwd,
  subagentDetail,
  toolDetailRows,
  toolResultTitle,
} from "../../src/ui/conversation/format";

const BASE_TOOL: ToolCall = {
  name: "Bash",
  call_id: "Absent",
  input: "Absent",
  command: "Absent",
  path: "Absent",
  url: "Absent",
  cwd: "Absent",
};

const SESSION = "/Users/dev/repo";

describe("shouldShowToolCwd", () => {
  it("hides a cwd that matches the session's", () => {
    expect(shouldShowToolCwd({ cwd: SESSION, path: null, sessionCwd: SESSION })).toBe(false);
  });

  it("hides a cwd beside an absolute path", () => {
    expect(
      shouldShowToolCwd({
        cwd: SESSION,
        path: `${SESSION}/README.md`,
        sessionCwd: null,
      }),
    ).toBe(false);
  });

  it("shows a cwd that differs from the session when the path is relative or absent", () => {
    expect(
      shouldShowToolCwd({
        cwd: "/Users/dev/other-repo",
        path: null,
        sessionCwd: SESSION,
      }),
    ).toBe(true);
    expect(
      shouldShowToolCwd({
        cwd: SESSION,
        path: "README.md",
        sessionCwd: null,
      }),
    ).toBe(true);
  });
});

describe("toolDetailRows", () => {
  it("includes a working directory row only when the source recorded one", () => {
    const recorded = toolDetailRows({ ...BASE_TOOL, cwd: { Recorded: "/Users/dev/other" } });

    expect(recorded).toContainEqual({ label: "Working directory", value: "/Users/dev/other" });
    expect(toolDetailRows(BASE_TOOL).some((row) => row.label === "Working directory")).toBe(false);
  });

  it("omits a working directory that matches the session's", () => {
    const rows = toolDetailRows(
      {
        ...BASE_TOOL,
        command: { Recorded: "ls -la" },
        cwd: { Recorded: SESSION },
      },
      { Recorded: SESSION },
    );

    expect(rows.map((row) => row.label)).toEqual(["Command"]);
  });

  it("omits a working directory beside an absolute path", () => {
    const rows = toolDetailRows(
      {
        ...BASE_TOOL,
        name: "Read",
        path: { Recorded: `${SESSION}/README.md` },
        cwd: { Recorded: SESSION },
      },
      { Recorded: SESSION },
    );

    expect(rows.map((row) => row.label)).toEqual(["Path"]);
  });

  it("keeps the recorded fields in a stable order and drops the rest", () => {
    const rows = toolDetailRows({
      ...BASE_TOOL,
      command: { Recorded: "ls -la" },
      cwd: { Recorded: "/Users/dev/other-repo" },
      input: { Recorded: '{"command":"ls -la"}' },
    });

    expect(rows.map((row) => row.label)).toEqual(["Command", "Working directory"]);
  });

  it("leaves the raw payload to the parameters block rather than making it a row", () => {
    const rows = toolDetailRows({
      ...BASE_TOOL,
      input: { Recorded: 'const r = await tools.exec_command({cmd:"ls"});' },
    });

    // A payload is recorded text of any length, so it renders as a clamped
    // block beneath these rows instead of as a one-line value inside them.
    expect(rows).toEqual([]);
  });

  it("treats a malformed value as unrecorded rather than rendering it", () => {
    expect(toolDetailRows({ ...BASE_TOOL, cwd: "Malformed" })).toEqual([]);
  });
});

describe("subagentDetail", () => {
  const UNRECORDED_SPAWN: Subagent = {
    native_id: "Absent",
    agent_type: "Absent",
    prompt: "Absent",
    outcome: "Absent",
    nickname: "Absent",
    duration_ms: "Absent",
    token_usage: "Absent",
    child_session_id: "Absent",
  };

  function spawnEvent(overrides: Partial<AillyEvent>): AillyEvent {
    return {
      id: "evt-1",
      session_id: "one",
      kind: EventKind.SubagentSpawn,
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
      ...overrides,
    };
  }

  it("reads the prompt the delegation recorded", () => {
    const event = spawnEvent({
      subagent: { Recorded: { ...UNRECORDED_SPAWN, prompt: { Recorded: "Map the parser" } } },
    });

    expect(subagentDetail(event)).toBe("Map the parser");
  });

  it("says so when the delegation recorded nothing to show", () => {
    expect(subagentDetail(spawnEvent({ subagent: { Recorded: UNRECORDED_SPAWN } }))).toBe(
      "No delegation detail recorded",
    );
  });
});

describe("toolResultTitle", () => {
  it("names the call a result answers when the harness recorded one", () => {
    expect(
      toolResultTitle({ call_id: { Recorded: "toolu_1" }, output: "Absent", is_error: "Absent" }),
    ).toBe("Tool result — toolu_1");
  });

  it("falls back to a bare title when no call was named", () => {
    expect(toolResultTitle({ call_id: "Absent", output: "Absent", is_error: "Absent" })).toBe(
      "Tool result",
    );
    expect(toolResultTitle(null)).toBe("Tool result");
  });
});
