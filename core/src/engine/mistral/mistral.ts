import { EngineGenerate } from "..";
import type { Content } from "../../content/content.js";
import * as openai from "../openai.js";
import { spawn } from "node:child_process";
import { normalize, join, dirname } from "node:path";

const DEFAULT_MODEL = "mistralai/Mistral-7B-v0.1";
interface MistralDebug {}

export const generate: EngineGenerate<MistralDebug> = async (
  c: Content,
  { model = DEFAULT_MODEL }: { model?: string }
) => {
  const prompt = c.meta?.messages?.map(({ content }) => content).join("\n");
  if (!prompt) {
    throw new Error("No messages in Content");
  }

  let cwd = dirname(
    (import.meta?.url.replace(/^file:/, "") ?? __filename).replace(
      "ailly/core/dist",
      "ailly/core/src"
    )
  );
  let command = join(cwd, normalize(".venv/bin/python3"));
  let args = [join(cwd, "mistral.py"), prompt];
  let child = spawn(command, args, { cwd });

  const stream = new TransformStream();

  let message = "";
  child.on("message", async (m) => {
    const writer = await stream.writable.getWriter();
    await writer.ready;
    await writer.write(m);
    writer.releaseLock();
    message += `${m}`;
  });

  const done = () => {
    stream.writable.close();
  };
  child.on("exit", done);
  child.on("close", done);
  child.on("disconnect", done);

  const error = (cause: unknown) => {
    stream.writable.abort(
      `child_process had a problem ${JSON.stringify(cause)}`
    );
  };
  child.on("error", error);

  return {
    stream: stream.readable,
    message: () => message,
    debug: () => ({}),
  };
};

export const format = openai.format;
export const getMessages = openai.getMessages;

export async function tune(
  content: Content[],
  context: Record<string, Content>,
  {
    model = DEFAULT_MODEL,
    apiKey = process.env["OPENAI_API_KEY"] ?? "",
    baseURL = "http://localhost:8000/v1",
  }: { model: string; apiKey: string; baseURL: string }
) {
  return openai.tune(content, context, { model, apiKey, baseURL });
}
