import { BedrockRuntimeClient, InvokeModelCommand } from '@aws-sdk/client-bedrock-runtime';
import { get_encoding } from "@dqbd/tiktoken";
import { Content } from "../content.js";
import { isDefined } from "../util.js";
import { Message, Summary } from "./index.js";

const MODEL = 'anthropic.claude-v2';

export function buildClaude2Prompt(messages: Message[]) {
  const prefixedMessages = messages.map(m => {
    let prefix: "Human" | "Assistant";
    if (m.role === "user" || m.role === "system") {
      prefix = "Human";
    } else {
      prefix = "Assistant";
    }

    return `\n\n${prefix}: ${m.content}`
  });

  prefixedMessages.push("\n\nAssistant:");

  return prefixedMessages.join("");
}

export async function generate(
  c: Content,
  {
    model = MODEL,
  }: { model: string; }
): Promise<{ message: string; debug: unknown }> {
  const bedrock = new BedrockRuntimeClient({});
  let messages = c.meta?.messages ?? [];
  if (messages.length < 2) {
    throw new Error("Not enough messages");
  }
  if (messages.at(-1)?.role == "assistant") {
    messages = messages.slice(0, -1);
  }
  console.log(
    "Calling Bedrock",
    messages.map((m) => ({
      role: m.role,
      content: m.content.replaceAll("\n", "").substring(0, 50) + "...",
      tokens: m.tokens,
    }))
  );
  const response = await bedrock.send(new InvokeModelCommand({
    modelId: model,
    contentType: 'application/json',
    accept: 'application/json',
    body: buildClaude2Prompt(messages)
  }));

  const body = JSON.parse(response.body.transformToString());

  console.log(`Response from Bedrock for ${c.name}`, {
    finish_reason: body.stop_reason,
  });
  return {
    message: body.completion ?? "",
    debug: {
      id: null,
      model,
      usage: null,
      finish: body.stop_reason,
    },
  };
}
// TODO: update formatter here for bedrock
export async function format(contents: Content[]): Promise<Summary> {
  const summary: Summary = { prompts: contents.length, tokens: 0 };
  for (const content of contents) {
    summary.tokens += await addContentMeta(content);
  }
  return summary;
}

const encoding = get_encoding("cl100k_base");
async function addContentMeta(content: Content) {
  content.meta ??= {};
  content.meta.messages = getMessages(content);
  content.meta.tokens = 0;
  for (const message of content.meta.messages) {
    const toks = (await encoding.encode(message.content)).length;
    message.tokens = toks;
    content.meta.tokens += toks;
  }
  return content.meta.tokens;
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

export async function tune(
  content: Content[],
) { }
