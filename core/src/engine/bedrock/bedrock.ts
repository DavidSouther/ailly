import {
  type Message as BedrockMessage,
  BedrockRuntimeClient,
  type ContentBlock,
  ConverseStreamCommand,
  type ConverseStreamCommandInput,
  InvokeModelCommand,
  type SystemContentBlock,
  type Tool,
  type ToolInputSchema,
} from "@aws-sdk/client-bedrock-runtime";
import { fromNodeProviderChain } from "@aws-sdk/credential-providers";
import { getLogger } from "@davidsouther/jiffies/lib/cjs/log.js";

import { assertExists } from "@davidsouther/jiffies/lib/cjs/assert";
import type { Content, ContentMeta, View } from "../../content/content.js";
import { LOGGER as ROOT_LOGGER, content } from "../../index.js";
import type { EngineDebug, EngineGenerate, Summary } from "../index.js";
import { addContentMessages } from "../messages.js";
import { type Models, type Prompt, PromptBuilder } from "./prompt_builder.js";

export const name = "bedrock";
export const DEFAULT_MODEL = "sonnet";

const LOGGER = getLogger("@ailly/core:bedrock");
const DEFAULT_SYSTEM_PROMPT = "You are Ailly, the helpful AI Writer's Ally.";

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
  sonnet: "us.anthropic.claude-3-7-sonnet-20250219-v1:0",
  haiku: "us.anthropic.claude-3-haiku-20240307-v1:0",
  opus: "us.anthropic.claude-3-opus-20240229-v1:0",
  nova_lite: "us.amazon.nova-lite-v1:0",
  nova_pro: "us.amazon.nova-pro-v1:0",
};

declare module "@davidsouther/jiffies/lib/cjs/log.js" {
  interface Logger {
    /** Augment our logger with a `trace` method to trace streaming calls. */
    trace: Log;
  }
}

export const generate: EngineGenerate<BedrockDebug> = (
  c: Content,
  { model = DEFAULT_MODEL }: { model?: string },
) => {
  LOGGER.level = ROOT_LOGGER.level;
  LOGGER.format = ROOT_LOGGER.format;
  LOGGER.console = ROOT_LOGGER.console;
  LOGGER.trace = LOGGER.logAt(0.5, "TRACE");
  const bedrock = makeClient();
  model = MODEL_MAP[model] ?? model;
  const inMessages = c.meta?.messages ?? [];
  if (!inMessages.find((m) => m.role === "user")) {
    throw new Error("Bedrock must have at least one message with role: user");
  }
  const promptBuilder = new PromptBuilder(model as Models);
  const prompt: Prompt = promptBuilder.build(inMessages);
  prompt.system = prompt.system.trim() || DEFAULT_SYSTEM_PROMPT;

  LOGGER.debug("Bedrock sending prompt", {
    ...prompt,
    messages: [`...${inMessages.length} messages...`],
    system: `${prompt.system.slice(0, 20)}...`,
  });

  try {
    const debug: BedrockDebug = {
      id: "",
      finish: "unknown",
      model,
      engine: "bedrock",
    };
    bedrock.config.region().then((region) => {
      debug.region = region;
    });
    const stream = new TransformStream();
    const writer = stream.writable.getWriter();
    let message: string = c.meta?.continue ? (c.response ?? "") : "";
    const converseStreamCommand: ConverseStreamCommandInput =
      createConverseStreamCommand(model, c, prompt);
    LOGGER.trace("Sending ConverseStreamCommand", {
      converseStreamCommand,
    });
    const done = bedrock
      .send(new ConverseStreamCommand(converseStreamCommand))
      .then(async (response) => {
        LOGGER.debug(`Begin streaming response from Bedrock for ${c.name}`);

        let blocks = 0;
        for await (const block of response.stream ?? []) {
          blocks += 1;
          LOGGER.trace("Got response stream message", block);
          if (block.validationException) {
            throw new Error(
              "The input fails to satisfy the constraints specified by Amazon Bedrock.",
              { cause: block.validationException },
            );
          }

          if (block.serviceUnavailableException) {
            throw new Error("The service isn't currently available.", {
              cause: block.serviceUnavailableException,
            });
          }
          if (block.throttlingException) {
            throw new Error(
              "Your request was denied due to exceeding the account quotas for Amazon Bedrock.",
              {
                cause: block.throttlingException,
              },
            );
          }

          if (
            block.internalServerException ||
            block.modelStreamErrorException
          ) {
            // An internal server error occurred. Retry your request.
            // A streaming error occurred. Retry your request.
            await new Promise<void>((r, _j) => {
              // pass some time
              setTimeout(() => r(), 20);
            });
          }

          if (block.metadata) {
            // block.metadata.metrics?.latencyMs
            // block.metadata.usage?.inputTokens
            // block.metadata.usage?.outputTokens
            // block.metadata.usage?.totalTokens
            // block.metadata.performanceConfig?.latency // OPTIMIZED || STANDARD
            // block.metadata.trace?.guardrail?.inputAssessment
            // block.metadata.trace?.guardrail?.modelOutput
            // block.metadata.trace?.guardrail?.outputAssessments
            debug.id =
              block.metadata.trace?.promptRouter?.invokedModelId ?? debug.id;
          }

          if (block.messageStart) {
            // debug.role = block.messageStart.role
          }

          if (block.contentBlockStart) {
            const { name } = block.contentBlockStart.start?.toolUse ?? {
              name: undefined,
            };
            if (name) {
              debug.toolUse = { name, input: {}, partial: "" };
            }
          }

          if (block.contentBlockDelta) {
            const text = block.contentBlockDelta.delta?.text;
            if (text) {
              message += text;
              await writer.ready;
              await writer.write(text);
            }
            const tool = block.contentBlockDelta.delta?.toolUse;
            if (tool && debug.toolUse) {
              debug.toolUse.partial += tool.input ?? "";
            }
          }

          if (block.contentBlockStop) {
            if (debug.toolUse) {
              debug.toolUse.input = JSON.parse(debug.toolUse.partial);
            }
          }

          if (block.messageStop) {
            debug.finish = block.messageStop.stopReason;
          }
        }

        LOGGER.debug(
          `Finished streaming response from Bedrock for ${c.name} (${debug.id}), total ${blocks} blocks.`,
        );
      })
      .catch((e) => {
        debug.finish = "failed";
        debug.error = e as Error;
        LOGGER.warn(
          `Error for Bedrock response ${c.name} (${debug.id ?? "no id"})`,
          {
            err: debug.error,
          },
        );
      })
      .finally(async () => {
        LOGGER.debug(
          `Closing write stream for bedrock response ${c.name} (${debug.id})`,
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

export const contentToToolConfig = (c: Content) => {
  return {
    tools: c.meta?.tools?.map(
      (tool) =>
        ({
          toolSpec: {
            name: tool.name,
            description: tool.description,
            inputSchema: {
              json: tool.parameters,
            } as unknown as ToolInputSchema.JsonMember, // We're more strict than Bedrock, hence the unknown cast.
          },
        }) as Tool.ToolSpecMember,
    ),
    // {
    //   "toolSpec": {
    //     "name": "top_song",
    //     "description": "Get the most popular song played on a radio station.",
    //     "inputSchema": {
    //       "json": {
    //         "type": "object",
    //         "properties": {
    //           "sign": {
    //             "type": "string",
    //             "description": "The call sign for the radio station for which you want the most popular song. Example calls signs are WZPZ and WKRP."
    //           }
    //         },
    //         "required": [
    //           "sign"
    //         ]
    //       }
    //     }
    //   }
    // }
  };
};

export const createConverseStreamCommand = (
  model: string,
  c: Content,
  prompt: Prompt,
): ConverseStreamCommandInput => {
  const stopSequences = c.context.edit ? ["```"] : undefined;
  // Use given temperature; or 0 for edit (don't want creativity) but 1.0 generally.
  const temperature =
    (c.meta?.temperature ?? c.context.edit !== undefined) ? 0.0 : 1.0;
  const maxTokens = c.meta?.maxTokens;

  return {
    modelId: model,
    system: [
      {
        text: prompt.system,
      } satisfies SystemContentBlock,
    ],
    messages: prompt.messages.map((message) => ({
      role: message.role,
      content: [{ text: message.content } satisfies ContentBlock],
    })),
    toolConfig: contentToToolConfig(c),
    inferenceConfig: {
      maxTokens,
      stopSequences,
      temperature,
    },
  };
};

function makeClient() {
  return new BedrockRuntimeClient({
    credentials: fromNodeProviderChain({
      ignoreCache: true,
      profile: process.env.AWS_PROFILE,
    }),
    ...(process.env.AWS_REGION
      ? { region: process.env.AWS_REGION }
      : { region: "us-west-2" }),
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
  context: Record<string, Content>,
): Promise<Summary> {
  const summary: Summary = { prompts: contents.length, tokens: 0 };
  for (const content of contents) {
    await addContentMessages(content, context);
  }
  return summary;
}

export async function tune(content: Content[]) {}

const td = new TextDecoder();
export async function vector(inputText: string, _: object): Promise<number[]> {
  const bedrock = new BedrockRuntimeClient({});
  const response = await bedrock.send(
    new InvokeModelCommand({
      body: JSON.stringify({ inputText }),
      modelId: "amazon.titan-embed-text-v1",
      contentType: "application/json",
      accept: "*/*",
    }),
  );

  const body = JSON.parse(td.decode(response.body));

  return body.embedding;
}

export function formatError(content: Content) {
  try {
    const debug = content.meta?.debug;
    const { message } = debug?.error ?? { message: "Unknown Error" };
    const model = assertExists(
      debug?.model,
      "Missing `model` in debug when formatting Error",
    );
    const region =
      process.env.AWS_REGION ??
      (content.meta?.debug as BedrockDebug | undefined)?.region ??
      "unknown region";
    let base = "There was an error calling Bedrock. ";
    switch (message) {
      case "The security token included in the request is invalid.":
        base += `${message} Please refresh your AWS CLI credentials.`;
        break;
      case "The security token included in the request is expired":
      case "Could not load credentials from any providers":
        base += `${message}. Please refresh your AWS CLI credentials.`;
        break;
      case "Could not resolve the foundation model from the provided model identifier.":
        base += `The model ${model} could not be resolved.`;
        if (Object.values(MODEL_MAP).includes(model)) {
          base += ` Please ensure it is enabled in AWS region ${region}, or change your AWS region.`;
        } else {
          base +=
            " Please verify it is the correct identifier for your foundation or custom model.";
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
