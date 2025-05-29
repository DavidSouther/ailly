import { dirname, join } from "node:path";
import matter, { type GrayMatterFile } from "gray-matter";
import * as YAML from "yaml";

import {
  type FileSystem,
  PLATFORM_PARTS,
  PLATFORM_PARTS_WIN,
  SEP,
  type Stats,
  basename,
  isAbsolute,
} from "@davidsouther/jiffies/lib/cjs/fs.js";

import {
  assertExists,
  checkExhaustive,
} from "@davidsouther/jiffies/lib/cjs/assert";
import type { EngineDebug, Message } from "../engine/index.js";
import type { Tool } from "../engine/tool.js";
import { LOGGER } from "../index.js";
import {
  type PromiseWithResolvers,
  isDefined,
  withResolvers,
} from "../util.js";
import { loadTemplateView } from "./template.js";

export const EXTENSION = ".ailly.md";
const END_REGEX = new RegExp(`${EXTENSION}$`);

type GrayMatterData = GrayMatterFile<string>["data"];

// Content is ordered on the file system using NN_name folders and nn_name.md[.ailly.md] files.
// The Content needs to keep track of where in the file system it is, so that a Prompt can write a Response.
// It also needs the predecessor Content at the same level of the file system to build the larger context of its message pairs.
export interface Content {
  // The absolute path in the local file system
  path: string;
  outPath: string;

  // The extracted name from the basename
  name: string;

  // The prompt itself
  prompt: string;
  // Any loaded or generated response
  response?: string;
  responseStream?: PromiseWithResolvers<ReadableStream<string>>;
  context: Context;
  meta?: ContentMeta;
}

export interface Context {
  view: false | View;
  // The list of system prompts above this content
  system?: System[];
  predecessor?: string;
  folder?: string[];
  augment?: { score: number; content: string; name: string }[];
  edit?:
    | { start: number; end: number; file: string }
    | { after: number; file: string };
  mcpClient?: never; // TODO: Define the mcpClient
}

// Additional useful metadata.
export interface ContentMeta {
  root?: string;
  out?: string;
  text?: string;
  context?: "conversation" | "folder" | "none";
  parent?: "root" | "always" | "never";
  continue?: boolean;
  messages?: Message[];
  skip?: boolean;
  skipHead?: boolean;
  isolated?: boolean;
  combined?: boolean;
  debug?: EngineDebug;
  view?: false | View;
  prompt?: string;
  temperature?: number;
  maxTokens?: number;
  tools?: Tool[];
}

export type AillyEditReplace = { start: number; end: number; file: string };
export type AillyEditInsert = { after: number; file: string };
export type AillyEdit = AillyEditReplace | AillyEditInsert;
export const isAillyEditReplace = (edit: AillyEdit): edit is AillyEditReplace =>
  (edit as AillyEditReplace).start !== undefined;

export type Value = string | number | boolean;
export interface View extends Record<string, View | Value> {}

export interface System {
  content: string;
  view: false | View;
}

export type Ordering =
  | { id: string; type: "prompt" | "response" }
  | { id: string; type: "system" }
  | { type: "ignore" };

export const splitOrderedName = (name: string): Ordering =>
  name === ".aillyrc"
    ? { type: "system", id: name }
    : name.endsWith(EXTENSION)
      ? { type: "response", id: name.replace(END_REGEX, "") }
      : { type: "prompt", id: name };

interface PartitionedDirectory {
  files: Stats[];
  folders: Stats[];
}

/**
 * 1. When the file is .aillyrc.md
 *    1. Parse it into [matter, prompt]
 *    2. Use matter as the meta data base, and put prompt in the system messages.
 * 2. When the file name is `<name>.<ext>` (but not `<name>.<ext>.ailly`)
 *    1. Read <name>.<ext>.ailly as the response
 *    1. Write the output to `<name>.<ext>.ailly`
 * 3. If it is a folder
 *    1. Find all files that are not denied by .gitignore
 *    2. Apply the above logic.
 * @param fs the file system abstraction.
 * @param system the system message chain.
 * @param meta current head matter & settings.
 * @param depth maximum depth to load. `1` loads only the cwd; 0 or negative loads no content.
 * @returns
 */
export async function loadContent(
  fs: FileSystem,
  parent: { system?: System[]; meta?: ContentMeta } = { system: [], meta: {} },
  depth: number = Number.MAX_SAFE_INTEGER,
): Promise<Record<string, Content>> {
  if (depth < 0) throw new Error("Depth in loadContent cannot be negative");
  if (depth < 1) return {};
  LOGGER.debug(`Loading content from ${fs.cwd()}`);
  const [system, meta] = await loadAillyRc(
    fs,
    parent.system ?? [],
    parent.meta ?? {},
  );
  if (meta.skip) {
    LOGGER.debug(`Skipping content from ${fs.cwd()} based on head of .aillyrc`);
    return {};
  }

  const dir = await loadDir(fs).catch((e) => ({ files: [], folders: [] }));
  const loaders = dir.files.map((file) => loadFile(fs, file, system, meta));
  const allFiles = await Promise.all(loaders);
  const files: Content[] = allFiles.filter(isDefined);

  const isIsolated = Boolean(meta.isolated);
  const context: NonNullable<ContentMeta["context"]> =
    meta.context ?? "conversation";
  switch (context) {
    case "conversation":
      if (isIsolated) break;
      files.sort((a, b) => a.name.localeCompare(b.name));
      for (let i = files.length - 1; i > 0; i--) {
        files[i].context.predecessor = files[i - 1].path;
      }
      break;
    case "none":
      for (const file of files) {
        file.context.system = undefined;
      }
      break;
    case "folder":
      if (isIsolated) {
        for (const file of files) {
          file.context.folder = [file.path];
        }
      } else {
        const folder = files.map((f) => f.path);
        for (const file of files) {
          file.context.folder = folder;
        }
      }
      break;
  }

  // TODO: Extract MCP information from meta server and attach MCP Clients to Context
  // TODO: stdio, http, and `mock`

  const folders: Record<string, Content> = {};
  if (context !== "none") {
    for (const folder of dir.folders) {
      if (folder.name === ".vectors") continue;
      fs.pushd(folder.name);
      const contents = await loadContent(
        fs,
        { system, meta },
        depth ? depth - 1 : undefined,
      );
      Object.assign(folders, contents);
      fs.popd();
    }
  }

  const content: Record<string, Content> = {
    ...files.reduce(
      (c, f) => {
        c[f.path] = f;
        return c;
      },
      {} as Record<string, Content>,
    ),
    ...folders,
  };
  LOGGER.debug(`Found ${Object.keys(content).length} at or below ${fs.cwd()}`);
  return content;
}

async function loadDir(fs: FileSystem): Promise<PartitionedDirectory> {
  const dir = await fs.readdir(".");
  const entries = await Promise.all(dir.map((s) => fs.stat(s)));

  const partitioned = entries.reduce(
    (dir: PartitionedDirectory, stats) => {
      if (stats.isDirectory()) dir.folders.push(stats);
      if (stats.isFile()) dir.files.push(stats);
      return dir;
    },
    { files: [], folders: [] },
  );
  return partitioned;
}

async function loadFile(
  fs: FileSystem,
  file: Stats,
  system: System[],
  head: GrayMatterData,
): Promise<Content | undefined> {
  const ordering = splitOrderedName(file.name);
  const cwd = fs.cwd();
  if (ordering.type === "prompt") {
    head.root = head.root ?? cwd;
    const promptPath = join(cwd, file.name);

    let text = "";
    let prompt = "";
    let data: Record<string, unknown> = {};
    try {
      text = await fs.readFile(promptPath).catch((e) => "");
      const parsed = matter(text);
      prompt = parsed.content;
      data = parsed.data;
    } catch (err) {
      LOGGER.warn(
        `Error reading prompt and parsing for matter in ${promptPath}`,
        { err },
      );
      return undefined;
    }

    let response: string | undefined;
    let outPath: string;
    const combined = head.combined ?? data.combined;
    if (data.prompt && combined !== false) {
      outPath = promptPath;
      response = prompt;
      data.combined = true;
      prompt = data.prompt as string;
      data.prompt = undefined;
    } else {
      if (data.prompt !== undefined) {
        if (prompt.trim().length > 0) {
          LOGGER.warn(`Head and body both have prompt, skipping ${file.name}`);
          data.skip = true;
        } else {
          prompt = data.prompt as string;
        }
      }

      outPath =
        head.out === undefined || head.root === head.out
          ? promptPath
          : promptPath.replace(head.root, head.out);
      if (!head.combined) {
        outPath += EXTENSION;
        data.combined = false;
        try {
          const outStat = await fs.stat(outPath);
          if (outStat.isFile()) {
            try {
              const outFile = await fs.readFile(outPath).catch((e) => "");
              const parsedResponse: GrayMatterFile<string> = matter(outFile);
              response = parsedResponse.content;
              data.meta = data.meta ?? {};
              (data.meta as { debug: object }).debug =
                parsedResponse.data.meta?.debug ?? {};
            } catch (err) {
              LOGGER.warn(
                `Error reading response and parsing for matter in ${outPath}`,
                { err },
              );
              return undefined;
            }
          }
        } catch (_) {
          // Noop
        }
      }
    }

    if (prompt === "") {
      LOGGER.warn(`Could not find a prompt in ${promptPath}`);
      return undefined;
    }

    if (response?.trim() === "") response = undefined;

    const view: false | View =
      data.view === false
        ? false
        : ((data.view as View) ?? ({} satisfies View));
    data.view = undefined;

    return {
      name: ordering.id,
      path: promptPath,
      outPath,
      context: {
        system,
        view,
      },
      prompt,
      response,
      responseStream: withResolvers(),
      meta: {
        ...head,
        ...data,
        text,
      },
    };
  }
  return undefined;
}

export async function loadAillyRc(
  fs: FileSystem,
  system: System[],
  meta: ContentMeta,
): Promise<[System[], ContentMeta]> {
  meta.parent = meta.parent ?? "root";
  const aillyrc = await fs.readFile(".aillyrc").catch((e) => "");
  // Reset to "root" if there's no intervening .aillyrc.
  if (aillyrc === "" && meta.parent === "always") meta.parent = "root";
  const { data, content } = matter(aillyrc);
  if (data["template-view"]) {
    const view = await loadTemplateView(fs, data["template-view"]);
    data.view = { ...(data.view ?? {}), ...view };
  }
  const view = data.view ?? {};
  data.view = undefined;
  return mergeAillyRc(meta, data, content, view, system, fs);
}

async function mergeAillyRc(
  content_meta: ContentMeta,
  data: { [key: string]: unknown },
  content: string,
  view: View,
  system: System[],
  fs: FileSystem,
): Promise<[System[], ContentMeta]> {
  const meta = { ...{ parent: "root" as const }, ...content_meta, ...data };
  switch (meta.parent) {
    case "root":
      if (!(content === "" && Object.keys(view).length === 0))
        system.push({ content, view });
      break;
    case "never":
      if (content) {
        system.splice(0, system.length, { content, view });
      } else {
        system.splice(0, system.length);
      }
      break;
    case "always":
      if (system.length === 0 && fs.cwd() !== "/") {
        fs.pushd("..");
        const [rcBlocks, _] = await loadAillyRc(fs, [], meta);
        for (const rcBlock of rcBlocks) {
          system.push(rcBlock);
        }
        fs.popd();
      }
      if (content) {
        system.push({ content, view });
      }
      break;
    default:
      checkExhaustive(meta.parent);
  }
  return [system, meta];
}

/**
 * Create a "synthetic" Content block with path "/dev/stdout" to serve as the Content root
 * for this Ailly call to the LLM.
 */
export function makeCLIContent({
  prompt,
  argContext = "conversation",
  argSystem = "",
  context,
  root,
  edit,
  view = {},
  isolated = true,
}: {
  prompt: string;
  argContext: "none" | "folder" | "conversation";
  argSystem?: string;
  context: Record<string, Content>;
  root?: string;
  edit?: AillyEdit | undefined;
  view?: View;
  isolated?: boolean;
}): Content {
  const inFolder = Object.keys(context).filter((c) => dirname(c) === root);
  // When argContext is folder, `folder` is all files in context in root.
  const folder =
    argContext === "folder"
      ? isolated && edit
        ? [edit?.file]
        : inFolder
      : undefined;
  // When argContext is `conversation`, `predecessor` is the last item in the root folder.
  const predecessor =
    argContext === "conversation" ? inFolder.at(-1) : undefined;
  // When argContext is none, system is empty; otherwise, system is argSystem + predecessor's system.
  const system =
    argContext === "none"
      ? []
      : [
          { content: argSystem ?? "", view: {} },
          ...((predecessor ? context[predecessor].context.system : undefined) ??
            []),
        ];
  const cliContent: Content = {
    name: "stdout",
    outPath: "/dev/stdout",
    path: "/dev/stdout",
    prompt: prompt ?? "",
    responseStream: withResolvers(),
    context: {
      view,
      predecessor,
      system,
      folder,
      edit,
    },
  };
  if (edit && context[edit.file]) {
    cliContent.meta = context[edit.file].meta;
    const name: string = basename(context[edit.file].name);
    const ext: string = name.split(".", 2).at(-1) ?? name;
    const start =
      (edit as { start: number }).start ??
      (edit as { after: number }).after ??
      0;
    const end = (edit as { end: number }).end ?? -1;
    const selection: string = (context[edit.file].meta?.text?.split("\n") ?? [])
      .slice(start, end)
      .join("\n");
    cliContent.context.view = {
      ...cliContent.context.view,
      ...{
        request: {
          name,
          ext,
          prompt: cliContent.prompt,
        },
        edit: { selection },
      },
    };
    cliContent.prompt = "{{{ output.edit }}}";
  }
  return cliContent;
}

/** The responseStream is for coordinating generation, not writing, of content. */
export type WritableContent = Omit<Content, "responseStream"> &
  Partial<Pick<Content, "responseStream">>;

export async function writeContent(
  fs: FileSystem,
  content: WritableContent[],
  options?: { clean?: boolean },
) {
  for (const c of Object.values(content)) {
    try {
      await writeSingleContent(fs, c, options);
    } catch (e) {
      LOGGER.error(`Failed to write content ${c.name}`, e as Error);
    }
  }
}

async function writeSingleContent(
  fs: FileSystem,
  content: WritableContent,
  options?: { clean?: boolean },
) {
  if (!content.response) return;
  const combined = content.meta?.combined ?? false;
  if (combined && content.outPath !== content.path) {
    throw new Error(
      `Mismatch path and output for ${content.path} vs ${content.outPath}`,
    );
  }

  const clean = options?.clean ?? false;
  if (clean && !combined) {
    await fs.rm(content.outPath);
    return;
  }

  const dir = dirname(content.outPath);
  await mkdirp(fs, dir);

  const filename = content.name + (combined ? "" : EXTENSION);
  LOGGER.info(`Writing response for ${filename}`);
  const path = join(dir, filename);
  const { debug, isolated, skip } = content.meta ?? {};
  if (content.context.augment && !clean) {
    (debug as { augment: unknown[] }).augment = content.context.augment.map(
      ({ score, name }) => ({
        score,
        name,
      }),
    );
  }

  const meta: ContentMeta = {
    ...(skip ? { skip } : {}),
    ...(combined ? { combined } : {}),
    ...(isolated ? { isolated } : {}),
    ...(Object.keys(content.context?.view ?? {}).length > 0
      ? { view: content.context?.view }
      : {}),
    ...(content.meta?.temperature
      ? { temperature: content.meta.temperature }
      : {}),
    ...(content.meta?.maxTokens ? { maxTokens: content.meta.maxTokens } : {}),
    ...(!clean ? { debug } : {}),
  };

  if (combined) {
    meta.prompt = content.meta?.prompt ?? content.prompt;
  }

  if (clean) {
    content.response = "";
  }

  const head = YAML.stringify(meta, {
    blockQuote: "literal",
    lineWidth: 0,
    sortMapEntries: true,
  });
  const file =
    (!combined && (content.meta?.skipHead || head === "{}\n")
      ? ""
      : `---\n${head}---\n`) + content.response;
  await fs.writeFile(path, file);
}

const IS_WINDOWS = PLATFORM_PARTS === PLATFORM_PARTS_WIN;
async function mkdirp(fs: FileSystem, dir: string) {
  if (!isAbsolute(dir)) {
    LOGGER.warn(`Cannot mkdirp on non-absolute path ${dir}`);
    return;
  }
  const parts = dir.split(SEP);
  for (let i = 1; i < parts.length; i++) {
    let path = join(SEP, ...parts.slice(1, i + 1));
    if (IS_WINDOWS) {
      path = parts[0] + SEP + path;
    }
    try {
      await fs.stat(path);
    } catch {
      await fs.mkdir(path);
    }
  }
}
