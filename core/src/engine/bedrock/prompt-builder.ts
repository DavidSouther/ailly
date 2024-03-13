import { Message } from "..";

export type Models =
  | "anthropic.claude-v2"
  | "anthropic.claude-3-sonnet-20240229-v1:0";

export class PromptBuilder {
  modelBuilders: Record<Models, (m: Message[]) => any> = {
    "anthropic.claude-v2": claude,
    "anthropic.claude-3-sonnet-20240229-v1:0": claude3,
  };

  constructor(private readonly modelName: Models) {}

  build(messages: Message[]) {
    return this.modelBuilders[this.modelName](messages);
  }
}

export function claude(messages: Message[]): {
  prompt: String;
  max_tokens_to_sample: number;
} {
  let output = "";
  let prevRole = "";

  const roleAdjustedMsgs = messages.map((msg) => ({
    ...msg,
    role: msg.role === "assistant" ? "Assistant" : "Human",
  }));

  roleAdjustedMsgs.forEach((msg) => {
    if (msg.role !== prevRole) {
      output += "\n\n" + msg.role + ": ";
      prevRole = msg.role;
    } else {
      output += "\n";
    }

    output += msg.content;
  });

  return { prompt: output + "\n\nAssistant:", max_tokens_to_sample: 8191 };
}

export interface LLMMessage {
  role: "user" | "assistant";
  content: string;
}

export function claude3(messages: Message[]): {
  messages: LLMMessage[];
  system: string;
  max_tokens: number;
  anthropic_version: "bedrock-2023-05-31"; //
} {
  let system = "";
  const promptMessages: LLMMessage[] = [];
  messages.forEach((message, i) => {
    switch (message.role) {
      case "system":
        system += message.content + "\n";
        break;
      case "assistant":
      case "user":
        if (promptMessages[i - 1]?.role === message.role) {
          promptMessages[i - 1].content += "\n" + message.content;
        } else {
          promptMessages.push({ role: message.role, content: message.content });
        }
        break;
    }
  });
  return {
    messages: promptMessages,
    max_tokens: 1024,
    system,
    anthropic_version: "bedrock-2023-05-31",
  };
}
