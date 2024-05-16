import {
  AillyEdit,
  Content,
  View,
  loadContent,
  makeCLIContent,
} from "@ailly/core/lib/content/content.js";
import {
  PipelineSettings,
  LOGGER as ROOT_LOGGER,
  makePipelineSettings,
} from "@ailly/core/lib/index.js";
import { assertExists } from "@davidsouther/jiffies/lib/cjs/assert.js";
import { FileSystem } from "@davidsouther/jiffies/lib/cjs/fs.js";
import {
  basicLogFormatter,
  getLogLevel,
  getLogger,
} from "@davidsouther/jiffies/lib/cjs/log.js";
import { Console } from "node:console";
import { join, resolve } from "node:path";
import { parse } from "yaml";
import { Args } from "./args.js";

export const LOGGER = getLogger("@ailly/cli");
export async function loadFs(
  fs: FileSystem,
  args: Args
): Promise<{
  context: Record<string, Content>;
  content: string[];
  settings: PipelineSettings;
}> {
  const root = resolve(args.values.root ?? ".");
  fs.cd(root);

  const settings = await makePipelineSettings({
    root,
    out: resolve(args.values.out ?? root),
    context: args.values.context ?? (args.values.edit ? "folder" : undefined),
    isolated: args.values.isolated,
    combined: args.values.combined,
    engine: args.values.engine,
    model: args.values.model,
    plugin: args.values.plugin,
    templateView: await loadTemplateView(fs, args.values["template-view"]),
    overwrite: !args.values["no-overwrite"],
    requestLimit:
      args.values["request-limit"] ?? args.values.model?.includes("opus")
        ? 1
        : undefined,
  });

  const positionals = args.positionals.map((a) => resolve(join(root, a)));
  const hasPositionals = positionals.length > 0;
  const hasPrompt =
    args.values.prompt !== undefined && args.values.prompt !== "";
  const isPipe = !hasPositionals && hasPrompt;
  const logLevel =
    args.values["log-level"] ??
    (args.values.verbose ? "verbose" : isPipe ? "silent" : "info");
  const logFormat = args.values["log-format"];
  const formatter = logFormat == "json" ? JSON.stringify : basicLogFormatter;
  ROOT_LOGGER.console = LOGGER.console = isPipe
    ? new Console(process.stderr, process.stderr)
    : global.console;
  ROOT_LOGGER.level = LOGGER.level = getLogLevel(logLevel);
  ROOT_LOGGER.format = LOGGER.format = formatter;

  const system = args.values.system ?? "";
  const depth = Number(args.values["max-depth"]);

  let context = await loadContent(
    fs,
    system ? [{ content: system, view: {} }] : [],
    settings,
    depth
  );

  let content: string[] = [];
  if (!hasPositionals && hasPrompt) {
    Object.values(context).forEach((c) => {
      c.meta = c.meta ?? {};
      c.meta.skip = true;
    });
  } else {
    if (!hasPositionals) positionals.push(root);
    content = Object.keys(context).filter((c) =>
      positionals.some((p) => c.startsWith(p))
    );
  }

  let edit = args.values.edit
    ? makeEdit(args.values.lines, content, hasPrompt)
    : undefined;
  if (hasPrompt) {
    if (edit && content.length == 1) {
      content = [];
    }
    const prompt = await readPrompt(args);
    const cliContent = makeCLIContent(
      prompt,
      settings.context,
      system,
      context,
      root,
      edit,
      settings.templateView,
      settings.isolated
    );
    context[cliContent.path] = cliContent;
    content.push(cliContent.path);
  }

  return { settings, content, context };
}

function readPrompt(args: Args): Promise<string> {
  const prompt = assertExists(args.values.prompt);
  if (prompt === "-") {
    // Treat `-` as "read from stdin"
    try {
      return readAll(process.stdin);
    } catch (err) {
      LOGGER.warn("Failed to read stdin", { e: err });
      return Promise.resolve("");
    }
  }
  return Promise.resolve(prompt);
}

export function makeEdit(
  lines: string | undefined,
  content: string[],
  hasPrompt: boolean
): AillyEdit | undefined {
  if (!lines) return undefined;
  if (content.length != 1) {
    throw new Error("Edit requires exactly 1 path");
  }
  if (!hasPrompt) {
    throw new Error("Edit requires a prompt to know what to change");
  }
  const file = content[0];
  const line = lines.split(":") ?? [];
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
 * Read, parse, and validate a template view.
 */
export async function loadTemplateView(
  fs: FileSystem,
  paths?: string[]
): Promise<View> {
  if (!paths) return {};
  let view = /* @type View */ {};
  for (const path of paths) {
    try {
      LOGGER.debug(`Reading template-view at ${path}`);
      const file = await fs.readFile(path);
      const parsed = parse(file);
      if (parsed && typeof parsed == "object") {
        view = { ...view, ...parsed };
      }
    } catch (err) {
      LOGGER.warn(`Failed to load template-view at ${path}`, { err });
    }
  }
  return view;
}

async function readAll(readable: typeof process.stdin): Promise<string> {
  return new Promise<string>((resolve, reject) => {
    const chunks: string[] = [];

    readable.on("readable", () => {
      let chunk;
      while (null !== (chunk = readable.read())) {
        chunks.push(chunk);
      }
    });

    readable.on("end", () => {
      const content = chunks.join("");
      resolve(content);
    });

    readable.on("error", reject);
  });
}
