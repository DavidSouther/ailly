import { FileSystem, Stats } from "@davidsouther/jiffies/lib/esm/fs.js";
import matter from "gray-matter";
import * as yaml from "js-yaml";
import { join, dirname } from "path";
import type { Message } from "../engine/index.js";
import { isDefined } from "../util.js";

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

  // The list of system prompts above this content
  system?: string[];
  predecessor?: Content;
  augment?: { score: number; content: string; name: string }[];
  meta?: ContentMeta;
}

// Additional useful metadata.
export interface ContentMeta {
  root?: string;
  out?: string;
  messages?: Message[];
  skip?: boolean;
  isolated?: boolean;
  combined?: boolean;
  debug?: unknown;
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
  const dir = await fs.readdir("");
  const entries = await Promise.all(dir.map((s) => fs.stat(s)));
  // const entries = await fs.scandir("");
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
  if (name.endsWith(".ailly")) {
    const id = name.replace(/\.ailly$/, "");
    return { type: "response", id };
  }
  return { type: "prompt", id: name };
}

async function loadFile(
  fs: FileSystem,
  file: Stats,
  system: string[],
  head: TODOGrayMatterData
): Promise<Content | undefined> {
  const ordering = splitOrderedName(file.name);
  const cwd = fs.cwd();
  switch (ordering.type) {
    case "prompt":
      let prompt: string = "";
      let data: Record<string, any> = {};
      const promptPath = join(cwd, file.name);
      try {
        const parsed = matter(await fs.readFile(promptPath).catch((e) => ""));
        prompt = parsed.content;
        data = parsed.data;
      } catch (e) {
        console.warn(
          `Error reading prompt and parsing for matter`,
          promptPath,
          e
        );
      }
      let response = "";
      if (data.prompt) {
        response = prompt;
        data.combined = true;
        prompt = data.prompt;
        delete data.prompt;
      } else {
        const responsePath = join(cwd, `${ordering.id}.ailly`).replace(
          head.root,
          head.out
        );
        try {
          response = matter(
            await fs.readFile(responsePath).catch((e) => "")
          ).content;
          data.combined = false;
        } catch (e) {
          console.warn(
            `Error reading response and parsing for matter`,
            responsePath,
            e
          );
        }
      }
      const path = join(fs.cwd(), file.name);
      const outPath = path.replace(head.root, head.out);
      return {
        name: ordering.id,
        system,
        path,
        outPath,
        prompt,
        response,
        meta: {
          ...head,
          ...data,
        },
      };
    default:
      return undefined;
  }
}

/**
 * 1. When the file is .aillyrc.md
 *    1. Parse it into [matter, prompt]
 *    2. Use matter as the meta data base, and put prompt in the system messages.
 * 2. ~When the file is `<nn>p_<name>`~
 *    1. ~Put it in a thread~
 *    2. ~Write the output to `<nn>r_<name>`~
 * 3. When the file name is `<name>.<ext>` (but not `<name>.<ext>.ailly`)
 *    1. Read <name>.<ext>.ailly as the response
 *    1. Write the output to `<name>.<ext>.ailly`
 * 4. If it is a folder
 *    1. Find all files that are not denied by .gitignore
 *    2. Apply the above logic.
 * @param fs the file system abstraction
 * @param system the system message chain
 * @param meta current head matter & settings
 * @returns
 */
export async function loadContent(
  fs: FileSystem,
  system: string[] = [],
  meta: ContentMeta = {}
): Promise<Content[]> {
  console.debug(`Loading content from ${fs.cwd()}`);
  const sys = matter(await fs.readFile(".aillyrc").catch((e) => ""));
  meta = { ...meta, ...sys.data };
  system = [...system, sys.content];
  if (meta.skip) {
    console.debug(
      `Skipping content from ${fs.cwd()} based on head of .aillyrc`
    );
    return [];
  }
  const dir = await loadDir(fs).catch((e) => defaultPartitionedDirectory());
  const files: Content[] = (
    await Promise.all(dir.files.map((file) => loadFile(fs, file, system, meta)))
  ).filter(isDefined);
  if (!Boolean(meta.isolated)) {
    files.sort((a, b) => a.name.localeCompare(b.name));
    for (let i = files.length - 1; i > 0; i--) {
      files[i].predecessor = files[i - 1];
    }
  }

  const folders: Content[] = [];
  for (const folder of dir.folders) {
    if (folder.name == ".vectors") continue;
    fs.pushd(folder.name);
    let contents = await loadContent(fs, system, meta);
    folders.push(...contents);
    fs.popd();
  }

  const content = [...files, ...folders];
  console.debug(`Found ${content.length} at or below ${fs.cwd()}`);
  return content;
}

export async function writeContent(fs: FileSystem, content: Content[]) {
  return Promise.allSettled(
    content.map(async (c) => {
      if (!c.response) return;
      const combined = c.meta?.combined ?? false;
      if (combined && c.outPath != c.path) {
        throw new Error(
          `Mismatch path and output for ${c.path} vs ${c.outPath}`
        );
      }

      const dir = dirname(c.outPath);
      await mkdirp(fs, dir);

      const filename = combined ? c.name : `${c.name}.ailly`;
      console.log(`Writing response for ${filename}`);
      const path = join(dir, filename);
      const { debug, isolated } = c.meta ?? {};
      if (c.augment) {
        (debug as { augment: unknown[] }).augment = c.augment.map(
          ({ score, name }) => ({
            score,
            name,
          })
        );
      }
      // TODO: Ensure `engine` and `model` are in `debug`
      const meta = {
        debug,
        isolated,
        combined,
      };

      if (combined) {
        (meta as unknown as { prompt: string }).prompt = c.prompt;
      }

      const head = yaml.dump(meta, { sortKeys: true });
      const file = `---\n${head}---\n${c.response}`;
      await fs.writeFile(path, file);
    })
  );
}

async function mkdirp(fs: FileSystem, dir: string) {
  const parts = dir.split("/");
  for (let i = 1; i < parts.length; i++) {
    const path = join(...parts.slice(0, i + 1));
    try {
      await fs.stat(path);
    } catch {
      await fs.mkdir(path);
    }
  }
}
