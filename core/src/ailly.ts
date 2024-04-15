import { Content, View } from "./content/content.js";
export { GitignoreFs } from "./content/gitignore_fs.js";
export { getPlugin } from "./plugin/index.js";
import { getEngine } from "./engine/index.js";
export { getEngine } from "./engine/index.js";
export { RAG } from "./plugin/rag.js";

export const DEFAULT_ENGINE = "bedrock";
export const DEFAULT_PLUGIN = "noop";

export type Thread = Content[];

export { GenerateManager } from "./actions/generate_manager.js";

export interface PipelineSettings {
  root: string;
  out: string;
  engine: string;
  model: string;
  context: "content" | "folder" | "none";
  plugin: string;
  isolated: boolean;
  overwrite: boolean;
  templateView: View;
}

export async function makePipelineSettings({
  root,
  out = root,
  engine = DEFAULT_ENGINE,
  model,
  context = "content",
  plugin = DEFAULT_PLUGIN,
  overwrite = true,
  isolated = false,
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
  templateView?: View;
}): Promise<PipelineSettings> {
  model = model ?? (await getEngine(engine)).DEFAULT_MODEL;
  context = ["content", "folder", "none"].includes(context)
    ? context
    : "content";
  return {
    root,
    out,
    engine,
    model,
    context: context as "content" | "folder" | "none",
    plugin,
    overwrite,
    isolated,
    templateView,
  };
}
