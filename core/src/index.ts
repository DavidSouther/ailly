import * as contentModule from "./content";
import * as aillyModule from "./ailly";
import * as pluginModule from "./plugin";

export namespace types {
  export type Content = contentModule.Content;
  export type ContentMeta = contentModule.ContentMeta;
  export type Message = pluginModule.Message;
  export type Summary = pluginModule.Summary;
}

export const content = {
  load: contentModule.loadContent,
};

export const Ailly = aillyModule;
