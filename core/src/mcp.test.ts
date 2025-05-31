import { execSync } from "node:child_process";
import { join, normalize } from "node:path";
import type { ContentBlock } from "@aws-sdk/client-bedrock-runtime";
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
  let content: unknown;
  assert(isOk(result));
  content = unwrap(result).content;
  expect((content as ContentBlock[])[0].text).toBe("3");

  const resultTwo = await mcpWrapper.invokeTool("add", { args: [3, 2] });
  content = unwrap(resultTwo).content;
  expect((content as ContentBlock[])[0].text).toBe("5");

  await mcpWrapper.cleanup();
});