import { getLogLevel, getLogger } from "@davidsouther/jiffies/lib/cjs/log.js";
import { Content, ContentMeta, View } from "./content/content.js";
import { getEngine } from "./engine/index.js";
import { getVersion } from "./version.js";
export { GitignoreFs } from "./content/gitignore_fs.js";
export { getPlugin } from "./plugin/index.js";

export const LOGGER = getLogger("@ailly/core");
LOGGER.level = getLogLevel(process.env["AILLY_LOG_LEVEL"]);

let dirname;
try {
  dirname = __dirname;
} catch (e) {}
export const version = dirname ? getVersion(dirname) : "(unknown)";

export const DEFAULT_ENGINE = "bedrock";
export const DEFAULT_PLUGIN = "noop";

export type Thread = Content[];

export const DEFAULT_SCHEDULER_LIMIT = 5;

export interface PipelineSettings {
  root: string;
  out: string;
  engine: string;
  model: string;
  requestLimit: number;
  context: NonNullable<ContentMeta["context"]>;
  plugin: string;
  isolated: boolean;
  combined: boolean | undefined;
  overwrite: boolean;
  templateView: View;
}

export async function makePipelineSettings({
  root,
  out = root,
  engine = DEFAULT_ENGINE,
  model,
  requestLimit = DEFAULT_SCHEDULER_LIMIT,
  context = "conversation",
  plugin = DEFAULT_PLUGIN,
  overwrite = true,
  isolated = false,
  combined,
  templateView = {},
}: {
  root: string;
  out?: string;
  engine?: string;
  model?: string;
  context?: string;
  plugin?: string;
  overwrite?: boolean;
  isolated?: boolean;
  combined?: boolean;
  requestLimit?: number;
  templateView?: View;
}): Promise<PipelineSettings> {
  model = model ?? (await getEngine(engine)).DEFAULT_MODEL;
  context = ["conversation", "folder", "none"].includes(context)
    ? context
    : "conversation";
  return {
    root,
    out,
    engine,
    model,
    requestLimit,
    context: context as NonNullable<ContentMeta["context"]>,
    plugin,
    overwrite,
    isolated,
    combined,
    templateView,
  };
}