import * as ailly from "@ailly/core";
import { DEFAULT_ENGINE, getEngine } from "@ailly/core/src/ailly";
import { ENGINES } from "@ailly/core/src/engine";
import {
  basicLogFormatter,
  getLogLevel,
  getLogger,
} from "@davidsouther/jiffies/lib/esm/log";
import vscode from "vscode";

export const LOGGER = getLogger("@ailly/extension");

export function resetLogger() {
  ailly.Ailly.LOGGER.level = LOGGER.level = getLogLevel(getAillyLogLevel());
  ailly.Ailly.LOGGER.format = LOGGER.format = getAillyLogPretty()
    ? basicLogFormatter
    : JSON.stringify;
}

export function getAillyLogLevel() {
  return getConfig().get<string>("log-level") ?? "info";
}

export function getAillyLogPretty() {
  return getConfig().get<boolean>("log-pretty") ?? false;
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
