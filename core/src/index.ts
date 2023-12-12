import * as contentModule from "./content/content.js";
import * as aillyModule from "./ailly.js";
import * as pluginModule from "./plugin/index.js";

export namespace types {
  export type Content = contentModule.Content;
  export type ContentMeta = contentModule.ContentMeta;
  export type Message = pluginModule.Message;
  export type Summary = pluginModule.Summary;
}

export const content = {
  load: contentModule.loadContent,
  write: contentModule.writeContent,
};

export const Ailly = aillyModule;
