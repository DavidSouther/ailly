#!/usr/bin/env node

import { mcpWrapper } from "./mcp";

import { expect, test } from "vitest";

test("aws-mcp-test", async () => {
  console.log("Initializing MCP wrapper...");

  // Initialize the wrapper with the configuration
  await mcpWrapper.initialize({
    servers: {
      mock: {
        type: "stdio",
        command: "npm",
        args: ["run", "mock-mcp-server"],
        env: {
          FASTMCP_LOG_LEVEL: "ERROR",
        },
      },
    },
  });

  const tools = await mcpWrapper.getToolsMap();
  expect(tools.size).toBe(3);
  expect(tools.has("read_documentation")).toBe(true);
  expect(tools.has("search_documentation")).toBe(true);
  expect(tools.has("recommend")).toBe(true);

  await mcpWrapper.cleanup();
}, 100000);
