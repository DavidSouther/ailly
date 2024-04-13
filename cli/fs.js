import { NodeFileSystemAdapter } from "@davidsouther/jiffies/lib/esm/fs_node.js";
import { DEFAULT_LOGGER, LEVEL, error } from "@davidsouther/jiffies/lib/esm/log.js";
import { dirname, resolve } from "node:path";
import { parse } from "yaml";
// import * as yaml from "yaml";
import * as ailly from "@ailly/core";

/**
 * @param {ReturnType<import("./args.js").makeArgs>} args
 * @returns {Promise<{
 *   fs: import("@davidsouther/jiffies/lib/esm/fs").FileSystem,
 *   content: ailly.types.Content[],
 *   settings: import("@ailly/core/dist/src/ailly").PipelineSettings
 * }>}
 */
export async function loadFs(args) {
  const root = resolve(args.values.root ?? '.');
  const fs = new ailly.Ailly.GitignoreFs(new NodeFileSystemAdapter());
  fs.cd(root);

  const settings = await ailly.Ailly.makePipelineSettings({
    root,
    out: resolve(args.values.out ?? root),
    isolated: args.values.isolated,
    engine: args.values.engine,
    model: args.values.model,
    plugin: args.values.plugin,
    templateView: await loadTemplateView(fs, args.values['template-view']),
    overwrite: !args.values["no-overwrite"],
  });
  const positionals = args.positionals.slice(2).map(a => resolve(a));
  const isPipe = positionals.length == 0 && args.values.prompt;
  DEFAULT_LOGGER.level = getLogLevel(args.values['log-level'], args.values.verbose, isPipe);

  let content = await ailly.content.load(
    fs,
    args.values.prompt ? [{ content: args.values.prompt, view: {} }] : [],
    settings
  );

  if (isPipe) {
    content.forEach(c => { c.meta = c.meta ?? {}; c.meta.skip = true; });
    const cliContent = {
      name: 'stdout',
      outPath: "/dev/stdout",
      path: "/dev/stdout",
      prompt: args.values.prompt ?? "",
      predecessor: content.filter(c => dirname(c.path) == root).at(-1),
      view: settings.templateView,
    };
    content.push(cliContent)
  } else {
    if (positionals.length == 0) positionals.push(root);
    content = content.filter((c) =>
      positionals.some((p) => c.path.startsWith(p))
    );
  }

  return { fs, settings, content };
}

/**
 * @typedef {import("@ailly/core/dist/src/content/content").View} View
 * @typedef {import("@davidsouther/jiffies/lib/esm/fs").FileSystem} FileSystem
 */

/**
 * Read, parse, and validate a template view.
 * 
 * @param {FileSystem} fs 
 * @param {string|undefined} path
 * @returns {Promise<View>}
 */
async function loadTemplateView(fs, path) {
  if (path == undefined) return {};
  try {
    const file = await fs.readFile(path);
    const view = parse(file);
    if (view && typeof view == "object") {
      return /** @type View */ (/** @type unknown */ view);
    }
  } catch (e) {
    console.warn(`Failed to load template-view ${path}`, e)
  }
  return {};
}

/**
 * @param {string|undefined} level
 * @param {boolean} verbose
 * @param {boolean} isPipe
 * @returns {number}
 */
function getLogLevel(level, verbose, isPipe) {
  if (level) {
    switch (level) {
      case "debug": LEVEL.DEBUG;
      case "info": LEVEL.INFO;
      case "warn": LEVEL.WARN;
      case "error": LEVEL.ERROR;
      default:
        if (!isNaN(+level)) return Number(level);
    }
  }
  if (verbose) {
    return LEVEL.INFO;
  }
  return isPipe ? LEVEL.SILENT : LEVEL.INFO;
}