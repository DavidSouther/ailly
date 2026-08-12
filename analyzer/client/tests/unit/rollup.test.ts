import { describe, expect, it } from "vitest";

import { type AillyEvent, EventKind, type ToolCall, type ToolResult } from "../../src/tauri";
import { categoryForTool, isWebCall, summarizeSession } from "../../src/ui/summary/rollup";

const SESSION_ID = "claude_code:/home/a.jsonl:one";
const SUSPECT_FILE = "packages/auth/src/session.ts";

function baseEvent(id: string, ordinal: number, kind: EventKind): AillyEvent {
  return {
    id,
    session_id: SESSION_ID,
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

function toolEvent(
  id: string,
  ordinal: number,
  tool: Partial<ToolCall> & { name: string },
): AillyEvent {
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

function resultEvent(id: string, ordinal: number, result: Partial<ToolResult>): AillyEvent {
  return {
    ...baseEvent(id, ordinal, EventKind.ToolResult),
    tool_result: {
      Recorded: {
        call_id: "Absent",
        output: "Absent",
        is_error: "Absent",
        ...result,
      } satisfies ToolResult,
    },
  };
}

/** Six tool calls, mirroring the feature test's session. */
const EVENTS: AillyEvent[] = [
  baseEvent("evt-1", 1, EventKind.UserTurn),
  toolEvent("evt-2", 2, { name: "Read", path: { Recorded: SUSPECT_FILE } }),
  toolEvent("evt-3", 3, { name: "Bash", command: { Recorded: "rg -l LegacySession" } }),
  toolEvent("evt-4", 4, { name: "WebFetch" }),
  toolEvent("evt-5", 5, { name: "Edit", path: { Recorded: SUSPECT_FILE } }),
  toolEvent("evt-6", 6, { name: "Read", path: { Recorded: "docs/auth/runbook.md" } }),
  toolEvent("evt-7", 7, { name: "mcp__acme__lookup" }),
];

describe("categoryForTool", () => {
  it("maps known Claude Code and Codex names, and nothing else", () => {
    expect(categoryForTool("Read")).toBe("read");
    expect(categoryForTool("apply_patch")).toBe("edit");
    expect(categoryForTool("shell")).toBe("exec");
    expect(categoryForTool("mcp__acme__lookup")).toBe("unclassified");
  });

  it("maps Codex's and Pi's real tool names, additively", () => {
    expect(categoryForTool("exec_command")).toBe("exec");
    expect(categoryForTool("write_stdin")).toBe("exec");
    expect(categoryForTool("update_plan")).toBe("other");
    expect(categoryForTool("read_file")).toBe("read");
    // Pi spells its tools lowercase; Claude's capitalized names are unchanged.
    expect(categoryForTool("bash")).toBe("exec");
    expect(categoryForTool("Bash")).toBe("exec");
    expect(categoryForTool("read")).toBe("read");
    expect(categoryForTool("edit")).toBe("edit");
    expect(categoryForTool("write")).toBe("edit");
  });
});

describe("name heuristics", () => {
  it("recognizes web-shaped tool names", () => {
    expect(isWebCall("WebFetch")).toBe(true);
    expect(isWebCall("Read")).toBe(false);
  });
});

describe("summarizeSession", () => {
  it("splits tool calls by category, keeping unknown tools visible", () => {
    const stats = summarizeSession(EVENTS);

    expect(stats.toolCallCount).toBe(6);
    expect(stats.filesTouchedCount).toBe(2);
    expect(stats.duration).toBe("Absent");
    expect(stats.subagentSpawnCount).toBe("Absent");
    expect(stats.categories).toContainEqual({ category: "read", count: 2, share: 33 });
    expect(stats.unclassified).toEqual({
      count: 1,
      share: 17,
      toolNames: ["mcp__acme__lookup"],
    });
  });

  it("reports an empty rollup for a session with no tool calls", () => {
    const stats = summarizeSession([baseEvent("evt-1", 1, EventKind.UserTurn)]);

    expect(stats.toolCallCount).toBe(0);
    expect(stats.categories).toEqual([]);
    expect(stats.unclassified.count).toBe(0);
    expect(stats.sources).toEqual([]);
  });

  it("records subagent spawns and a duration when the source has them", () => {
    const stats = summarizeSession([
      {
        ...baseEvent("evt-1", 1, EventKind.UserTurn),
        timestamp: { Recorded: "2026-08-12T15:00:00Z" },
      },
      baseEvent("evt-2", 2, EventKind.SubagentSpawn),
      {
        ...baseEvent("evt-3", 3, EventKind.AssistantTurn),
        timestamp: { Recorded: "2026-08-12T15:18:42Z" },
      },
    ]);

    expect(stats.subagentSpawnCount).toEqual({ Recorded: 1 });
    expect(stats.duration).toEqual({ Recorded: "18m 42s" });
  });

  it("ranks tools by count and keeps raw harness names", () => {
    const stats = summarizeSession(EVENTS);

    expect(stats.toolsByFrequency[0]).toEqual({
      name: "Read",
      count: 2,
      share: 33,
      category: "read",
    });
    expect(stats.toolsByFrequency.map((tool) => tool.name)).toContain("mcp__acme__lookup");
  });

  it("orders tools tied on count by first appearance, not alphabetically", () => {
    const stats = summarizeSession([
      toolEvent("evt-1", 1, { name: "Write" }),
      toolEvent("evt-2", 2, { name: "Bash" }),
    ]);

    expect(stats.toolsByFrequency.map((tool) => tool.name)).toEqual(["Write", "Bash"]);
  });

  it("excludes tool calls the source did not record", () => {
    const stats = summarizeSession([
      toolEvent("evt-1", 1, { name: "Read", path: { Recorded: SUSPECT_FILE } }),
      baseEvent("evt-2", 2, EventKind.ToolCall),
    ]);

    expect(stats.toolCallCount).toBe(1);
    expect(stats.toolsByFrequency).toHaveLength(1);
  });

  it("groups sources by kind", () => {
    const stats = summarizeSession(EVENTS);
    const shell = stats.sources.find((group) => group.kind === "shell");
    const web = stats.sources.find((group) => group.kind === "web");
    const file = stats.sources.find((group) => group.kind === "file");

    expect(shell?.calls[0]?.detail).toBe("rg -l LegacySession");
    expect(web?.calls[0]?.detailRecorded).toBe(false);
    expect(web?.calls[0]?.detail).toMatch(/not recorded/i);
    expect(file?.label).toBe("File access");
    expect(file?.files.map((entry) => entry.path)).toEqual([SUSPECT_FILE, "docs/auth/runbook.md"]);
    expect(file?.count).toBe(2);
  });

  it("carries a call's working directory only when the source recorded one", () => {
    const stats = summarizeSession([
      toolEvent("evt-1", 1, {
        name: "Bash",
        command: { Recorded: "ls -la" },
        cwd: { Recorded: "/Users/dev/other-repo" },
      }),
      toolEvent("evt-2", 2, { name: "Bash", command: { Recorded: "pwd" } }),
    ]);
    const shell = stats.sources.find((group) => group.kind === "shell");

    expect(shell?.calls[0]?.cwd).toBe("/Users/dev/other-repo");
    expect(shell?.calls[1]?.cwd).toBeNull();
  });

  it("pairs a call with the result that names it, and only that result", () => {
    const stats = summarizeSession([
      toolEvent("evt-1", 1, {
        name: "Bash",
        command: { Recorded: "cargo test" },
        call_id: { Recorded: "call-1" },
      }),
      toolEvent("evt-2", 2, {
        name: "Bash",
        command: { Recorded: "pwd" },
        call_id: { Recorded: "call-2" },
      }),
      resultEvent("evt-3", 3, {
        call_id: { Recorded: "call-1" },
        output: { Recorded: "1 passed" },
      }),
      // Answers nothing: no call id, so it pairs with nothing rather than with
      // the call it happens to sit beside.
      resultEvent("evt-4", 4, { output: { Recorded: "orphaned" } }),
    ]);
    const shell = stats.sources.find((group) => group.kind === "shell");

    expect(shell?.calls[0]?.output).toEqual({ Recorded: "1 passed" });
    expect(shell?.calls[1]?.output).toBeNull();
  });

  it("reads chunked results for one call as a single output in source order", () => {
    const stats = summarizeSession([
      toolEvent("evt-1", 1, {
        name: "Bash",
        command: { Recorded: "cargo test" },
        call_id: { Recorded: "call-1" },
      }),
      resultEvent("evt-2", 2, { call_id: { Recorded: "call-1" }, output: { Recorded: "first" } }),
      resultEvent("evt-3", 3, { call_id: { Recorded: "call-1" }, output: { Recorded: "second" } }),
    ]);
    const shell = stats.sources.find((group) => group.kind === "shell");

    expect(shell?.calls[0]?.output).toEqual({ Recorded: "first\nsecond" });
  });

  it("keeps a recorded empty output distinct from no output at all", () => {
    const stats = summarizeSession([
      toolEvent("evt-1", 1, {
        name: "Bash",
        command: { Recorded: "true" },
        call_id: { Recorded: "call-1" },
      }),
      resultEvent("evt-2", 2, { call_id: { Recorded: "call-1" }, output: { Recorded: "" } }),
    ]);
    const shell = stats.sources.find((group) => group.kind === "shell");

    expect(shell?.calls[0]?.output).toEqual({ Recorded: "" });
  });

  it("keeps an unshowable output distinct from an unanswered call", () => {
    const stats = summarizeSession([
      toolEvent("evt-1", 1, {
        name: "Bash",
        command: { Recorded: "screencapture out.png" },
        call_id: { Recorded: "call-1" },
      }),
      // Claude returns image results as content blocks with no text in them.
      resultEvent("evt-2", 2, { call_id: { Recorded: "call-1" }, output: "Unsupported" }),
    ]);
    const shell = stats.sources.find((group) => group.kind === "shell");

    expect(shell?.calls[0]?.output).toBe("Unsupported");
  });

  it("marks an output the harness recorded as an error", () => {
    const stats = summarizeSession([
      toolEvent("evt-1", 1, {
        name: "Bash",
        command: { Recorded: "cargo test" },
        call_id: { Recorded: "call-1" },
      }),
      resultEvent("evt-2", 2, {
        call_id: { Recorded: "call-1" },
        output: { Recorded: "1 failed" },
        is_error: { Recorded: true },
      }),
    ]);
    const shell = stats.sources.find((group) => group.kind === "shell");

    expect(shell?.calls[0]?.outputIsError).toBe(true);
  });

  it("omits a source kind the session never used", () => {
    const stats = summarizeSession([toolEvent("evt-1", 1, { name: "Bash" })]);

    expect(stats.sources.map((group) => group.kind)).toEqual(["shell"]);
  });

  it("labels an unrecorded shell command instead of showing it as empty", () => {
    const stats = summarizeSession([
      toolEvent("evt-1", 1, { name: "Bash" }),
      toolEvent("evt-2", 2, { name: "Bash", command: { Recorded: "" } }),
    ]);
    const shell = stats.sources.find((group) => group.kind === "shell");

    expect(shell?.calls[0]).toMatchObject({
      detail: "Command not recorded",
      detailRecorded: false,
    });
    expect(shell?.calls[1]).toMatchObject({ detail: "", detailRecorded: true });
  });

  it("ranks files by touch count and lists each file's distinct tools", () => {
    const stats = summarizeSession(EVENTS);

    expect(stats.filesTouched[0]).toEqual({
      path: SUSPECT_FILE,
      touches: 2,
      tools: ["Read", "Edit"],
    });
  });

  it("counts a call with no recorded path toward totals but not toward files", () => {
    const stats = summarizeSession([toolEvent("evt-1", 1, { name: "Read" })]);

    expect(stats.toolCallCount).toBe(1);
    expect(stats.filesTouched).toEqual([]);
  });

  it("lists a repeated tool once per file", () => {
    const stats = summarizeSession([
      toolEvent("evt-1", 1, { name: "Read", path: { Recorded: SUSPECT_FILE } }),
      toolEvent("evt-2", 2, { name: "Read", path: { Recorded: SUSPECT_FILE } }),
    ]);

    expect(stats.filesTouched[0]?.tools).toEqual(["Read"]);
  });
});
