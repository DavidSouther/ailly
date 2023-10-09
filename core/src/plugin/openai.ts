import { OpenAI, toFile } from "openai";
import { get_encoding } from "@dqbd/tiktoken";
import { Content } from "../content.js";
import { isDefined } from "../util.js";
import { Message, Summary } from "./index.js";

// const MODEL = "gpt-3.5-turbo-0613";
// const FT_MODEL = process.env["OPENAI_FT_MODEL"];
const MODEL = "gpt-4-0613";
// const MODEL = `ft:${BASE_MODEL}:personal::${FT_MODEL}`;
// const MODEL = "gpt-3.5-turbo-16k-0613";

export async function generate(
  c: Content,
  {
    model = MODEL,
    apiKey = process.env["OPENAI_API_KEY"] ?? "",
  }: { model: string; apiKey: string }
): Promise<{ message: string; debug: unknown }> {
  const openai = new OpenAI({ apiKey });
  let messages = c.meta?.messages ?? [];
  if (messages.length < 2) {
    throw new Error("Not enough messages");
  }
  if (messages.at(-1)?.role == "assistant") {
    messages = messages.slice(0, -1);
  }
  console.log(
    "Calling OpenAI",
    messages.map((m) => ({
      role: m.role,
      content: m.content.replaceAll("\n", "").substring(0, 50) + "...",
      tokens: m.tokens,
    }))
  );
  const completions = await openai.chat.completions.create({
    messages: (c.meta?.messages ?? []).map(({ role, content }) => ({
      role,
      content,
    })),
    model,
  });
  const choice = completions.choices[0];
  console.log(`Response from OpenAI for ${c.name}`, {
    id: completions.id,
    finish_reason: choice.finish_reason,
  });
  return {
    message: choice.message.content ?? "",
    debug: {
      id: completions.id,
      model: completions.model,
      usage: completions.usage,
      finish: choice.finish_reason,
    },
  };
}

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
  {
    model = MODEL,
    apiKey = process.env["OPENAI_API_KEY"] ?? "",
  }: { model: string; apiKey: string }
) {
  const openai = new OpenAI({ apiKey });
  const summary = await format(content);

  const file = content
    .map((c) =>
      JSON.stringify({
        messages: (c.meta?.messages ?? []).map(({ role, content }) => ({
          role,
          content,
        })),
      })
    )
    .join("\n");
  const trainingFile = await openai.files.create({
    file: await toFile(Buffer.from(file)),
    purpose: "fine-tune",
  });

  console.log("Created openai training file", trainingFile);

  const fineTune = await openai.fineTuning.jobs.create({
    training_file: trainingFile.id,
    model: "gpt-3.5-turbo",
  });

  console.log("Started fine-tuning job", fineTune);
  console.log(
    `New fine tuning model should be ft:${fineTune.model}:${fineTune.organization_id}::${fineTune.id}`
  );
}
