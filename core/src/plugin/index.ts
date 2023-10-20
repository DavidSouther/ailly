import { Content, ContentMeta } from "../content.js";
import * as openai from "./openai.js";
import * as bedrock from "./bedrock/bedrock.js";
import * as mistral from "./mistral/mistral.js";

export interface Plugin {
  DEFAULT_MODEL: string;
  format: (c: Content[]) => Promise<void>;
  generate: (
    c: Content,
    parameters: ContentMeta
  ) => Promise<{ debug: unknown; message: string }>;
}

export interface Message {
  role: "system" | "user" | "assistant";
  content: string;
  tokens?: number;
}

export interface Summary {
  tokens: number;
  prompts: number;
}

export const PLUGINS: Record<string, Plugin> = {
  openai: openai as unknown as Plugin,
  bedrock: bedrock as unknown as Plugin
};
