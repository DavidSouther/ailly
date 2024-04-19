import { Content, View, ContentMeta } from "./content/content.js";
export { GitignoreFs } from "./content/gitignore_fs.js";
export { getPlugin } from "./plugin/index.js";
import { getEngine } from "./engine/index.js";
export { getEngine } from "./engine/index.js";
export * from "./util.js";

export const DEFAULT_ENGINE = "bedrock";
export const DEFAULT_PLUGIN = "noop";

export type Thread = Content[];

export { GenerateManager } from "./actions/generate_manager.js";

export interface PipelineSettings {
  root: string;
  out: string;
  engine: string;
  model: string;
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
    context: context as NonNullable<ContentMeta["context"]>,
    plugin,
    overwrite,
    isolated,
    combined,
    templateView,
  };
}
