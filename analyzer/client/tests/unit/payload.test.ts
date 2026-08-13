import { describe, expect, it } from "vitest";

import type { ToolCall } from "../../src/tauri";
import { toolPayloadText } from "../../src/ui/payload";

const BASE_TOOL: ToolCall = {
  name: "Edit",
  call_id: "Absent",
  input: "Absent",
  command: "Absent",
  path: "Absent",
  url: "Absent",
  cwd: "Absent",
};

describe("toolPayloadText", () => {
  it("renders an object payload one field per line, with source on its own lines", () => {
    const payload = toolPayloadText({
      ...BASE_TOOL,
      path: { Recorded: "src/session.ts" },
      input: {
        Recorded: JSON.stringify({
          file_path: "src/session.ts",
          old_string: "return 3600;",
          new_string: "return TTL;\n// seconds",
        }),
      },
    });

    // file_path is the Path row already beside this block, so it is not repeated.
    expect(payload).toEqual({
      Recorded: "old_string: return 3600;\nnew_string:\nreturn TTL;\n// seconds",
    });
  });

  it("unwraps a payload the harness recorded as a JSON string", () => {
    const payload = toolPayloadText({
      ...BASE_TOOL,
      name: "apply_patch",
      input: { Recorded: JSON.stringify("*** Begin Patch\n-old\n+new") },
    });

    expect(payload).toEqual({ Recorded: "*** Begin Patch\n-old\n+new" });
  });

  it("keeps a payload that is not JSON at all", () => {
    const snippet = 'const r = await tools.exec_command({cmd:"ls"});';

    expect(toolPayloadText({ ...BASE_TOOL, input: { Recorded: snippet } })).toEqual({
      Recorded: snippet,
    });
  });

  it("shows nothing when the payload only repeats fields already rendered", () => {
    expect(
      toolPayloadText({
        ...BASE_TOOL,
        name: "Bash",
        command: { Recorded: "cargo test" },
        cwd: { Recorded: "/repo" },
        input: { Recorded: JSON.stringify({ command: "cargo test", workdir: "/repo" }) },
      }),
    ).toBeNull();
  });

  it("keeps the part of a read's payload the path row does not carry", () => {
    expect(
      toolPayloadText({
        ...BASE_TOOL,
        name: "Read",
        path: { Recorded: "runbook.md" },
        input: { Recorded: JSON.stringify({ file_path: "runbook.md", offset: 100, limit: 50 }) },
      }),
    ).toEqual({ Recorded: "offset: 100\nlimit: 50" });
  });

  it("reports an unreadable payload rather than dropping it, and an absent one as nothing", () => {
    expect(toolPayloadText({ ...BASE_TOOL, input: "Malformed" })).toBe("Malformed");
    expect(toolPayloadText({ ...BASE_TOOL, input: "Absent" })).toBeNull();
  });
});
