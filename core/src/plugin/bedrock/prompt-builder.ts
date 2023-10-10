import { Message } from "..";

export type Models = "anthropic.claude-v2";

export class PromptBuilder {
  modelBuilders: Record<Models, (m: Message[]) => any> = {
    "anthropic.claude-v2": claude,
  };

  constructor(private readonly modelName: Models) {}

  build<R>(messages: Message[]): R {
    return this.modelBuilders[this.modelName](messages);
  }
}

export function claude(messages: Message[]) {
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

  return output + "\n\nAssistant:";
}
