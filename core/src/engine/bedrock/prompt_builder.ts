import type {
  ContentBlock,
  ConverseStreamCommandInput,
  SystemContentBlock,
  Tool,
  ToolInputSchema,
} from "@aws-sdk/client-bedrock-runtime";
import type { Content } from "../../content/content.js";
import { DEFAULT_SYSTEM_PROMPT, type Message } from "../index.js";

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
  let system = "";
  const messages: Message[] = [];
  for (const message of content.meta?.messages ?? []) {
    if (message.role === "system") {
      system += `${message.content}\n`;
    } else if (message.role === "user" && message.toolUse) {
      messages.push({
        role: message.role,
        toolUse: message.toolUse,
        content: "",
      });
    } else {
      const last = messages.at(-1);
      if (last?.role === message.role) {
        last.content += `\n${message.content}`;
      } else {
        messages.push({
          role: message.role,
          content: message.content,
        });
      }
    }
  }

  system = system.trim() || DEFAULT_SYSTEM_PROMPT;

  const prompt = { messages, system };
  const command = createConverseStreamCommand(model, content, prompt);
  return command;
}

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
  };
};
