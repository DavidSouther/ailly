import { Content, ContentMeta } from "../content.js";
import * as openai from "./openai.js";
import * as bedrock from "./bedrock/bedrock.js";
import * as mistral from "./mistral/mistral.js";
import { isDefined } from "../util.js";

export interface Plugin {
  DEFAULT_MODEL: string;
  format(c: Content[]): Promise<void>;
  generate(
    c: Content,
    parameters: ContentMeta
  ): Promise<{ debug: unknown; message: string }>;
  tune(c: Content[], parameters: ContentMeta): Promise<unknown>;
  vector(s: string, parameters: ContentMeta): Promise<number[]>;
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
  bedrock: bedrock as unknown as Plugin,
  mistral: mistral as unknown as Plugin,
};

export function getPlugin(name: string): Plugin {
  if (!PLUGINS[name]) throw new Error(`Unknown plugin ${name}`);
  return PLUGINS[name];
}

export function getMessages(content: Content): Message[] {
  const system = content.system.join("\n");
  const history: Content[] = [];
  while (content) {
    history.push(content);
    content = content.predecessor!;
  }
  history.reverse();
  const augment = history
    .map<Array<Message | undefined>>(
      (c) =>
        (c.meta?.augment || []).map<Message>(({ content }) => ({
          role: "user",
          content: "Background information: " + content,
        })) ?? []
    )
    .flat()
    .filter(isDefined)
    .slice(0, 1);
  const parts = history
    .map<Array<Message | undefined>>((content) => [
      {
        role: "user",
        content: content.prompt,
      },
      content.response
        ? { role: "assistant", content: content.response }
        : undefined,
    ])
    .flat()
    .filter(isDefined);
  return [{ role: "system", content: system }, ...augment, ...parts];
}
