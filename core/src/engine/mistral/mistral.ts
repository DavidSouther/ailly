import { spawn } from "node:child_process";
import { dirname, join, normalize } from "node:path";
import type { Content } from "../../content/content.js";
import { type PipelineSettings } from "../../index.js";
import { type EngineDebug, type EngineGenerate } from "../index.js";
import * as openai from "../openai.js";
import { withResolvers } from "../../util";

const DEFAULT_MODEL = "mistralai/Mistral-7B-v0.1";
interface MistralDebug extends EngineDebug {}

export const generate: EngineGenerate<MistralDebug> = (
  c: Content,
  _: PipelineSettings
) => {
  const prompt = c.meta?.messages?.map(({ content }) => content).join("\n");
  if (!prompt) {
    throw new Error("No messages in Content");
  }

  const cwd = getMistralDirectory();
  const command = join(cwd, normalize(".venv/bin/python3"));
  const args = [join(cwd, "mistral.py"), prompt];
  const child = spawn(command, args, { cwd });

  const stream = new TransformStream();
  const done = withResolvers<void>();

  let message = "";
  child.on("message", async (m) => {
    const writer = await stream.writable.getWriter();
    await writer.ready;
    await writer.write(m);
    writer.releaseLock();
    message += `${m}`;
  });

  const onDone = () => {
    stream.writable.close();
    done.resolve();
  };
  child.on("exit", onDone);
  child.on("close", onDone);
  child.on("disconnect", onDone);

  const onError = (cause: unknown) => {
    stream.writable.abort(
      `child_process had a problem ${JSON.stringify(cause)}`
    );
    done.reject(cause);
  };
  child.on("error", onError);

  return {
    stream: stream.readable,
    message: () => message,
    debug: () => ({}),
    done: done.promise,
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

function getMistralDirectory() {
  if (process.env["AILLY_MISTRAL_ROOT"]) {
    return process.env["AILLY_MISTRAL_ROOT"];
  }
  let filename;
  try {
    filename = __filename;
  } catch (e) {}
  if (!filename) {
    throw new Error("Could not determine Mistral root");
  }

  return dirname(filename.replace("ailly/core/dist", "ailly/core/src"));
}
