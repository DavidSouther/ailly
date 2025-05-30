import { Client } from "@modelcontextprotocol/sdk/client/index";
import { StdioClientTransport } from "@modelcontextprotocol/sdk/client/stdio";

import type { JSONSchemaTypeName, Tool } from "./engine/tool";

export type MCPConfig =
  | {
      type: "stdio";
      command: string;
      args?: string[];
      env?: Record<string, string>;
    }
  | { type: "http"; url: string; headers?: Record<string, string> };

/**
 * Configuration for MCP servers
 */
export interface MCPServersConfig {
  servers: MCPConfig[];
  inputs?: Array<{
    type: string;
    id: string;
    description: string;
    password?: boolean;
  }>;
}

export interface ToolInformation {
  client: Client;
  tool: Tool;
}

/**
 * Wrapper for MCP Client
 */
export class MCPClient {
  private toolsMap: Map<string, ToolInformation> = new Map();
  private config?: MCPServersConfig;
  /**
   * Initialize the MCP wrapper with a configuration
   */
  async initialize(config: MCPServersConfig | undefined): Promise<void> {
    this.config = config;
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
    if (this.config?.servers.length === 0) {
      return;
    }

    for (const serverConfig of this.config?.servers || []) {
      const clientInfo = { name: "ailly-client", version: "1.0.0" };

      // Create client and transport based on server type
      let client: Client;
      let transport: StdioClientTransport;

      if (serverConfig.type === "stdio" && serverConfig.command) {
        client = new Client(clientInfo);
        transport = new StdioClientTransport({
          command: serverConfig.command,
          args: serverConfig.args || [],
          env: serverConfig.env || {},
        });
      } else {
        throw new Error("Invalid server configuration.");
      }

      await client.connect(transport);

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
  ) {
    const toolInfo = this.toolsMap.get(toolName);
    if (!toolInfo) {
      throw new Error(`Tool ${toolName} not found`);
    }

    const { client } = toolInfo;

    return await client.callTool({
      name: toolName,
      arguments: parameters,
      ...(context ? { context } : {}),
    });
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
  }
}

// Export a singleton instance
export const mcpWrapper = new MCPClient();
