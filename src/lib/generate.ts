"use server";

import { Content, addMessagesToContent, loadContent } from "@/lib/content";
import { NodeFileSystem } from "@/lib/fs";
import OpenAI, { toFile } from "openai";
import { dirname, join } from "path";

// const BASE_MODEL = "gpt-3.5-turbo-0613";
// const FT_MODEL = process.env["OPENAI_FT_MODEL"];
const MODEL = "gpt-4-0613";
// const MODEL = `ft:${BASE_MODEL}:personal::${FT_MODEL}`;
// const MODEL = "gpt-3.5-turbo-16k-0613";

export async function generateAllAction() {
  await generateAll();
  return { message: "Generating" };
}

export async function tuneAction() {
  await tune();
  return { message: "Tuning" };
}

function getFs() {
  const fs = new NodeFileSystem();
  fs.cd("content");
  return fs;
}

// TODO make this async*
export async function generateAll() {
  // TODO Ensure that `previous` content gets generated first and included in future calls.
  const content = await loadContent(getFs());
  const summary = await addMessagesToContent(content);

  content.forEach(generateOne);
}

export async function tune() {
  const content = await loadContent(getFs());
  const summary = await addMessagesToContent(content);

  const file = content
    .map((c) =>
      JSON.stringify({
        messages: (c.messages ?? []).map(({ role, content }) => ({
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

const openai = new OpenAI({
  apiKey: process.env.OPENAI_API_KEY,
});

async function generateLlamaSagemaker() {}
async function generateTitan() {}
async function generateClaude() {}

async function generateOpenAi(
  c: Content
): Promise<{ message: string; debug: unknown }> {
  let messages = c.messages ?? [];
  if (messages.at(-1)?.role == "assistant") {
    messages = messages.slice(0, -1);
  }
  console.log("Calling OpenAI", messages);
  const completions = await openai.chat.completions.create({
    messages: (c.messages ?? []).map(({ role, content }) => ({
      role,
      content,
    })),
    model: MODEL,
  });
  console.log(`Response from OpenAI for ${c.id}`, completions);
  const choice = completions.choices[0];
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

export async function generateOne(c: Content): Promise<void> {
  const generated = await generateOpenAi(c);
  const path = join(dirname(c.path), `${c.order}r_${c.id}`);
  await getFs().writeFile(
    path,
    `---\ngenerated: ${new Date().toISOString()}\ndebug: ${JSON.stringify(
      generated.debug
    )}\n---\n\n${generated.message}`
  );
}
