import type { Content } from "../../content/content.js";
import * as openai from "../openai.js";
import { spawn } from "node:child_process";
import { normalize, join, dirname } from "node:path";

const MODEL = "mistralai/Mistral-7B-v0.1";

export async function generate(
  c: Content,
  {}: {}
): Promise<{ message: string; debug: unknown }> {
  return new Promise<{ message: string; debug: unknown }>((resolve, reject) => {
    const prompt = c.meta?.messages?.map(({ content }) => content).join("\n");
    if (!prompt) {
      return reject("No messages in Content");
    }

    let cwd = dirname(
      import.meta.url
        .replace(/^file:/, "")
        .replace("ailly/core/dist", "ailly/core/src")
    );
    let command = join(cwd, normalize(".venv/bin/python3"));
    let args = [join(cwd, "mistral.py"), prompt];
    let child = spawn(command, args, { cwd });

    let response = "";
    child.on("message", (m) => (response += `${m}`));

    const done = () => {
      resolve({ message: response, debug: {} });
    };
    child.on("exit", done);
    child.on("close", done);
    child.on("disconnect", done);

    const error = (cause: unknown) =>
      reject(new Error("child_process had a problem", { cause }));
    child.on("error", error);
  });
}

export const format = openai.format;
export const getMessages = openai.getMessages;

export async function tune(
  content: Content[],
  context: Record<string, Content>,
  {
    model = MODEL,
    apiKey = process.env["OPENAI_API_KEY"] ?? "",
    baseURL = "http://localhost:8000/v1",
  }: { model: string; apiKey: string; baseURL: string }
) {
  return openai.tune(content, context, { model, apiKey, baseURL });
}
