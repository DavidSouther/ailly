import type { Message } from "../index.js";

export interface Prompt {
  messages: LLMMessage[];
  system: string;
}

export interface LLMMessage {
  role: "user" | "assistant";
  content: string;
}

export type Models =
  | "anthropic.claude-v2"
  | "anthropic.claude-3-opus-20240229-v1:0"
  | "us.anthropic.claude-3-opus-20240229-v1:0" // cross-region inference
  | "anthropic.claude-3-sonnet-20240229-v1:0"
  | "us.anthropic.claude-3-7-sonnet-20250219-v1:0" // cross-region inference
  | "anthropic.claude-3-haiku-20240307-v1:0"
  | "us.anthropic.claude-3-haiku-20240307-v1:0" // cross-region inference
  | "us.amazon.nova-lite-v1:0"
  | "us.amazon.nova-pro-v1:0";

export class PromptBuilder {
  modelBuilders: Record<Models, (m: Message[]) => any> = {
    "anthropic.claude-v2": converseBuilder,
    "anthropic.claude-3-opus-20240229-v1:0": converseBuilder,
    "us.anthropic.claude-3-opus-20240229-v1:0": converseBuilder,
    "anthropic.claude-3-sonnet-20240229-v1:0": converseBuilder,
    "us.anthropic.claude-3-7-sonnet-20250219-v1:0": converseBuilder,
    "anthropic.claude-3-haiku-20240307-v1:0": converseBuilder,
    "us.anthropic.claude-3-haiku-20240307-v1:0": converseBuilder,
    "us.amazon.nova-lite-v1:0": converseBuilder,
    "us.amazon.nova-pro-v1:0": converseBuilder,
  };

  constructor(private readonly modelName: Models) {}

  build(messages: Message[]): Prompt {
    return converseBuilder(messages);
  }
}

export function converseBuilder(messages: Message[]): Prompt {
  let system = "";
  const promptMessages: LLMMessage[] = [];
  for (const message of messages) {
    switch (message.role) {
      case "system":
        system += `${message.content}\n`;
        break;
      case "assistant":
      case "user": {
        const last = promptMessages.at(-1);
        if (last?.role === message.role) {
          last.content += `\n${message.content}`;
        } else {
          promptMessages.push({
            role: message.role,
            content: message.content,
          });
        }
        break;
      }
    }
  }

  return {
    messages: promptMessages,
    system,
  };
}
