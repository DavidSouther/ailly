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
export const version = getVersion(import.meta.url);

// TODO move this to jiffies
import { execSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { normalize, join } from "node:path";
import { readFileSync } from "node:fs";

export function getVersion(root: /*ImportMeta.URL*/ string) {
  const cwd = normalize(join(fileURLToPath(root), ".."));
  const packageJson = join(cwd, "./package.json");
  const pkg = JSON.parse(readFileSync(packageJson, { encoding: "utf8" }));
  return pkg.version;
}

export function getRevision(root: /* ImportMeta.URL */ string) {
  const cwd = normalize(join(fileURLToPath(root), ".."));
  let sha = "";
  let status = "";
  let changes = "";
  try {
    // Are there any outstanding git changes?
    const run = (cmd: string) =>
      execSync(cmd, { encoding: "utf-8", cwd, stdio: "pipe" });
    sha = run("git rev-parse --short HEAD").trim();
    status = run("git status --porcelain=v1");
  } catch (e) {}
  if (sha.length > 0)
    changes = ` (${sha} ±${status.trim().split("\n").length})`;
  return changes;
}
