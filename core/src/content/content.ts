import {
  FileSystem,
  PLATFORM_PARTS,
  PLATFORM_PARTS_WIN,
  SEP,
  Stats,
  basename,
  isAbsolute,
} from "@davidsouther/jiffies/lib/cjs/fs.js";
import matter from "gray-matter";
import * as YAML from "yaml";
import { join, dirname } from "path";
import type { EngineDebug, Message } from "../engine/index.js";
import { LOGGER, isDefined } from "../util.js";

export const EXTENSION = ".ailly.md";

type TODOGrayMatterData = Record<string, any> | ContentMeta;

// Content is ordered on the file system using NN_name folders and nnp_name.md files.
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
  response?: string;
  responseStream?: ReadableStream;
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
}

// Additional useful metadata.
export interface ContentMeta {
  root?: string;
  out?: string;
  text?: string;
  context?: "conversation" | "folder" | "none";
  parent?: "root" | "always" | "never";
  messages?: Message[];
  skip?: boolean;
  isolated?: boolean;
  combined?: boolean;
  debug?: EngineDebug;
  view?: false | View;
  prompt?: string;
  temperature?: number;
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

interface PartitionedDirectory {
  files: Stats[];
  folders: Stats[];
}

function defaultPartitionedDirectory(): PartitionedDirectory {
  return {
    files: [],
    folders: [],
  };
}

function partitionDirectory(stats: Stats[]): PartitionedDirectory {
  return stats.reduce((dir, stats) => {
    if (stats.isDirectory()) dir.folders.push(stats);
    if (stats.isFile()) dir.files.push(stats);
    return dir;
  }, defaultPartitionedDirectory());
}

async function loadDir(fs: FileSystem): Promise<PartitionedDirectory> {
  const dir = await fs.readdir(".");
  const entries = await Promise.all(dir.map((s) => fs.stat(s)));
  return partitionDirectory(entries);
}

export type Ordering =
  | { id: string; type: "prompt" | "response" }
  | { id: string; type: "system" }
  | { type: "ignore" };

export function splitOrderedName(name: string): Ordering {
  if (name.startsWith(".aillyrc")) {
    return { type: "system", id: name };
  }
  if (name.startsWith("_")) {
    return { type: "ignore" };
  }
  if (name.endsWith(EXTENSION)) {
    const id = name.replace(new RegExp(EXTENSION + "$"), "");
    return { type: "response", id };
  }
  return { type: "prompt", id: name };
}

async function loadFile(
  fs: FileSystem,
  file: Stats,
  system: System[],
  head: TODOGrayMatterData
): Promise<Content | undefined> {
  const ordering = splitOrderedName(file.name);
  const cwd = fs.cwd();
  if (ordering.type == "prompt") {
    head.root = head.root ?? cwd;
    const promptPath = join(cwd, file.name);

    let text: string = "";
    let prompt: string = "";
    let data: Record<string, any> = {};
    try {
      text = await fs.readFile(promptPath).catch((e) => "");
      const parsed = matter(text);
      prompt = parsed.content;
      data = parsed.data;
    } catch (e) {
      LOGGER.warn(
        `Error reading prompt and parsing for matter in ${promptPath}`,
        e as Error
      );
    }

    let response: string | undefined;
    let outPath: string;
    if (data.prompt) {
      outPath = promptPath;
      response = prompt;
      data.combined = true;
      prompt = data.prompt;
      delete data.prompt;
    } else {
      outPath =
        head.out === undefined || head.root === head.out
          ? promptPath
          : promptPath.replace(head.root, head.out);
      if (!head.combined) {
        outPath += EXTENSION;
        try {
          response = matter(
            await fs.readFile(outPath).catch((e) => "")
          ).content;
          data.combined = false;
        } catch (e) {
          LOGGER.warn(
            `Error reading response and parsing for matter in ${outPath}`,
            e as Error
          );
        }
      }
    }
    if (response?.trim() == "") response = undefined;

    const view = data.view === false ? false : data.view ?? {};
    delete data.view;

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
      meta: {
        ...head,
        ...data,
        text,
      },
    };
  } else {
    return undefined;
  }
}

export async function loadAillyRc(
  fs: FileSystem,
  system: System[],
  meta: ContentMeta
): Promise<[System[], ContentMeta]> {
  meta.parent = meta.parent ?? "root";
  const aillyrc = await fs.readFile(".aillyrc").catch((e) => "");
  // Reset to "root" if there's no intervening .aillyrc.
  if (aillyrc === "" && meta.parent === "always") meta.parent = "root";
  const { data, content } = matter(aillyrc);
  if (data["template-view"]) {
    try {
      const templateView = await fs.readFile(data["template-view"]);
      const view = YAML.parse(templateView);
      data.view = { ...(data.view ?? {}), ...view };
    } catch (err) {
      LOGGER.warn("Failed to load template-view", { err });
    }
  }
  const view = data.view ?? {};
  delete data.view;
  meta = { ...meta, ...data };
  switch (meta.parent) {
    case "root":
      if (!(content == "" && Object.keys(view).length == 0))
        system = [...system, { content, view }];
      break;
    case "never":
      if (content != "") {
        system = [{ content, view }];
      } else {
        system = [];
      }
      break;
    case "always":
      if (system.length == 0 && fs.cwd() !== "/") {
        fs.pushd("..");
        system = [...(await loadAillyRc(fs, [], meta))[0]];
        fs.popd();
      }
      if (content != "") system = [...system, { content, view }];
  }

  return [system, meta];
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
  system: System[] = [],
  meta: ContentMeta = {},
  depth: number = Number.MAX_SAFE_INTEGER
): Promise<Record<string, Content>> {
  if (depth < 1) return {};
  LOGGER.debug(`Loading content from ${fs.cwd()}`);
  [system, meta] = await loadAillyRc(fs, system, meta);
  if (meta.skip) {
    LOGGER.debug(`Skipping content from ${fs.cwd()} based on head of .aillyrc`);
    return {};
  }

  const dir = await loadDir(fs).catch((e) => defaultPartitionedDirectory());
  const files: Content[] = (
    await Promise.all(dir.files.map((file) => loadFile(fs, file, system, meta)))
  ).filter(isDefined);

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
        delete file.context.system;
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

  const folders: Record<string, Content> = {};
  if (context != "none") {
    for (const folder of dir.folders) {
      if (folder.name == ".vectors") continue;
      fs.pushd(folder.name);
      let contents = await loadContent(
        fs,
        system,
        meta,
        depth ? depth - 1 : undefined
      );
      Object.assign(folders, contents);
      fs.popd();
    }
  }

  const content: Record<string, Content> = {
    ...files.reduce(
      (c, f) => ((c[f.path] = f), c),
      {} as Record<string, Content>
    ),
    ...folders,
  };
  LOGGER.debug(`Found ${Object.keys(content).length} at or below ${fs.cwd()}`);
  return content;
}

async function writeSingleContent(fs: FileSystem, content: Content) {
  if (!content.response) return;
  const combined = content.meta?.combined ?? false;
  if (combined && content.outPath != content.path) {
    throw new Error(
      `Mismatch path and output for ${content.path} vs ${content.outPath}`
    );
  }

  const dir = dirname(content.outPath);
  await mkdirp(fs, dir);

  const filename = content.name + (combined ? "" : EXTENSION);
  LOGGER.info(`Writing response for ${filename}`);
  const path = join(dir, filename);
  const { debug, isolated } = content.meta ?? {};
  if (content.context.augment) {
    (debug as { augment: unknown[] }).augment = content.context.augment.map(
      ({ score, name }) => ({
        score,
        name,
      })
    );
  }
  // TODO: Ensure `engine` and `model` are in `debug`
  const meta: ContentMeta = {
    debug,
    isolated,
    combined,
  };

  if (combined) {
    meta.prompt = content.meta?.prompt ?? content.prompt;
  }

  const head = YAML.stringify(meta, {
    blockQuote: "literal",
    lineWidth: 0,
    sortMapEntries: true,
  });
  const file = `---\n${head}---\n${content.response}`;
  await fs.writeFile(path, file);
}

export async function writeContent(fs: FileSystem, content: Content[]) {
  for (const c of Object.values(content)) {
    try {
      await writeSingleContent(fs, c);
    } catch (e) {
      LOGGER.error(`Failed to write content ${c.name}`, e as Error);
    }
  }
}

const IS_WINDOWS = PLATFORM_PARTS == PLATFORM_PARTS_WIN;
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

/**
 * Create a "synthetic" Content block with path "/dev/stdout" to serve as the Content root
 * for this Ailly call to the LLM.
 */
export function makeCLIContent(
  prompt: string,
  argContext: "none" | "folder" | "conversation",
  argSystem: string,
  context: Record<string, Content>,
  root: string,
  edit: AillyEdit | undefined,
  view: View,
  isolated: boolean
): Content {
  const inFolder = Object.keys(context).filter((c) => dirname(c) == root);
  // When argContext is folder, `folder` is all files in context in root.
  const folder =
    argContext == "folder"
      ? isolated && edit
        ? [edit?.file]
        : inFolder
      : undefined;
  // When argContext is `conversation`, `predecessor` is the last item in the root folder.
  const predecessor =
    argContext == "conversation" ? inFolder.at(-1) : undefined;
  // When argContext is none, system is empty; otherwise, system is argSystem + predecessor's system.
  const system =
    argContext == "none"
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
    cliContent.prompt = "{{{output.edit}}}";
  }
  return cliContent;
}
