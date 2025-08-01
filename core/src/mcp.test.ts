import { join, resolve } from "node:path";
import { assert } from "@davidsouther/jiffies/lib/cjs/assert.js";
import { isOk, unwrap } from "@davidsouther/jiffies/src/result.js";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { expect, test } from "vitest";
import { type MCPServersConfig, PluginTransport, mcpWrapper } from "./mcp.js";

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
  const { tools } = mcpWrapper;
  expect(tools.length).toBe(1);
  expect(tools[0].name).toBe("add");

  //test get tool
  //test invokeTool
  const result = await mcpWrapper.invokeTool("add", { args: [1, 2] });
  assert(isOk(result));
  const block = unwrap(result);
  expect(block).toEqual({ content: [{ type: "text", text: "3" }] });

  await mcpWrapper.cleanup();
});

test("PluginTransport", async () => {
  const clientInfo = { name: "ailly-client", version: "1.0.0" };

  // Create client and transport based on server type
  const client = new Client(clientInfo);
  const transport = new PluginTransport(
    resolve(__dirname, "../testing/mcpPlugin.mjs"),
  );
  await client.connect(transport);
  const { tools } = await client.listTools();
  expect(tools.length).toBe(1);
  expect(tools[0].name).toBe("add");
  const result = await client.callTool({
    name: "add",
    arguments: { args: [1, 2] },
  });
  expect(result.content).toEqual([{ type: "text", text: "3" }]);
});