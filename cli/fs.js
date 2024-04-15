import { DEFAULT_LOGGER, LEVEL } from "@davidsouther/jiffies/lib/esm/log.js";
import { assertExists } from "@davidsouther/jiffies/lib/esm/assert.js";
import { dirname, resolve, join } from "node:path";
import { parse } from "yaml";
import * as ailly from "@ailly/core";

/** @typedef {ReturnType<import("./args.js").makeArgs>} Args */
/** @typedef {import("@ailly/core").types.Content} Content */
/** @typedef {{start: number, end: number, file: string}|{after: number, file: string}} Edit */
/** @typedef {import("@ailly/core/dist/src/ailly").PipelineSettings} PipelineSettings */
/** @typedef {import("@ailly/core/dist/src/content/content").View} View */
/** @typedef {import("@davidsouther/jiffies/lib/esm/fs").FileSystem} FileSystem */

/**
 * @param {import("@davidsouther/jiffies/lib/esm/fs").FileSystem} fs 
 * @param {Args} args
 * @returns {Promise<{
 *   context: Record<string, ailly.types.Content>,
 *   content: string[],
 *   settings: PipelineSettings
 * }>}
 */
export async function loadFs(fs, args) {
  const root = resolve(args.values.root ?? '.');
  fs.cd(root);

  const settings = await ailly.Ailly.makePipelineSettings({
    root,
    out: resolve(args.values.out ?? root),
    context: args.values.context,
    isolated: args.values.isolated,
    combined: args.values.combined,
    engine: args.values.engine,
    model: args.values.model,
    plugin: args.values.plugin,
    templateView: await loadTemplateView(fs, args.values['template-view']),
    overwrite: !args.values["no-overwrite"],
  });

  const positionals = args.positionals.slice(2).map(a => resolve(join(root, a)));
  const hasPositionals = positionals.length > 0;
  const hasPrompt = args.values.prompt !== undefined && args.values.prompt !== "";
  const isPipe = !hasPositionals && hasPrompt;
  DEFAULT_LOGGER.level = getLogLevel(args.values['log-level'], args.values.verbose ?? false, isPipe);

  const system = args.values.system ?? "";

  let context = await ailly.content.load(
    fs,
    system ? [{ content: system, view: {} }] : [],
    settings
  );

  let content = /* @type {string[]} */[];
  if (!hasPositionals && hasPrompt) {
    Object.values(context).forEach(c => { c.meta = c.meta ?? {}; c.meta.skip = true; });
  } else {
    if (!hasPositionals) positionals.push(root);
    content = Object.keys(context).filter((c) =>
      positionals.some((p) => c.startsWith(p))
    );
  }

  let edit = args.values.edit ? makeEdit(args.values.lines, content, hasPrompt) : undefined;
  if (hasPrompt) {
    if (edit && content.length == 1) {
      content = [];
    }
    const prompt = assertExists(args.values.prompt);
    const cliContent = makeCLIContent(prompt, settings.context, system, context, root, edit, content, settings.templateView);
    context[cliContent.path] = cliContent;
    content.push(cliContent.path);
  }

  return { settings, content, context };
}

/**
 * @param {string|undefined} lines
 * @param {string[]} content 
 * @param {boolean} hasPrompt 
 * @returns Edit;
 */
export function makeEdit(lines, content, hasPrompt) {
  if (!lines) return undefined;
  if (content.length != 1) {
    throw new Error("Edit requires exactly 1 path");
  }
  if (!hasPrompt) {
    throw new Error("Edit requires a prompt to know what to change");
  }
  const file = content[0];
  const line = lines.split(':') ?? [];
  const hasStart = Boolean(line[0]);
  const hasEnd = Boolean(line[1]);
  const start = Number(line[0]) - 1;
  const end = Number(line[1]) - 1;
  switch (true) {
    case hasStart && hasEnd:
      return { start, end, file };
    case hasStart:
      return { after: start, file };
    case hasEnd:
      return { after: end - 1, file };
    default:
      throw new Error("Edit lines have at least one of start or end");
  }
}

/**
 * Create a "synthetic" Content block with path "/dev/stdout" to serve as the Content root
 * for this Ailly call to the LLM.
 * 
 * @param {string} prompt 
 * @param {'none'|'folder'|'content'} argContext
 * @param {string} argSystem 
 * @param {Record<string, Content>} context 
 * @param {string} root 
 * @param {Edit|undefined} edit 
 * @param {string[]} content 
 * @param {*} view 
 * @returns Content
 */
export function makeCLIContent(prompt, argContext, argSystem, context, root, edit, content, view) {
  // When argContext is folder, `folder` is all files in context in root.
  const folder = argContext == 'folder' ? Object.keys(context).filter(c => dirname(c) == root) : undefined;
  // When argContext is `content`, `predecessor` is the last item in the root folder.
  const predecessor = argContext == 'content' ? content.filter(c => dirname(c) == root).at(-1) : undefined;
  // When argContext is none, system is empty; otherwise, system is argSystem + predecessor's system.
  const system = argContext == "none" ? [] : [{ content: argSystem ?? "", view: {} }, ...((predecessor ? context[predecessor].context.system : undefined) ?? [])];
  const cliContent = {
    name: 'stdout',
    outPath: "/dev/stdout",
    path: "/dev/stdout",
    prompt: prompt ?? "",
    context: {
      view,
      predecessor,
      system,
      folder,
      edit,
    }
  };
  return cliContent;
}

/**
 * Read, parse, and validate a template view.
 * 
 * @param {FileSystem} fs 
 * @param {string|undefined} path
 * @returns {Promise<View>}
 */
export async function loadTemplateView(fs, path) {
  if (!path) return {};
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
export function getLogLevel(level, verbose, isPipe) {
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