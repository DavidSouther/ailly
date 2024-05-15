import { PipelineSettings } from "../index.js";
import type { Content, View } from "../content/content.js";
import type { Engine } from "../engine/index.js";
import { RAG } from "./rag.js";

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
  noop: { default: RAG.empty as unknown as PluginBuilder },
  none: { default: RAG.empty as unknown as PluginBuilder },
};

export async function getPlugin(
  name: keyof typeof PLUGINS | string
): Promise<{ default: PluginBuilder }> {
  if (name.startsWith("file://")) {
    return import(name);
  }
  if (!PLUGINS[name]) throw new Error(`Unknown plugin ${name}`);
  return PLUGINS[name];
}
