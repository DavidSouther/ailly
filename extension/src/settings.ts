import {
  DEFAULT_ENGINE,
  LOGGER as ROOT_LOGGER,
} from "@ailly/core/lib/ailly.js";
import { ENGINES, getEngine } from "@ailly/core/lib/engine/index.js";
import * as vscode from "vscode";
const { getLogger } = require("@davidsouther/jiffies/lib/cjs/log.js");

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
    err?: Error;
  }
>(data: D) {
  let base = `${data.name} ${data.message}`;
  if (data.err) {
    base += ` err: ${data.err.message}${
      data.err.cause
        ? " " + String((data.err.cause as { message?: string }).message)
        : ""
    }`;
  } else {
    const debug: Partial<D> = { ...data };
    delete debug.name;
    delete debug.message;
    delete debug.prefix;
    delete debug.level;
    delete debug.source;
    if (Object.keys(debug).length > 0) {
      base += " " + JSON.stringify(debug);
    }
  }
  return base;
}

export function resetLogger() {
  let level = outputChannel.logLevel - 1;
  if (level < 0) {
    level = 5;
  }
  ROOT_LOGGER.level = LOGGER.level = level;
  ROOT_LOGGER.format = LOGGER.format = aillyLogFormatter;
  ROOT_LOGGER.console = LOGGER.console = outputChannel as unknown as Console;
}

export const SETTINGS = {
  openApiKey: "openaiApiKey",
  engine: "engine",
  model: "model",
  awsProfile: "awsProfile",
  awsRegion: "awsRegion",
  preferStreamingEdit: "preferStreamingEdit",
};

export const Settings = {
  async getOpenAIKey(): Promise<string | undefined> {
    if (process.env["OPENAI_API_KEY"]) {
      return process.env["OPENAI_API_KEY"];
    }
    let aillyConfig = Settings.getConfig();
    if (aillyConfig.has(SETTINGS.openApiKey)) {
      const key = aillyConfig.get<string>(SETTINGS.openApiKey);
      if (key) {
        return key;
      }
    }
    const key = await vscode.window.showInputBox({
      title: "Ailly: OpenAI API Key",
      prompt: "API Key from OpenAI for requests",
    });
    aillyConfig.update(SETTINGS.openApiKey, key);
    return key;
  },

  async getAillyEngine(): Promise<string> {
    const aillyConfig = Settings.getConfig();
    if (aillyConfig.has(SETTINGS.engine)) {
      const engine = aillyConfig.get<string>(SETTINGS.engine);
      if (engine) {
        return engine;
      }
    }
    const engine = await vscode.window.showQuickPick(Object.keys(ENGINES), {
      title: "Ailly: Engine",
    });
    aillyConfig.update(SETTINGS.engine, engine);
    return engine ?? DEFAULT_ENGINE;
  },

  async getAillyModel(engineName: string): Promise<string | undefined> {
    const engine = await getEngine(engineName);
    const aillyConfig = Settings.getConfig();
    if (aillyConfig.has(SETTINGS.model)) {
      const model = aillyConfig.get<string>(SETTINGS.model);
      if (model) {
        return model;
      }
    }
    const models = engine.models?.();
    if (!models) {
      return;
    }
    const model = await vscode.window.showQuickPick(models, {
      title: "Ailly: Model",
    });
    aillyConfig.update(SETTINGS.model, model);
    return model;
  },

  async getAillyAwsProfile(): Promise<string> {
    const aillyConfig = Settings.getConfig();
    const awsProfile = aillyConfig.get<string>(SETTINGS.awsProfile);
    return awsProfile ?? process.env["AWS_PROFILE"] ?? "default";
  },

  async getAillyAwsRegion(): Promise<string | undefined> {
    const aillyConfig = Settings.getConfig();
    const awsProfile = aillyConfig.get<string>(SETTINGS.awsRegion);
    return awsProfile ?? (process.env["AWS_REGION"] || undefined);
  },

  async prepareBedrock() {
    process.env["AWS_PROFILE"] = await Settings.getAillyAwsProfile();
    const region = await Settings.getAillyAwsRegion();
    if (region !== undefined) {
      process.env["AWS_REGION"] = region;
    }
  },

  getPreferStreamingEdit(): boolean {
    const aillyConfig = Settings.getConfig();
    const preferStreamingEdit = aillyConfig.get<boolean>(
      SETTINGS.preferStreamingEdit
    );
    return preferStreamingEdit ?? true;
  },

  getConfig() {
    return vscode.workspace.getConfiguration("ailly");
  },
};
