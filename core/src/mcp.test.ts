#!/usr/bin/env node

import { mcpWrapper } from "./mcp"
import { expect, test } from "vitest";
import { type MCPServersConfig, type ToolInformation, mcpWrapper } from "./mcp";

test("mock-doc-test", async () => {
  console.log("Initializing MCP wrapper...");

  const mcpConfig: MCPServersConfig = {
    servers: {
      "mock-aws-docs": {
        type: "stdio",
        command: "node",
        args: ["./lib/testing/mockDocsMCPServer.js"],
        env: {
          FASTMCP_LOG_LEVEL: "ERROR",
        },
      },
    },
  };

  await mcpWrapper.initialize(mcpConfig);

  //test get tools
  const tools: Map<string, ToolInformation> = mcpWrapper.getToolsMap();
  expect(tools.size).toBe(1);
  expect(tools.has("add")).toBe(true);

  //test get tool
  const tool = mcpWrapper.getTool("add");
  expect(tool.tool.name).toBe("add");

  //test invokeTool
  const result = await mcpWrapper.invokeTool("add", { args: [1, 2] });
  expect(result.content[0].text).toBe("3");

  const resultTwo = await mcpWrapper.invokeTool("add", { args: [3, 2] });
  expect(resultTwo.content[0].text).toBe("5");

  await mcpWrapper.cleanup();
});
