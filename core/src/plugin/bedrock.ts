import { BedrockRuntimeClient, InvokeModelCommand } from '@aws-sdk/client-bedrock-runtime';
import { Content } from "../content.js";
import { isDefined } from "../util.js";
import { Message, Summary } from "./index.js";

export const DEFAULT_MODEL = 'anthropic.claude-v2';

export function buildClaude2Prompt(messages: Message[]) {
  let output = '';
  let prevRole = "";

  const roleAdjustedMsgs = messages.map(msg => ({ ...msg, role: msg.role === 'assistant' ? 'Assistant' : 'Human' }))

  roleAdjustedMsgs.forEach(msg => {
    if (msg.role !== prevRole) {
      output += '\n\n' + msg.role + ': ';
      prevRole = msg.role;
    }
    else {
      output += '\n';
    }

    output += msg.content;
  });

  return output + '\n\nAssistant:';
}

export async function generate(
  c: Content,
  {
    model = DEFAULT_MODEL,
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
    body: JSON.stringify({ prompt: buildClaude2Prompt(messages), max_tokens_to_sample: 8191 })
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
  content.meta.tokens = 0;
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
