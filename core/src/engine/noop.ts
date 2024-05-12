import { getLogger } from "@davidsouther/jiffies/lib/esm/log.js";
import { Content } from "../content/content.js";
import { LOGGER as ROOT_LOGGER } from "../util.js";
import type { PipelineSettings } from "../ailly.js";
import { addContentMessages } from "./messages.js";
import { EngineGenerate } from ".";

const LOGGER = getLogger("@ailly/core:noop");

export const DEFAULT_MODEL = "NOOP";
export const TIMEOUT = {
  timeout: 0,
  setTimeout(timeout: number) {
    TIMEOUT.timeout = timeout;
  },
  resetTimeout() {
    TIMEOUT.setTimeout(Number(process.env["AILLY_NOOP_TIMEOUT"] ?? 750));
  },
};
TIMEOUT.resetTimeout();
export const name = "noop";

export async function format(
  contents: Content[],
  context: Record<string, Content>
): Promise<void> {
  for (const content of contents) {
    addContentMessages(content, context);
  }
}

export interface MistralDebug {}

export const generate: EngineGenerate<MistralDebug> = async (
  content: Content,
  _: PipelineSettings
) => {
  LOGGER.level = ROOT_LOGGER.level;
  LOGGER.format = ROOT_LOGGER.format;
  await new Promise<void>((resolve) => {
    setTimeout(() => resolve(), TIMEOUT.timeout);
  });

  const system = content.context.system?.map((s) => s.content).join("\n");
  const messages = content.meta?.messages
    ?.map((m) => `${m.role}: ${m.content}`)
    .join("\n");
  const message =
    process.env["AILLY_NOOP_RESPONSE"] ??
    [
      `noop response for ${content.name}:`,
      system,
      messages,
      content.prompt,
    ].join("\n");

  const stream = new TextEncoderStream();
  Promise.resolve().then(async () => {
    const writer = await stream.writable.getWriter();
    await writer.ready;
    await writer.write(message);
    writer.close();
  });
  return {
    stream: stream.readable,
    message: () => message,
    debug: () => ({}),
  };
};
export async function vector(s: string, _: unknown): Promise<number[]> {
  return [0.0, 1.0];
}
