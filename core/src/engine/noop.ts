import { assert } from "@davidsouther/jiffies/lib/cjs/assert.js";
import { getLogger } from "@davidsouther/jiffies/lib/cjs/log.js";
import { isOk, unwrap } from "@davidsouther/jiffies/lib/cjs/result.js";

import type { Content } from "../content/content.js";
import { type PipelineSettings, LOGGER as ROOT_LOGGER } from "../index.js";
import type { EngineDebug, EngineGenerate } from "./index.js";
import { addContentMessages } from "./messages.js";

const LOGGER = getLogger("@ailly/core:noop");

export const DEFAULT_MODEL = "NOOP";
export const TIMEOUT = {
  timeout: 0,
  setTimeout(timeout: number) {
    TIMEOUT.timeout = timeout;
  },
  resetTimeout(defaultTimeout = 750) {
    const timeout = Number(process.env.AILLY_NOOP_TIMEOUT ?? defaultTimeout);
    assert(timeout >= 0);
    TIMEOUT.setTimeout(timeout);
  },
};
TIMEOUT.resetTimeout();
export const name = "noop";

export async function format(
  contents: Content[],
  context: Record<string, Content>,
): Promise<void> {
  for (const content of contents) {
    addContentMessages(content, context);
  }
}

const NOOP_STRIDE = 10;

export const generate: EngineGenerate = (
  content: Content,
  _: PipelineSettings,
) => {
  LOGGER.level = ROOT_LOGGER.level;
  LOGGER.format = ROOT_LOGGER.format;

  const [message, debug] = makeMessages(content);

  let error: Error | undefined;
  const stride = Number(process.env.NOOP_STRIDE ?? NOOP_STRIDE);
  assert(stride > 0);
  const stream = new TextEncoderStream();
  const writer = stream.writable.getWriter();
  const done = Promise.resolve()
    .then(async () => {
      await sleep(TIMEOUT.timeout);
      for (let i = 0; i <= message.length; i += stride) {
        await writer.ready;
        await writer.write(message.substring(i, i + stride));
        await sleep(TIMEOUT.timeout / 10);
      }
    })
    .catch((err) => {
      error = err as Error;
    })
    .finally(async () => {
      await writer.close();
    });

  return {
    stream: stream.readable,
    message: () => message,
    debug: () => (error ? { finish: "failed", error } : debug),
    done,
    ...debug,
  };
};

function makeMessages(content: Content): [string, EngineDebug] {
  const system = content.context.system
    ?.map((s, i) => `[system ${i}] ${s.content}`)
    .join("\n");

  const messageList = content.meta?.messages ?? [];

  const last = messageList.at(-1);
  let debug: EngineDebug = {};

  if (last?.toolUse) {
    if (isOk(last.toolUse.result)) {
      const result = unwrap(last.toolUse.result) as {
        content: Array<{ text: string }>;
      };
      messageList.push({
        role: "assistant",
        content: `TOOL RETURNED ${result.content[0].text}`,
      });
    } else {
      messageList.push({
        role: "assistant",
        content: `TOOL FAILED ${last.toolUse.result.err.message}`,
      });
    }
  }

  if (last?.content.includes("USE")) {
    const useTool = messageList
      .at(-1)
      ?.content.match(/USE (?<tool>[^\s]+) WITH (?<args>[^\n]+)/);
    if (useTool) {
      const { tool, args: rawArgs } = useTool.groups ?? {};
      const args = rawArgs.split(/\s+/);
      messageList.push({
        role: "assistant",
        content: `USING TOOL ${tool} WITH ARGS [${args.join(", ")}]\n`,
      });
      debug = {
        toolUse: {
          name: tool,
          input: { args },
          partial: rawArgs,
          id: "",
        },
      };
    }
  }

  const messages = messageList
    .map((m, i) => `[message ${i}] ${m.role}: ${m.content}`)
    .join("\n");

  return [
    process.env.AILLY_NOOP_RESPONSE ??
      [
        `noop response for ${content.name}:`,
        system,
        messages,
        `[response] ${content.prompt}`,
      ].join("\n"),
    debug,
  ];
}

function sleep(duration: number) {
  if (Number.isFinite(duration) && duration > 16)
    return new Promise<void>((resolve) => {
      setTimeout(() => resolve(), duration);
    });
}

export async function vector(s: string, _: unknown): Promise<number[]> {
  return [0.0, 1.0];
}
