import {
  BedrockRuntimeClient,
  InvokeModelCommand,
} from "@aws-sdk/client-bedrock-runtime";
import { Content } from "../../content/content.js";
import { isDefined } from "../../util.js";
import { Message, Summary } from "../index.js";
import { Models, PromptBuilder } from "./prompt-builder.js";

export const name = "bedrock";
export const DEFAULT_MODEL: Models = "anthropic.claude-v2";

const promptBuilder = new PromptBuilder(DEFAULT_MODEL);

export async function generate(
  c: Content,
  { model = DEFAULT_MODEL }: { model: string }
): Promise<{ message: string; debug: unknown }> {
  const bedrock = new BedrockRuntimeClient({});

  let messages = c.meta?.messages ?? [];
  if (messages.length < 2) {
    throw new Error("Not enough messages");
  }
  if (messages.at(-1)?.role == "assistant") {
    messages = messages.slice(0, -1);
  }

  const prompt = promptBuilder.build(messages);
  const response = await bedrock.send(
    new InvokeModelCommand({
      modelId: model,
      contentType: "application/json",
      accept: "application/json",
      body: JSON.stringify(prompt),
    })
  );

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

export async function format(contents: Content[]): Promise<Summary> {
  const summary: Summary = { prompts: contents.length, tokens: 0 };
  for (const content of contents) {
    await addContentMeta(content);
  }
  return summary;
}

async function addContentMeta(content: Content) {
  content.meta ??= {};
  content.meta.messages = getMessages(content);
}

export function getMessages(content: Content): Message[] {
  const system = (content.system ?? []).join("\n");
  const history: Content[] = [];
  while (content) {
    history.push(content);
    content = content.predecessor!;
  }
  history.reverse();
  const augment = history
    .map<Array<Message | undefined>>(
      (c) =>
        (c.augment ?? []).map<Message>(({ content }) => ({
          role: "user",
          content: "Background information: " + content,
        })) ?? []
    )
    .flat()
    .filter(isDefined);
  const parts = history
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
    .filter(isDefined);
  return [{ role: "system", content: system }, ...augment, ...parts];
}

export async function tune(content: Content[]) {}

const td = new TextDecoder();
export async function vector(inputText: string, {}: {}): Promise<number[]> {
  const bedrock = new BedrockRuntimeClient({});
  const response = await bedrock.send(
    new InvokeModelCommand({
      body: JSON.stringify({ inputText }),
      modelId: "amazon.titan-embed-text-v1",
      contentType: "application/json",
      accept: "*/*",
    })
  );

  const body = JSON.parse(td.decode(response.body));

  return body.embedding;
}
