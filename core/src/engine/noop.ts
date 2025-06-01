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
  resetTimeout() {
    TIMEOUT.setTimeout(Number(process.env.AILLY_NOOP_TIMEOUT ?? 750));
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

export const generate: EngineGenerate = (
  content: Content,
  _: PipelineSettings,
) => {
  LOGGER.level = ROOT_LOGGER.level;
  LOGGER.format = ROOT_LOGGER.format;

  const [message, debug] = makeMessages(content);

  let error: Error | undefined;
  const stream = new TextEncoderStream();
  const done = Promise.resolve()
    .then(async () => {
      await sleep(TIMEOUT.timeout);
      const writer = await stream.writable.getWriter();
      try {
        await writer.ready;
        if (process.env.AILLY_NOOP_STREAM) {
          let first = true;
          for (const word of message.split(" ")) {
            await writer.write((first ? "" : " ") + word);
            first = false;
            await sleep(TIMEOUT.timeout / 10);
          }
        } else {
          writer.write(message);
        }
      } finally {
        writer.close();
      }
    })
    .catch((err) => {
      error = err as Error;
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
  const messages = messageList
    .map((m, i) => `[message ${i}] ${m.role}: ${m.content}`)
    .join("\n");

  const last = messageList.at(-1);
  if (last?.toolUse) {
    if (isOk(last.toolUse.result)) {
      const result = unwrap(last.toolUse.result) as {
        content: Array<{ text: string }>;
      };
      return [`TOOL RETURNED ${result.content[0].text}\n`, {}];
    }
    return [`TOOL FAILED ${last.toolUse.result.err.message}\n`, {}];
  }

  if (last?.content.includes("USE")) {
    const useTool = messageList
      .at(-1)
      ?.content.match(/USE (?<tool>[^\s]+) WITH (?<args>[^\n]+)/);
    if (useTool) {
      const { tool, args: rawArgs } = useTool.groups ?? {};
      const args = rawArgs.split(/\s+/);
      return [
        `USING TOOL ${tool} WITH ARGS [${args.join(", ")}]\n`,
        {
          toolUse: {
            name: tool,
            input: { args },
            partial: "",
            id: "",
          },
        },
      ];
    }
  }

  return [
    process.env.AILLY_NOOP_RESPONSE ??
      [
        `noop response for ${content.name}:`,
        system,
        messages,
        `[response] ${content.prompt}`,
      ].join("\n"),
    {},
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
