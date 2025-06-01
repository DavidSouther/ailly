import { Console } from "node:console";
import { join, resolve } from "node:path";

import {
  type AillyEdit,
  type Content,
  loadContent,
  makeCLIContent,
} from "@ailly/core/lib/content/content.js";
import { loadTemplateView } from "@ailly/core/lib/content/template.js";
import {
  type PipelineSettings,
  LOGGER as ROOT_LOGGER,
  makePipelineSettings,
} from "@ailly/core/lib/index.js";
import { assertExists } from "@davidsouther/jiffies/lib/cjs/assert.js";
import type { FileSystem } from "@davidsouther/jiffies/lib/cjs/fs.js";
import {
  LEVEL,
  getLogLevel,
  getLogger,
} from "@davidsouther/jiffies/lib/cjs/log.js";

import type { Args } from "./args.js";

function basicLogFormatter(
  data: { prefix: string; message: string } | Record<string, unknown>,
) {
  return `${data.prefix}: ${data.message} ${Object.entries(data)
    .map(([k, v]) => {
      if (
        k === "name" ||
        k === "prefix" ||
        k === "message" ||
        k === "level" ||
        k === "source"
      )
        return "";
      return `${k}=${JSON.stringify(v)}`;
    })
    .join(" ")}`;
}

export const LOGGER = getLogger("@ailly/cli");
export async function loadFs(
  fs: FileSystem,
  args: Args,
): Promise<{
  context: Record<string, Content>;
  content: string[];
  settings: PipelineSettings;
}> {
  const root = resolve(args.values.root ?? ".");
  fs.cd(root);

  const positionals = args.positionals.map((a) => resolve(join(root, a)));
  const hasPositionals = positionals.length > 0;
  const hasPrompt = (args.values.prompt ?? "") !== "";
  const isPipe = !hasPositionals && hasPrompt;
  const logLevel =
    args.values["log-level"]?.toLowerCase() ??
    (args.values.verbose ? "verbose" : isPipe ? "silent" : "info");
  ROOT_LOGGER.console = LOGGER.console = isPipe
    ? new Console(process.stderr, process.stderr)
    : global.console;
  ROOT_LOGGER.level = LOGGER.level = getLogLevel(
    logLevel === "trace" ? "0.5" : logLevel,
  );
  const logFormat = args.values["log-format"];
  const formatter =
    logFormat === "json" ||
    ROOT_LOGGER.level < LEVEL.DEBUG ||
    args.values.verbose
      ? JSON.stringify
      : basicLogFormatter;
  ROOT_LOGGER.format = LOGGER.format = formatter;

  const argContext =
    args.values.context ?? (args.values.edit ? "folder" : undefined);

  const out = resolve(args.values.out ?? root);

  const templateView = await loadTemplateView(
    fs,
    ...(args.values["template-view"] ?? []),
  );
  const isExpensiveModel = args.values.model?.includes("opus") ?? false;
  const baseRequestLimit = args.values["request-limit"]
    ? Number(args.values["request-limit"])
    : undefined;
  const requestLimit = baseRequestLimit ?? (isExpensiveModel ? 1 : 5);

  const settings = await makePipelineSettings({
    root,
    out,
    context: argContext,
    isolated: args.values.isolated,
    combined: args.values.combined,
    continued: args.values.continue,
    engine: args.values.engine,
    model: args.values.model,
    plugin: args.values.plugin,
    templateView,
    overwrite: !args.values["no-overwrite"],
    requestLimit,
    skipHead: args.values["skip-head"],
  });

  const system = args.values.system ?? "";
  const depth = Number(args.values["max-depth"]);

  const context = await loadContent(
    fs,
    {
      system: system ? [{ content: system, view: {} }] : [],
      meta: settings,
    },
    depth,
  );

  let content: string[] = [];
  if (!hasPositionals && hasPrompt) {
    for (const c of Object.values(context)) {
      c.meta = c.meta ?? {};
      c.meta.skip = true;
    }
  } else {
    if (!hasPositionals) positionals.push(root);
    content = Object.keys(context).filter((c) =>
      positionals.some((p) => c.startsWith(p)),
    );
  }

  const edit = args.values.edit
    ? makeEdit(args.values.lines, content, hasPrompt)
    : undefined;
  if (hasPrompt) {
    if (edit && content.length === 1) {
      content = [];
    }
    const prompt = await readPrompt(args);
    const cliContent = makeCLIContent({
      prompt,
      argContext: settings.context,
      argSystem: system,
      context,
      root,
      edit,
      view: settings.templateView,
      isolated: settings.isolated,
    });
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
  hasPrompt: boolean,
): AillyEdit | undefined {
  if (!lines) return undefined;
  if (content.length !== 1) {
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

async function readAll(readable: typeof process.stdin): Promise<string> {
  return new Promise<string>((resolve, reject) => {
    const chunks: string[] = [];

    readable.on("readable", () => {
      let chunk: string | null;
      // biome-ignore lint/suspicious/noAssignInExpressions: quick read
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
