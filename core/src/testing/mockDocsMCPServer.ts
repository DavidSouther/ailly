import {
  McpServer,
} from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { z } from "zod";

// Create an MCP server
const server = new McpServer({
  name: "mock-aws-docs",
  version: "1.0.0",
});

server.tool(
  "add",
  {
    args: z
      .array(z.number().int())
      .describe("Array of numbers to add together"),
  },
  async ({ args }) => {
    const sum = args.reduce((total, num) => total + num, 0);

    return {
      content: [{ type: "text", text: String(sum) }],
    };
  },
);

async function startServer() {
  const transport = new StdioServerTransport();
  await server.connect(transport);
}

startServer();
