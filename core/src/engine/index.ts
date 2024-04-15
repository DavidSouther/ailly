import { Content, ContentMeta, View } from "../content/content.js";
import * as openai from "./openai.js";
import * as bedrock from "./bedrock/bedrock.js";
import * as mistral from "./mistral/mistral.js";
import { PipelineSettings } from "../ailly.js";

export interface Engine {
  DEFAULT_MODEL: string;
  name: string;
  format(c: Content[], context: Record<string, Content>): Promise<void>;
  generate<D extends {} = {}>(
    c: Content,
    parameters: PipelineSettings
  ): Promise<{ debug: D; message: string }>;
  vector(s: string, parameters: ContentMeta): Promise<number[]>;
  view?(): Promise<View>;
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

export const ENGINES: Record<string, Engine> = {
  openai: openai as unknown as Engine,
  bedrock: bedrock as unknown as Engine,
  mistral: mistral as unknown as Engine,
};

export async function getEngine(name: string): Promise<Engine> {
  if (name.startsWith("file://")) {
    return import(name);
  }
  if (!ENGINES[name]) throw new Error(`Unknown plugin ${name}`);
  return ENGINES[name];
}
