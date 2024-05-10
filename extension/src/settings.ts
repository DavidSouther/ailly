import * as ailly from "@ailly/core";
import { DEFAULT_ENGINE, getEngine } from "@ailly/core/src/ailly";
import { ENGINES } from "@ailly/core/src/engine";
import { getLogger } from "@davidsouther/jiffies/lib/esm/log";
import vscode from "vscode";

export const LOGGER = getLogger("@ailly/extension");
const outputChannel = vscode.window.createOutputChannel("Ailly", {
  log: true,
});

function aillyLogFormatter<
  D extends {
    name: string;
    prefix: string;
    level: number;
    message: string;
    source: string;
  }
>(data: D) {
  let base = `${data.message}`;
  if (data.debug) {
    base += ` debug: ${JSON.stringify(data.debug)}`;
  }
  if (data.prompt) {
    base += ` prompt: ${JSON.stringify(data.prompt)}`;
  }
  if (data.messages) {
    base += ` prompt: \n${data.messages}`;
  }
  return base;
}

export function resetLogger() {
  let level = outputChannel.logLevel - 1;
  if (level < 0) level = 5;
  ailly.Ailly.LOGGER.level = LOGGER.level = level;
  ailly.Ailly.LOGGER.format = LOGGER.format = aillyLogFormatter;
  ailly.Ailly.LOGGER.console = LOGGER.console =
    outputChannel as unknown as Console;
}

export async function getOpenAIKey(): Promise<string | undefined> {
  if (process.env["OPENAI_API_KEY"]) {
    return process.env["OPENAI_API_KEY"];
  }
  let aillyConfig = getConfig();
  if (aillyConfig.has("openai-api-key")) {
    const key = aillyConfig.get<string>("openai-api-key");
    if (key) {
      return key;
    }
  }
  const key = await vscode.window.showInputBox({
    title: "Ailly: OpenAI API Key",
    prompt: "API Key from OpenAI for requests",
  });
  aillyConfig.update("openai-api-key", key);
  return key;
}

export async function getAillyEngine(): Promise<string> {
  const aillyConfig = getConfig();
  if (aillyConfig.has("engine")) {
    const engine = aillyConfig.get<string>("engine");
    if (engine) {
      return engine;
    }
  }
  const engine = await vscode.window.showQuickPick(Object.keys(ENGINES), {
    title: "Ailly: Engine",
  });
  aillyConfig.update("engine", engine);
  return engine ?? DEFAULT_ENGINE;
}

export async function getAillyModel(
  engineName: string
): Promise<string | undefined> {
  const engine = await getEngine(engineName);
  const aillyConfig = getConfig();
  if (aillyConfig.has("model")) {
    const model = aillyConfig.get<string>("model");
    if (model) {
      return model;
    }
  }
  const models = engine.models?.();
  if (!models) return;
  const model = await vscode.window.showQuickPick(models, {
    title: "Ailly: Model",
  });
  aillyConfig.update("model", model);
  return model;
}

export async function getAillyAwsProfile(): Promise<string> {
  const aillyConfig = getConfig();
  const awsProfile = aillyConfig.get<string>("aws-profile");
  return awsProfile ?? process.env["AWS_PROFILE"] ?? "default";
}

function getConfig() {
  return vscode.workspace.getConfiguration("ailly");
}
