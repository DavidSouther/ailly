import {
  BedrockRuntimeClient,
  InvokeModelCommand,
  InvokeModelWithResponseStreamCommand,
} from "@aws-sdk/client-bedrock-runtime";
import { Content, View } from "../../content/content.js";
import { LOGGER as ROOT_LOGGER } from "../../ailly.js";
import { EngineGenerate, Summary } from "../index.js";
import { Models, PromptBuilder } from "./prompt-builder.js";
import { getLogger } from "@davidsouther/jiffies/lib/cjs/log.js";
import { fromNodeProviderChain } from "@aws-sdk/credential-providers";
import { addContentMessages } from "../messages.js";

export const name = "bedrock";
export const DEFAULT_MODEL = "anthropic.claude-3-sonnet-20240229-v1:0";

const LOGGER = getLogger("@ailly/core:bedrock");

export interface BedrockDebug {
  statistics?: {
    inputTokenCount?: number;
    outputTokenCount?: number;
    invocationLatency?: number;
    firstByteLatency?: number;
  };
  finish?: string;
  error?: Error;
  id: string;
}

const MODEL_MAP: Record<string, string> = {
  sonnet: "anthropic.claude-3-sonnet-20240229-v1:0",
  haiku: "anthropic.claude-3-haiku-20240307-v1:0",
  opus: "anthropic.claude-3-opus-20240229-v1:0",
};

export const generate: EngineGenerate<BedrockDebug> = (
  c: Content,
  { model = DEFAULT_MODEL }: { model?: string }
) => {
  LOGGER.level = ROOT_LOGGER.level;
  LOGGER.format = ROOT_LOGGER.format;
  LOGGER.console = ROOT_LOGGER.console;
  const bedrock = new BedrockRuntimeClient({
    credentials: fromNodeProviderChain({
      ignoreCache: true,
      profile: process.env["AWS_PROFILE"],
    }),
    ...(process.env["AWS_REGION"] ? { region: process.env["AWS_REGION"] } : {}),
  });
  model = MODEL_MAP[model] ?? model;
  let messages = c.meta?.messages ?? [];
  if (!messages.find((m) => m.role == "user")) {
    throw new Error("Bedrock must have at least one message with role: user");
  }
  const stopSequences = c.context.edit ? ["```"] : undefined;

  const promptBuilder = new PromptBuilder(model as Models);
  const prompt = promptBuilder.build(messages);

  if (stopSequences) {
    prompt.stop_sequences = stopSequences;
  }

  // Use given temperature; or 0 for edit (don't want creativity) but 1.0 generally.
  prompt.temperature =
    c.meta?.temperature ?? c.context.edit != undefined ? 0.0 : 1.0;

  LOGGER.debug("Bedrock sending prompt", {
    ...prompt,
    messages: [`...${prompt.messages.length} messages...`],
    system: `${prompt.system.slice(0, 20)}...`,
  });

  try {
    let message = "";
    const debug: BedrockDebug = { id: "", finish: "unknown" };
    const stream = new TransformStream();
    const writer = stream.writable.getWriter();
    const done = bedrock
      .send(
        new InvokeModelWithResponseStreamCommand({
          modelId: model,
          contentType: "application/json",
          accept: "application/json",
          body: JSON.stringify(prompt),
        })
      )
      .then(async (response) => {
        LOGGER.info(`Begin streaming response from Bedrock for ${c.name}`);

        for await (const block of response.body ?? []) {
          const chunk = JSON.parse(
            new TextDecoder().decode(block.chunk?.bytes)
          );
          LOGGER.debug(
            `Received chunk for (${
              chunk.message?.id ?? debug.id
            }) from Bedrock for ${c.name}`,
            { chunk }
          );
          switch (chunk.type) {
            case "message_start":
              debug.id = chunk.message.id;
              break;
            case "content_block_start":
              break;
            case "content_block_delta":
              const text = chunk.delta.text;
              await writer.ready;
              message += text;
              await writer.write(text);
              break;
            case "message_delta":
              debug.finish = chunk.delta.stop_reason;
              break;
            case "message_stop":
              debug.statistics = chunk["amazon-bedrock-invocationMetrics"];
              break;
          }
        }
      })
      .catch((e) => {
        debug.finish = "failed";
        debug.error = e as Error;
        LOGGER.error(`Error for bedrock response ${debug.id}`, {
          err: debug.error,
        });
      })
      .finally(async () => {
        LOGGER.debug(`Closing write stream for bedrock response ${debug.id}`);
        await writer.close();
      });

    return {
      stream: stream.readable,
      message: () => (debug.error ? "💩" : message),
      debug: () => debug,
      done,
    };
  } catch (error) {
    LOGGER.warn(`Error from Bedrock for ${c.name}`, { error });
    return {
      stream: new TextDecoderStream("💩").readable,
      message: () => "💩",
      debug: () => ({
        finish: "failed",
        error: error as Error,
        id: "_failed_",
      }),
      done: Promise.resolve(),
    };
  }
};

export async function view(): Promise<View> {
  return {
    claude: {
      long: "Prefer long, elegant, formal language.",
    },
  };
}

export async function format(
  contents: Content[],
  context: Record<string, Content>
): Promise<Summary> {
  const summary: Summary = { prompts: contents.length, tokens: 0 };
  for (const content of contents) {
    await addContentMessages(content, context);
  }
  return summary;
}

export async function tune(content: Content[]) {}

const td = new TextDecoder();
export async function vector(inputText: string, {}: {}): Promise<number[]> {
  const bedrock = new BedrockRuntimeClient({});
  const response = await bedrock.send(
    new InvokeModelCommand({
      body: JSON.stringify({ inputText }),
      modelId: "amazon.titan-embed-text-v1",
      contentType: "application/json",
      accept: "*/*",
    })
  );

  const body = JSON.parse(td.decode(response.body));

  return body.embedding;
}

export function extractFirstFence(message: string): string {
  const firstTicks = message.indexOf("```");
  if (firstTicks != 0) LOGGER.warn("First code fence is not at index 0");
  const endOfFirstLine = message.indexOf("\n", firstTicks);
  const nextTicks = message.indexOf("```", endOfFirstLine + 1);
  message = message.slice(endOfFirstLine + 1, nextTicks + 1);
  return message;
}
