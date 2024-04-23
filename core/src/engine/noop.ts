import { getLogger } from "@davidsouther/jiffies/lib/esm/log.js";
import { Content } from "../content/content.js";
import { LOGGER as ROOT_LOGGER } from "../util.js";
import type { PipelineSettings } from "../ailly";
import type { Message } from "./index";

const LOGGER = getLogger("@ailly/core:noop");

const asMessages = (content: Content) => [
  { role: "user", content: content.prompt } satisfies Message,
  ...(content.response
    ? [{ role: "assistant", content: content.response } satisfies Message]
    : []),
];

export const DEFAULT_MODEL = "NOOP";
const NOOP_TIMEOUT = Number(process.env["AILLY_NOOP_TIMEOUT"] ?? 750);
export const name = "noop";
export async function format(
  contents: Content[],
  context: Record<string, Content>
): Promise<void> {
  for (const content of contents) {
    let messages: Message[] = [];
    if (content.context.folder) {
      messages = Object.values(context).map<Message[]>(asMessages).flat();
    } else if (content.context.predecessor) {
      let history = [context[content.context.predecessor]];
      while (history.at(-1)?.context.predecessor) {
        history.push(context[history.at(-1)?.context?.predecessor!]);
      }
      messages = history.reverse().map(asMessages).flat();
    }
    content.meta ??= {};
    content.meta.messages = messages;
  }
}
export async function generate<D extends {} = {}>(
  content: Content,
  _: PipelineSettings
): Promise<{ debug: D; message: string }> {
  LOGGER.level = ROOT_LOGGER.level;
  LOGGER.format = ROOT_LOGGER.format;
  await new Promise<void>((resolve) => {
    setTimeout(() => resolve(), NOOP_TIMEOUT);
  });
  const system = content.context.system?.map((s) => s.content).join("\n");
  const messages = content.meta?.messages
    ?.map((m) => `${m.role}: ${m.content}`)
    .join("\n");
  return {
    message:
      process.env["AILLY_NOOP_RESPONSE"] ??
      [
        `noop response for ${content.name}:`,
        system,
        messages,
        content.prompt,
      ].join("\n"),
    debug: { system: content.context.system } as unknown as D,
  };
}
export async function vector(s: string, _: unknown): Promise<number[]> {
  return [0.0, 1.0];
}
