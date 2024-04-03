import * as rag from "./rag.js";
import { PipelineSettings } from "../ailly.js";
import type { Content, View } from "../content/content.js";
import type { Engine } from "../engine/index.js";

export interface PluginBuilder {
  (engine: Engine, settings: PipelineSettings): Promise<Plugin>;
}

export interface Plugin {
  augment(c: Content): Promise<void>;
  clean(c: Content): Promise<void>;
  update(c: Content[]): Promise<void>;
  view?(): Promise<View>;
}

export const PLUGINS: Record<string, { default: PluginBuilder }> = {
  noop: { default: rag.RAG.empty as unknown as PluginBuilder },
  none: { default: rag.RAG.empty as unknown as PluginBuilder },
  rag: { default: rag.RAG.build as unknown as PluginBuilder },
};

export async function getPlugin(
  name: string
): Promise<{ default: PluginBuilder }> {
  if (name.startsWith("file://")) {
    return import(name);
  }
  if (!PLUGINS[name]) throw new Error(`Unknown plugin ${name}`);
  return PLUGINS[name];
}
