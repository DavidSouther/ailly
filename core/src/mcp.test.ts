#!/usr/bin/env node

import { mcpWrapper } from "./mcp";

import type { Ok } from "@davidsouther/jiffies/src/result";
import { expect, test } from "vitest";

test("aws-mcp-test", async () => {
  console.log("Initializing MCP wrapper...");

  // Initialize the wrapper with the configuration
  await mcpWrapper.initialize({
    servers: {
      mock: {
        type: "stdio",
        command: "uvx",
        args: [
          "--from",
          "awslabs-aws-documentation-mcp-server",
          "awslabs.aws-documentation-mcp-server.exe",
        ],
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

  const toolResult = await mcpWrapper.invokeTool("recommend", {
    url: "https://docs.aws.amazon.com/AmazonS3/latest/userguide/bucketnamingrules.html",
  });

  expect(toolResult).toBeDefined();
  expect("ok" in toolResult).toBe(true);
  const result = (toolResult as Ok<unknown>).ok;
  expect(Array.isArray((result as { content: unknown }).content)).toBe(true);
  expect(
    (result as { content: Array<Record<string, string>> }).content.length,
  ).toBe(129);

  await mcpWrapper.cleanup();
}, 100000);
