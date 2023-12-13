import { Content } from "./content/content.js";
export { getPlugin } from "./plugin/index.js";
import { getEngine } from "./engine/index.js";
export { getEngine } from "./engine/index.js";
export { RAG } from "./plugin/rag.js";

export const DEFAULT_ENGINE = "openai";
export const DEFAULT_PLUGIN = "noop";

export type Thread = Content[];

export { updateDatabase } from "./actions/update_database.js";
export { GenerateManager } from "./actions/generate_manager.js";

export interface PipelineSettings {
  root: string;
  paths: string[];
  out: string;
  engine: string;
  model: string;
  plugin: string;
  overwrite: boolean;
}

export async function makePipelineSettings({
  root,
  paths = [],
  out = root,
  engine = DEFAULT_ENGINE,
  model,
  plugin = DEFAULT_PLUGIN,
  overwrite = true,
}: {
  root: string;
  paths?: string[];
  out?: string;
  engine?: string;
  model?: string;
  plugin?: string;
  overwrite?: boolean;
}): Promise<PipelineSettings> {
  model = model ?? (await getEngine(engine)).DEFAULT_MODEL;
  return {
    root,
    paths,
    out,
    engine,
    model,
    plugin,
    overwrite,
  };
}
