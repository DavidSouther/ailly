import type { Temporal } from "temporal-polyfill";
import type { Content, ContentMeta, View } from "../content/content.js";
import type { PipelineSettings } from "../index.js";
import * as bedrock from "./bedrock/bedrock.js";
import * as mistral from "./mistral/mistral.js";
import * as noop from "./noop.js";
import * as openai from "./openai.js";

export interface EngineDebug {
  engine?: string;
  model?: string;
  finish?: string;
  error?: Error;
  lastRun?: Temporal.Instant | string;
}

// TODO: Add tool call requests to return type
export type EngineGenerate<D extends EngineDebug = object> = (
  c: Content,
  parameters: PipelineSettings,
) => {
  stream: ReadableStream;
  message(): string;
  debug(): D;
  done: Promise<void>;
};

export interface Engine {
  DEFAULT_MODEL: string;
  name: string;
  format(c: Content[], context: Record<string, Content>): Promise<void>;
  generate: EngineGenerate;
  vector(s: string, parameters: ContentMeta): Promise<number[]>;
  view?(): Promise<View>;
  models?(): string[];
  formatError?(content: Content): string | undefined;
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
  noop: noop as unknown as Engine,
};

export async function getEngine(name: string): Promise<Engine> {
  if (name.startsWith("file://")) {
    return import(name);
  }
  if (!ENGINES[name]) throw new Error(`Unknown plugin ${name}`);
  return ENGINES[name];
}
