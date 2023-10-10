import { Content } from "../content";
import * as openai from "./openai.js";

export interface Plugin {
  DEFAULT_MODEL: string;
  format: (c: Content[]) => Promise<void>;
  generate: (
    c: Content,
    parameters: Record<string, string>
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
};
