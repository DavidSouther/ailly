import { performance } from "node:perf_hooks";

import {
  BedrockRuntimeClient,
  ConverseStreamCommand,
  type ConverseStreamCommandInput,
  InvokeModelCommand,
} from "@aws-sdk/client-bedrock-runtime";
import { fromNodeProviderChain } from "@aws-sdk/credential-providers";
import { getLogger } from "@davidsouther/jiffies/lib/cjs/log.js";

import { assertExists } from "@davidsouther/jiffies/lib/cjs/assert";
import type { Content, View } from "../../content/content.js";
import { LOGGER as ROOT_LOGGER } from "../../index.js";
import type { EngineDebug, EngineGenerate, Summary } from "../index.js";
import { addContentMessages } from "../messages.js";
import { MODEL_MAP, type Models, PromptBuilder } from "./prompt_builder.js";

export const name = "bedrock";
export const DEFAULT_MODEL = "sonnet";

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
  c.meta ??= {};
  c.meta.messages ??= [];
  if (!c.meta.messages.find((m) => m.role === "user")) {
    throw new Error("Bedrock must have at least one message with role: user");
  }
  const promptBuilder = new PromptBuilder(model as Models);
  const converseStreamCommand: ConverseStreamCommandInput =
    promptBuilder.build(c);

  LOGGER.debug("Bedrock sending prompt", {
    messages: converseStreamCommand.messages?.map((m) => ({
      role: m.role,
      content: m.content?.map((block) => {
        switch (true) {
          case !!block.text:
            return `${block.text.slice(0, 60)}...`;
          case !!block.toolUse:
            return `${block.toolUse.name}(${JSON.stringify(block.toolUse.input)})#${block.toolUse.toolUseId}`;
          case !!block.toolResult:
            return `#${block.toolResult.toolUseId} = ${JSON.stringify(block.toolResult.content)}`;
        }
      }),
    })),
    system: `${converseStreamCommand.system?.slice(0, 20) ?? ""}...`,
  });

  LOGGER.trace("Sending ConverseStreamCommand", {
    converseStreamCommand,
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
    const write = async (text: string | undefined) => {
      if (text) {
        message += text;
        await writer.ready;
        await writer.write(text);
      }
    };
    let message: string = c.meta?.continue ? (c.response ?? "") : "";
    const start = performance.now();
    let firstResponse = -1;
    const done = bedrock
      .send(new ConverseStreamCommand(converseStreamCommand))
      .then(async (response) => {
        LOGGER.debug(`Begin streaming response from Bedrock for ${c.name}`);

        let blocks = 0;
        for await (const block of response.stream ?? []) {
          if (blocks === 0) {
            firstResponse = performance.now();
          }
          blocks += 1;
          LOGGER.trace("Got response stream message", { blocks, ...block });
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
            // if (block.metadata.metrics?.latencyMs) {
            //   debug.statistics = {...debug.statistics, invocationLatency: }
            // }
            if (block.metadata.usage?.inputTokens) {
              debug.statistics = {
                ...debug.statistics,
                inputTokenCount: block.metadata.usage.inputTokens,
              };
            }
            if (block.metadata.usage?.outputTokens) {
              debug.statistics = {
                ...debug.statistics,
                outputTokenCount: block.metadata.usage.outputTokens,
              };
            }
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
            const toolUse = block.contentBlockStart.start?.toolUse;
            const { name, toolUseId } = toolUse ?? { name: undefined };
            if (name && toolUseId) {
              debug.toolUse = { name, input: {}, partial: "", id: toolUseId };
              await write(" ");
            }
          }

          if (block.contentBlockDelta) {
            await write(block.contentBlockDelta.delta?.text);
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
        const finished = performance.now();

        const firstByteLatency = firstResponse - start;
        const invocationLatency = finished - start;

        debug.statistics = {
          ...debug.statistics,
          firstByteLatency,
          invocationLatency,
        };

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
        if (Object.values(MODEL_MAP).includes(model as unknown as Models)) {
          base += ` Please ensure it is enabled in AWS region ${region}, or change your AWS region.`;
        } else {
          base +=
            " Please verify it is the correct identifier for your foundation or custom model.";
        }
        break;
      case "You don't have access to the model with the specified model ID.":
        base += `Please enable model access to model with id ${model} in the AWS region ${region}.\n`;
        base += `https://${region}.console.aws.amazon.com/bedrock/home?region=${region}#/modelaccess`;
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
