import {
  BedrockRuntimeClient,
  InvokeModelCommand,
  InvokeModelWithResponseStreamCommand,
} from "@aws-sdk/client-bedrock-runtime";
import { fromNodeProviderChain } from "@aws-sdk/credential-providers";
import { getLogger } from "@davidsouther/jiffies/lib/cjs/log.js";

import { Content, View } from "../../content/content.js";
import { LOGGER as ROOT_LOGGER } from "../../index.js";
import type { EngineDebug, EngineGenerate, Summary } from "../index.js";
import { addContentMessages } from "../messages.js";
import { PromptBuilder, type Models } from "./prompt_builder.js";

export const name = "bedrock";
export const DEFAULT_MODEL = "haiku";

const LOGGER = getLogger("@ailly/core:bedrock");

export interface BedrockDebug extends EngineDebug {
  statistics?: {
    inputTokenCount?: number;
    outputTokenCount?: number;
    invocationLatency?: number;
    firstByteLatency?: number;
  };
  finish?: string;
  error?: Error;
  region?: string;
  id: string;
}

const MODEL_MAP: Record<string, string> = {
  sonnet: "anthropic.claude-3-sonnet-20240229-v1:0",
  haiku: "anthropic.claude-3-haiku-20240307-v1:0",
  opus: "anthropic.claude-3-opus-20240229-v1:0",
};

declare module "@davidsouther/jiffies/lib/cjs/log.js" {
  interface Logger {
    /** Augment our logger with a `trace` method to trace streaming calls. */
    trace: Log;
  }
}

export const generate: EngineGenerate<BedrockDebug> = (
  c: Content,
  { model = DEFAULT_MODEL }: { model?: string }
) => {
  LOGGER.level = ROOT_LOGGER.level;
  LOGGER.format = ROOT_LOGGER.format;
  LOGGER.console = ROOT_LOGGER.console;
  LOGGER.trace = LOGGER.logAt(0.5, "TRACE");
  const bedrock = makeClient();
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
    messages: [`...${messages.length} messages...`],
    system: `${prompt.system.slice(0, 20)}...`,
  });

  try {
    let message = c.meta?.continue ? c.response ?? "" : "";
    const debug: BedrockDebug = {
      id: "",
      finish: "unknown",
      model,
      engine: "bedrock",
    };
    bedrock.config.region().then((region) => (debug.region = region));
    const stream = new TransformStream();
    const writer = stream.writable.getWriter();
    const invokeModelCommand = {
      modelId: model,
      contentType: "application/json",
      accept: "application/json",
      body: JSON.stringify(prompt),
    };
    LOGGER.trace("Sending InvokeModelWithResponseStreamCommand", {
      invokeModelCommand,
    });
    const done = bedrock
      .send(new InvokeModelWithResponseStreamCommand(invokeModelCommand))
      .then(async (response) => {
        LOGGER.debug(`Begin streaming response from Bedrock for ${c.name}`);

        let chunks = 0;
        // This is only for Claude3 https://docs.anthropic.com/en/api/messages-streaming
        for await (const block of response.body ?? []) {
          const chunk = JSON.parse(
            new TextDecoder().decode(block.chunk?.bytes)
          );
          LOGGER.trace(
            `Received chunk from Bedrock for ${c.name} (${
              chunk.message?.id ?? debug.id
            })`,
            { chunk }
          );
          chunks += 1;
          switch (chunk.type) {
            case "message_start":
              debug.id = chunk.message.id;
              debug.model = chunk.message.model;
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

        LOGGER.debug(
          `Finished streaming response from Bedrock for ${c.name} (${debug.id}), total ${chunks} chunks.`
        );
      })
      .catch((e) => {
        debug.finish = "failed";
        debug.error = e as Error;
        LOGGER.warn(
          `Error for Bedrock response ${c.name} (${debug.id ?? "no id"})`,
          {
            err: debug.error,
          }
        );
      })
      .finally(async () => {
        LOGGER.debug(
          `Closing write stream for bedrock response ${c.name} (${debug.id})`
        );
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

function makeClient() {
  return new BedrockRuntimeClient({
    credentials: fromNodeProviderChain({
      ignoreCache: true,
      profile: process.env["AWS_PROFILE"],
    }),
    ...(process.env["AWS_REGION"] ? { region: process.env["AWS_REGION"] } : {}),
  });
}

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

export function formatError(content: Content) {
  try {
    const { message } = content.meta!.debug!.error!;
    const model = content.meta!.debug!.model!;
    const region =
      process.env["AWS_REGION"] ??
      (content.meta?.debug as BedrockDebug | undefined)?.region ??
      "unknown region";
    let base = "There was an error calling Bedrock. ";
    switch (message) {
      case "The security token included in the request is invalid.":
        base += message + " Please refresh your AWS CLI credentials.";
        break;
      case "The security token included in the request is expired":
      case "Could not load credentials from any providers":
        base += message + ". Please refresh your AWS CLI credentials.";
        break;
      case "Could not resolve the foundation model from the provided model identifier.":
        base += `The model ${model} could not be resolved.`;
        if (Object.values(MODEL_MAP).includes(model)) {
          base += ` Please ensure it is enabled in AWS region ${region}, or change your AWS region.`;
        } else {
          base += ` Please verify it is the correct identifier for your foundation or custom model.`;
        }
        break;
      case "You don't have access to the model with the specified model ID.":
        base += `Please enable model access to model with id ${model} in the AWS region ${region}.`;
        break;
      case "The provided model identifier is invalid.":
        base += `The provided model identifier "${model}" is not recognized. Please ensure it is a valid model deployed in AWS region ${region}, or change your AWS region.`;
        break;
      default:
        base += message;
        break;
    }
    return base;
  } catch (_) {
    return undefined;
  }
}
