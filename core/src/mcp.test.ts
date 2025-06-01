import { join } from "node:path";
import { assert, assertExists } from "@davidsouther/jiffies/lib/cjs/assert.js";
import { isOk, unwrap } from "@davidsouther/jiffies/src/result.js";
import { expect, test } from "vitest";
import type { ToolInformation } from "./engine/tool.js";
import { type MCPServersConfig, mcpWrapper } from "./mcp.js";

test("mock-doc-test", async () => {
  // Skip this test on windows, as it relies on `/usr/bin/env` in the testing server
  if (process.platform === "win32") return;
  const mcpConfig: MCPServersConfig = {
    servers: {
      "mock-aws-docs": {
        type: "stdio",
        command: process.argv0,
        args: [join(__dirname, "..", "testing", "mockMCPServer.mjs")],
      },
    },
  };

  await mcpWrapper.initialize(mcpConfig);

  //test get tools
  const tools: Map<string, ToolInformation> = mcpWrapper.getToolsMap();
  expect(tools.size).toBe(1);
  expect(tools.has("add")).toBe(true);

  //test get tool
  const tool = assertExists(mcpWrapper.getTool("add"));
  expect(tool.tool.name).toBe("add");

  //test invokeTool
  const result = await mcpWrapper.invokeTool("add", { args: [1, 2] });
  assert(isOk(result));
  const block = unwrap(result);
  expect(block).toEqual({ sum: 3 });

  await mcpWrapper.cleanup();
});
