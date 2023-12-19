import { Message } from "..";

export type Models = "anthropic.claude-v2";

export class PromptBuilder {
  modelBuilders: Record<Models, (m: Message[]) => any> = {
    "anthropic.claude-v2": claude,
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
