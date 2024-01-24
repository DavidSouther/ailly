import pkg from "./package.json" assert { type: "json" };
import * as contentModule from "./src/content/content.js";
import * as aillyModule from "./src/ailly.js";
import * as engineModule from "./src/engine/index.js";
import * as pluginModule from "./src/plugin/index.js";

export namespace types {
  export type Content = contentModule.Content;
  export type ContentMeta = contentModule.ContentMeta;
  export type Message = engineModule.Message;
  export type Summary = engineModule.Summary;
  export type Plugin = pluginModule.Plugin;
  export type PluginBuilder = pluginModule.PluginBuilder;
}

export const content = {
  load: contentModule.loadContent,
  write: contentModule.writeContent,
};

export const Ailly = aillyModule;

export const version = pkg.version;
