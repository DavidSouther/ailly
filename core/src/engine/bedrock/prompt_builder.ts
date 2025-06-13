import type {
  ContentBlock,
  ConverseStreamCommandInput,
  Message,
  SystemContentBlock,
  Tool,
  ToolInputSchema,
  ToolResultBlock,
  ToolResultContentBlock,
} from "@aws-sdk/client-bedrock-runtime";
import { Err, isOk, unwrap } from "@davidsouther/jiffies/lib/cjs/result.js";

import type { Content } from "../../content/content.js";
import { DEFAULT_SYSTEM_PROMPT } from "../index.js";
import type { ToolInvocationResult } from "../tool";
export interface Prompt {
  messages: Message[];
  system: string;
}

export type Models =
  | "us.amazon.nova-lite-v1:0"
  | "us.amazon.nova-pro-v1:0"
  | "us.anthropic.claude-3-7-sonnet-20250219-v1:0"
  | "us.anthropic.claude-3-haiku-20240307-v1:0"
  | "us.anthropic.claude-3-opus-20240229-v1:0"
  | "us.anthropic.claude-opus-4-20250514-v1:0"
  | "us.anthropic.claude-sonnet-4-20250514-v1:0";

export const MODEL_MAP: Record<string, Models> = {
  sonnet: "us.anthropic.claude-3-7-sonnet-20250219-v1:0",
  haiku: "us.anthropic.claude-3-haiku-20240307-v1:0",
  opus: "us.anthropic.claude-3-opus-20240229-v1:0",
  sonnet3: "us.anthropic.claude-3-7-sonnet-20250219-v1:0",
  haiku3: "us.anthropic.claude-3-haiku-20240307-v1:0",
  opus3: "us.anthropic.claude-3-opus-20240229-v1:0",
  "sonnet-3": "us.anthropic.claude-3-7-sonnet-20250219-v1:0",
  "haiku-3": "us.anthropic.claude-3-haiku-20240307-v1:0",
  "opus-3": "us.anthropic.claude-3-opus-20240229-v1:0",
  sonnet4: "us.anthropic.claude-sonnet-4-20250514-v1:0",
  opus4: "us.anthropic.claude-opus-4-20250514-v1:0",
  "sonnet-4": "us.anthropic.claude-sonnet-4-20250514-v1:0",
  "opus-4": "us.anthropic.claude-opus-4-20250514-v1:0",
  nova: "us.amazon.nova-lite-v1:0",
  novapro: "us.amazon.nova-pro-v1:0",
  "nova-lite": "us.amazon.nova-lite-v1:0",
  "nova-pro": "us.amazon.nova-pro-v1:0",
  nova_lite: "us.amazon.nova-lite-v1:0",
  nova_pro: "us.amazon.nova-pro-v1:0",
};

export class PromptBuilder {
  constructor(private readonly modelName: Models) {}

  build(content: Content): ConverseStreamCommandInput {
    return converseBuilder(this.modelName, content);
  }
}

export function converseBuilder(
  model: Models,
  content: Content,
): ConverseStreamCommandInput {
  const system: SystemContentBlock = { text: "" };
  const messages: Message[] = [];
  for (const message of content.meta?.messages ?? []) {
    if (message.role === "system") {
      system.text += `${message.content}\n`;
    } else {
      if (messages.at(-1)?.role !== message.role) {
        messages.push({
          role: message.role,
          content: [],
        });
      }
      if (message.content) {
        messages.at(-1)?.content?.push({ text: message.content });
      }
      if (message.toolUse) {
        messages.at(-2)?.content?.push({
          toolUse: {
            name: message.toolUse.name,
            toolUseId: message.toolUse.id,
            // biome-ignore lint/suspicious/noExplicitAny: Bedrock keeps DocumentType private
            input: message.toolUse.input as any,
          },
        });
        messages.at(-1)?.content?.push({
          toolResult: toolUseResult(message.toolUse),
        } satisfies ContentBlock.ToolResultMember);
      }
    }
  }

  system.text = system.text.trim() || DEFAULT_SYSTEM_PROMPT;

  const stopSequences = content.context.edit ? ["```"] : undefined;
  // Use given temperature; or 0 for edit (don't want creativity) but 1.0 generally.
  const temperature =
    (content.meta?.temperature ?? content.context.edit !== undefined)
      ? 0.0
      : 1.0;
  const maxTokens = content.meta?.maxTokens;

  const toolConfig = contentToToolConfig(content);

  return {
    modelId: model,
    system: [system],
    messages,
    ...toolConfig,
    inferenceConfig: {
      maxTokens,
      stopSequences,
      temperature,
    },
  };
}

export const contentToToolConfig = (content: Content) => {
  return content.meta?.tools
    ? {
        toolConfig: {
          tools: content.meta?.tools?.map(
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
        },
      }
    : {};
};

export function toolUseResult({
  result,
  id: toolUseId = "unknown",
}: {
  result: ToolInvocationResult;
  id?: string;
}): ToolResultBlock {
  if (isOk(result)) {
    return {
      toolUseId,
      status: "success",
      content: [{ json: unwrap(result) } as unknown as ToolResultContentBlock],
    };
  }
  return {
    toolUseId,
    status: "error",
    content: [{ json: Err(result) } as unknown as ToolResultContentBlock],
  };
}
