import { ENGINES, getEngine } from "@ailly/core/lib/engine/index.js";
import { DEFAULT_ENGINE } from "@ailly/core/lib/index";
import { LOGGER as ROOT_LOGGER } from "@ailly/core/lib/index.js";
import { getLogger } from "@davidsouther/jiffies/lib/cjs/log.js";
import * as vscode from "vscode";

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
  },
>(data: D) {
  let base = `${data.name} ${data.message}`;
  if (data.err) {
    base += ` err: ${data.err.message}${
      data.err.cause ? ` ${(data.err.cause as Error).message}` : ""
    }`;
  }
  const debug: Partial<D> = { ...data };
  debug.name = undefined;
  debug.message = undefined;
  debug.prefix = undefined;
  debug.level = undefined;
  if (Object.keys(debug).length > 0) {
    base += ` ${JSON.stringify(debug)}`;
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

const SETTINGS_KEYS = {
  openApiKey: "openaiApiKey",
  engine: "engine",
  model: "model",
  awsProfile: "awsProfile",
  awsRegion: "awsRegion",
  preferStreamingEdit: "preferStreamingEdit",
  templateViews: "templateViews",
};
export const SETTINGS = {
  async getOpenAIKey(): Promise<string | undefined> {
    if (process.env.OPENAI_API_KEY) {
      return process.env.OPENAI_API_KEY;
    }
    const aillyConfig = getConfig();
    if (aillyConfig.has(SETTINGS_KEYS.openApiKey)) {
      const key = aillyConfig.get<string>(SETTINGS_KEYS.openApiKey);
      if (key) {
        return key;
      }
    }
    const key = await vscode.window.showInputBox({
      title: "Ailly: OpenAI API Key",
      prompt: "API Key from OpenAI for requests",
    });
    aillyConfig.update(SETTINGS_KEYS.openApiKey, key);
    return key;
  },

  async getAillyEngine(): Promise<string> {
    const aillyConfig = getConfig();
    if (aillyConfig.has(SETTINGS_KEYS.engine)) {
      const engine = aillyConfig.get<string>(SETTINGS_KEYS.engine);
      if (engine) {
        return engine;
      }
    }
    const engine = await vscode.window.showQuickPick(Object.keys(ENGINES), {
      title: "Ailly: Engine",
    });
    aillyConfig.update(SETTINGS_KEYS.engine, engine);
    return engine ?? DEFAULT_ENGINE;
  },

  async getAillyModel(engineName: string): Promise<string | undefined> {
    const engine = await getEngine(engineName);
    const aillyConfig = getConfig();
    if (aillyConfig.has(SETTINGS_KEYS.model)) {
      const model = aillyConfig.get<string>(SETTINGS_KEYS.model);
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
    aillyConfig.update(SETTINGS_KEYS.model, model);
    return model;
  },

  async getAillyAwsProfile(): Promise<string> {
    const aillyConfig = getConfig();
    const awsProfile = aillyConfig.get<string>(SETTINGS_KEYS.awsProfile);
    return awsProfile ?? process.env.AWS_PROFILE ?? "default";
  },

  async getAillyAwsRegion(): Promise<string | undefined> {
    const aillyConfig = getConfig();
    const awsProfile = aillyConfig.get<string>(SETTINGS_KEYS.awsRegion);
    return awsProfile ?? (process.env.AWS_REGION || undefined);
  },

  async prepareBedrock() {
    process.env.AWS_PROFILE = await SETTINGS.getAillyAwsProfile();
    const region = await SETTINGS.getAillyAwsRegion();
    if (region !== undefined) {
      process.env.AWS_REGION = region;
    }
  },

  getPreferStreamingEdit(): boolean {
    const aillyConfig = getConfig();
    const preferStreamingEdit = aillyConfig.get<boolean>(
      SETTINGS_KEYS.preferStreamingEdit,
    );
    return preferStreamingEdit ?? true;
  },

  getTemplateViews(): string[] {
    const aillyConfig = getConfig();
    const aillyTemplateViews = aillyConfig.get<string[]>(
      SETTINGS_KEYS.templateViews,
    );
    return aillyTemplateViews ?? [];
  },

  /** The duration in ms to wait after completing all threads to hide the Ailly status bar. */
  getAillyStatusBarHideDelay(): number {
    return 2000;
  },
};

function getConfig() {
  return vscode.workspace.getConfiguration("ailly");
}
