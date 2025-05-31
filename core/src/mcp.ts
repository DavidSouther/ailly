import { Err, Ok } from "@davidsouther/jiffies/lib/esm/result.js";
import { Client } from "@modelcontextprotocol/sdk/client/index.js";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio.js";
import { StreamableHTTPClientTransport } from "@modelcontextprotocol/sdk/client/streamableHttp.js";
import type { Transport } from "@modelcontextprotocol/sdk/shared/transport.js";

import type {
  JSONSchemaTypeName,
  Tool,
  ToolInformation,
  ToolInvocationResult,
} from "./engine/tool.js";

export type MCPServerConfig =
  | {
      type: "stdio";
      command: string;
      args?: string[];
      env?: Record<string, string>;
      cwd?: string;
    }
  | { type: "http"; url: string; headers?: Record<string, string> };

export type MCPConfig = Record<string, MCPServerConfig>;

/**
 * Configuration for MCP servers
 */
export interface MCPServersConfig {
  servers: MCPConfig;
  inputs?: Array<{
    type: string;
    id: string;
    description: string;
    password?: boolean;
  }>;
}

/**
 * Wrapper for MCP Client
 */
export class MCPClient {
  private toolsMap: Map<string, ToolInformation> = new Map();
  private clients: Map<string, Client> = new Map();
  private config?: MCPServersConfig;
  /**
   * Initialize the MCP wrapper with a configuration
   */
  async initialize(config: MCPServersConfig | undefined): Promise<void> {
    this.config = config;

    for (const [name, serverConfig] of Object.entries(
      this.config?.servers ?? {},
    )) {
      const clientInfo = { name: "ailly-client", version: "1.0.0" };

      // Create client and transport based on server type
      const client = new Client(clientInfo);
      let transport: Transport;

      if (serverConfig.type === "stdio" && serverConfig.command) {
        transport = new StdioClientTransport({
          command: serverConfig.command,
          args: serverConfig.args ?? [],
          env: serverConfig.env ?? {},
          cwd: serverConfig.cwd ?? undefined,
        });
      } else if (serverConfig.type === "http" && serverConfig.url) {
        transport = new StreamableHTTPClientTransport(
          new URL(serverConfig.url),
        );
      } else {
        throw new Error("Invalid server configuration, .");
      }
      await client.connect(transport);
      this.clients.set(name, client);
    }

    await this.fillToolInformation();
  }

  /**
   * Get the current configuration
   */
  getToolsMap(): Map<string, ToolInformation> {
    return this.toolsMap;
  }

  /**
   * Get the current configuration
   */
  getConfig(): MCPServersConfig | undefined {
    return this.config;
  }

  /**
   * Get or create an MCP client for a server
   */
  async fillToolInformation() {
    for (const client of this.clients.values()) {
      const tools = await this.getTools(client);

      for (const tool of tools) {
        this.toolsMap.set(`${tool.name}`, { client, tool });
      }
    }
  }

  /**
   * Invoke a tool on an MCP server
   */
  async invokeTool(
    toolName: string,
    parameters: Record<string, unknown>,
    context?: string,
  ): Promise<ToolInvocationResult> {
    const toolInfo = this.toolsMap.get(toolName);
    if (!toolInfo) {
      return Err({
        message: `Tool ${toolName} not found`,
        code: "TOOL_NOT_FOUND",
      });
    }

    const { client } = toolInfo;

    try {
      const result = await client.callTool({
        name: toolName,
        arguments: parameters,
        ...(context ? { context } : {}),
      });

      if (result.isError) {
        return Err({
          message: `Tool execution failed. Result: ${JSON.stringify(result)}`,
          code: "TOOL_EXECUTION_FAILED",
        });
      }

      return Ok(result);
    } catch (error) {
      return Err({
        message: error instanceof Error ? error.message : String(error),
        code: "TOOL_EXECUTION_FAILED",
      });
    }
  }

  getAllTools(): Tool[] {
    const tools: Tool[] = [];
    for (const [key, value] of this.toolsMap.entries()) {
      tools.push(value.tool);
    }
    return tools;
  }

  getTool(name: string | undefined): ToolInformation | undefined {
    if (!name) return undefined;
    return this.toolsMap.get(name);
  }

  /**
   * Get available tools from an MCP server
   */
  private async getTools(client: Client): Promise<Tool[]> {
    if (!client) {
      throw new Error("Client passed in is null");
    }

    const result = await client.listTools();
    return result.tools.map((tool) => {
      const properties: Record<
        string,
        {
          type: JSONSchemaTypeName;
          description?: string;
          enum?: string[];
        }
      > = {};

      // Convert properties to the expected format
      if (tool.inputSchema?.properties) {
        for (const [key, prop] of Object.entries(tool.inputSchema.properties)) {
          properties[key] = {
            type: (prop as { type?: JSONSchemaTypeName }).type || "string",
            description: (prop as { description?: string }).description,
            enum: (prop as { enum?: string[] }).enum,
          };
        }
      }

      return {
        name: tool.name,
        description: tool.description,
        parameters: {
          type: "object",
          properties,
          required: tool.inputSchema?.required || [],
        },
      };
    });
  }

  /**
   * Clean up resources
   */
  async cleanup(): Promise<void> {
    // The Client class doesn't have a disconnect method
    // We'll just clear our maps
    for (const client of this.clients.values()) {
      await client.close();
    }
    this.clients.clear();
    this.toolsMap.clear();
  }
}

// Export a singleton instance
export const mcpWrapper = new MCPClient();
