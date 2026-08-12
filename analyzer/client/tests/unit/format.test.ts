import { describe, expect, it } from "vitest";

import type { ToolCall } from "../../src/tauri";
import {
  shouldShowToolCwd,
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

    // Input repeats Command once that is recorded, so it stays off the list.
    expect(rows.map((row) => row.label)).toEqual(["Command", "Working directory"]);
  });

  it("falls back to Input when no structured detail was recorded", () => {
    const rows = toolDetailRows({
      ...BASE_TOOL,
      input: { Recorded: 'const r = await tools.exec_command({cmd:"ls"});' },
    });

    expect(rows).toEqual([
      { label: "Input", value: 'const r = await tools.exec_command({cmd:"ls"});' },
    ]);
  });

  it("treats a malformed value as unrecorded rather than rendering it", () => {
    expect(toolDetailRows({ ...BASE_TOOL, cwd: "Malformed" })).toEqual([]);
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
