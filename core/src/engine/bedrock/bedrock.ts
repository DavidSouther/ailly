import {
  BedrockRuntimeClient,
  InvokeModelCommand,
  InvokeModelWithResponseStreamCommand,
} from "@aws-sdk/client-bedrock-runtime";
import { Content, View } from "../../content/content.js";
import { LOGGER as ROOT_LOGGER } from "../../util.js";
import { EngineGenerate, Summary } from "../index.js";
import { Models, PromptBuilder } from "./prompt-builder.js";
import { getLogger } from "@davidsouther/jiffies/lib/esm/log.js";
import { fromNodeProviderChain } from "@aws-sdk/credential-providers";
import { addContentMessages } from "../messages.js";

export const name = "bedrock";
export const DEFAULT_MODEL = "anthropic.claude-3-sonnet-20240229-v1:0";

const LOGGER = getLogger("@ailly/core:bedrock");

export interface BedrockDebug {
  finish?: string;
  error?: Error;
}

const MODEL_MAP: Record<string, string> = {
  sonnet: "anthropic.claude-3-sonnet-20240229-v1:0",
  haiku: "anthropic.claude-3-haiku-20240307-v1:0",
  opus: "anthropic.claude-3-opus-20240229-v1:0",
};

export const generate: EngineGenerate<BedrockDebug> = async (
  c: Content,
  { model = DEFAULT_MODEL }: { model?: string }
) => {
  LOGGER.level = ROOT_LOGGER.level;
  LOGGER.format = ROOT_LOGGER.format;
  const bedrock = new BedrockRuntimeClient({
    credentials: fromNodeProviderChain({
      ignoreCache: true,
      profile: process.env["AWS_PROFILE"],
    }),
    ...(process.env["AWS_REGION"] && { region: process.env["AWS_REGION"] }),
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
    let chunkNum = 0;
    const stream = new TransformStream();
    const response = await bedrock.send(
      new InvokeModelWithResponseStreamCommand({
        modelId: model,
        contentType: "application/json",
        accept: "application/json",
        body: JSON.stringify(prompt),
      })
    );

    Promise.resolve().then(async () => {
      LOGGER.info(`Begin streaming response from Bedrock for ${c.name}`);

      for await (const block of response.body ?? []) {
        LOGGER.debug(`Received chunk ${chunkNum++} from Bedrock for ${c.name}`);
        const writer = stream.writable.getWriter();
        await writer.ready;
        const chunk = new TextDecoder().decode(block.chunk?.bytes);
        message += chunk;
        await writer.write(chunk);
        writer.releaseLock();
      }

      await stream.writable.getWriter().close();
    });
    // In edit mode, claude (at least) does not return the stop sequence nor the prefill, so the edit is the message.

    return {
      stream: stream.readable,
      message: () => message,
      debug: () => ({
        finish: "unknown",
      }),
    };
  } catch (error) {
    LOGGER.warn(`Error from Bedrock for ${c.name}`, { error });
    return {
      stream: new TextDecoderStream("💩").readable,
      message: () => "💩",
      debug: () => ({
        finish: "failed",
        error: error as Error,
      }),
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
