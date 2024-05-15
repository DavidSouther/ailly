import { ENGINES, getEngine } from "@ailly/core/src/engine";
import {
  DEFAULT_ENGINE,
  LOGGER as ROOT_LOGGER,
} from "@ailly/core/src/index.js";
import { getLogger } from "@davidsouther/jiffies/lib/cjs/log.js";
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
  let base = `${data.name} ${data.message}`;
  if (data.err) {
    base += ` err: ${JSON.stringify(data.err)}`;
  }
  if (data.debug) {
    base += ` debug: ${JSON.stringify(data.debug)}`;
  }
  if (data.prompt) {
    base += ` prompt: ${JSON.stringify(data.prompt)}`;
  }
  if (data.messages) {
    base += ` prompt: \n${JSON.stringify(data.messages)}`;
  }
  return base;
}

export function resetLogger() {
  let level = outputChannel.logLevel - 1;
  if (level < 0) level = 5;
  ROOT_LOGGER.level = LOGGER.level = level;
  ROOT_LOGGER.format = LOGGER.format = aillyLogFormatter;
  ROOT_LOGGER.console = LOGGER.console = outputChannel as unknown as Console;
}

const SETTINGS = {
  OPENAI_API_KEY: "openaiApiKey",
  ENGINE: "engine",
  MODEL: "model",
  AWS_PROFILE: "awsProfile",
  AWS_REGION: "awsRegion",
};
export async function getOpenAIKey(): Promise<string | undefined> {
  if (process.env["OPENAI_API_KEY"]) {
    return process.env["OPENAI_API_KEY"];
  }
  let aillyConfig = getConfig();
  if (aillyConfig.has(SETTINGS.OPENAI_API_KEY)) {
    const key = aillyConfig.get<string>(SETTINGS.OPENAI_API_KEY);
    if (key) {
      return key;
    }
  }
  const key = await vscode.window.showInputBox({
    title: "Ailly: OpenAI API Key",
    prompt: "API Key from OpenAI for requests",
  });
  aillyConfig.update(SETTINGS.OPENAI_API_KEY, key);
  return key;
}

export async function getAillyEngine(): Promise<string> {
  const aillyConfig = getConfig();
  if (aillyConfig.has(SETTINGS.ENGINE)) {
    const engine = aillyConfig.get<string>(SETTINGS.ENGINE);
    if (engine) {
      return engine;
    }
  }
  const engine = await vscode.window.showQuickPick(Object.keys(ENGINES), {
    title: "Ailly: Engine",
  });
  aillyConfig.update(SETTINGS.ENGINE, engine);
  return engine ?? DEFAULT_ENGINE;
}

export async function getAillyModel(
  engineName: string
): Promise<string | undefined> {
  const engine = await getEngine(engineName);
  const aillyConfig = getConfig();
  if (aillyConfig.has(SETTINGS.MODEL)) {
    const model = aillyConfig.get<string>(SETTINGS.MODEL);
    if (model) {
      return model;
    }
  }
  const models = engine.models?.();
  if (!models) return;
  const model = await vscode.window.showQuickPick(models, {
    title: "Ailly: Model",
  });
  aillyConfig.update(SETTINGS.MODEL, model);
  return model;
}

export async function getAillyAwsProfile(): Promise<string> {
  const aillyConfig = getConfig();
  const awsProfile = aillyConfig.get<string>(SETTINGS.AWS_PROFILE);
  return awsProfile ?? process.env["AWS_PROFILE"] ?? "default";
}

export async function getAillyAwsRegion(): Promise<string | undefined> {
  const aillyConfig = getConfig();
  const awsProfile = aillyConfig.get<string>(SETTINGS.AWS_REGION);
  return awsProfile ?? (process.env["AWS_REGION"] || undefined);
}

export async function prepareBedrock() {
  process.env["AWS_PROFILE"] = await getAillyAwsProfile();
  const region = await getAillyAwsRegion();
  if (region != undefined) {
    process.env["AWS_REGION"] = region;
  }
}

function getConfig() {
  return vscode.workspace.getConfiguration("ailly");
}
