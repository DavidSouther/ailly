import type { Result } from "@davidsouther/jiffies/src/result";
import type { Client } from "@modelcontextprotocol/sdk/client/index";

export type JSONSchemaTypeName =
  | "string"
  | "number"
  | "integer"
  | "boolean"
  | "object"
  | "array";

export interface ToolParameters {
  type: "object";
  properties: {
    [key: string]: {
      type: JSONSchemaTypeName;
      description?: string;
      enum?: string[];
    };
  };
  required?: string[];
  additionalProperties?: boolean;
}

export interface Tool {
  name: string;
  description?: string;
  parameters: ToolParameters;
}

export interface ToolInformation {
  client: Client;
  tool: Tool;
}

export type ToolInvocationResult = Result<
  Record<string, unknown>,
  { message: string; code?: string }
>;
