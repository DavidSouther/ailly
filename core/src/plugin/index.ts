import * as rag from "./rag.js";
import { PipelineSettings } from "../ailly.js";
import type { Content } from "../content/content";
import type { Engine } from "../engine";

export interface PluginBuilder {
  (engine: Engine, settings: PipelineSettings): Promise<Plugin>;
}

export interface Plugin {
  augment(c: Content): Promise<void>;
  clean(c: Content): Promise<void>;
}

export const PLUGINS: Record<string, PluginBuilder> = {
  noop: rag.RAG.empty as unknown as PluginBuilder,
  none: rag.RAG.empty as unknown as PluginBuilder,
  rag: rag.RAG.build as unknown as PluginBuilder,
};

export function getPlugin(name: string): PluginBuilder {
  if (name.startsWith("file://")) {
    return require(name.replace(/^file:\/\//, ""));
  }
  if (!PLUGINS[name]) throw new Error(`Unknown plugin ${name}`);
  return PLUGINS[name];
}
