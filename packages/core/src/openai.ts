import { get_encoding } from "@dqbd/tiktoken";
import { Content } from "./content";
import { isDefined } from "./util";

async function addContentMeta(content: Content) {
  let messages = getMessages(content);
  let tokens = 0;
  for (const message of messages) {
    tokens += message.tokens = (await encoding.encode(message.content)).length;
  }
  content.meta = {
    messages,
    tokens,
  };
  return tokens;
}

const encoding = get_encoding("cl100k_base");
export async function addMessagesToContent(
  contents: Content[]
): Promise<Summary> {
  const summary: Summary = { prompts: contents.length, tokens: 0 };
  for (const content of contents) {
    summary.tokens += await addContentMeta(content);
  }
  return summary;
}

export interface Message {
  role: "system" | "user" | "assistant";
  content: string;
  tokens?: number;
}

export function getMessages(content: Content): Message[] {
  const system = content.system.join("\n");
  const history: Content[] = [];
  while (content) {
    history.push(content);
    content = content.predecessor!;
  }
  history.reverse();
  return [
    { role: "system", content: system },
    ...history
      .map<Array<Message | undefined>>((content) => [
        {
          role: "user",
          content: content.prompt,
        },
        content.response
          ? { role: "assistant", content: content.response }
          : undefined,
      ])
      .flat()
      .filter(isDefined),
  ];
}

export interface Summary {
  tokens: number;
  prompts: number;
}
