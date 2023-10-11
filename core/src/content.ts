import { FileSystem, Stats } from "@davidsouther/jiffies/lib/esm/fs.js";
import matter from "gray-matter";
import { join, normalize, dirname } from "path";
import * as gitignoreParser from "gitignore-parser";
import { type Message } from "./plugin/index.js";
import { isDefined } from "./util.js";

type TODOGrayMatterData = Record<string, any>;

// Content is ordered on the file system using NN_name folders and nnp_name.md files.
// The Content needs to keep track of where in the file system it is, so that a Prompt can write a Response.
// It also needs the predecessor Content at the same level of the file system to build the larger context of its message pairs.
export interface Content {
  // The absolute path in the local file system
  path: string;

  // The extracted name from the basename
  name: string;

  // The list of system prompts above this content
  system: string[];

  prompt: string;
  response?: string;
  predecessor?: Content;
  meta?: ContentMeta;
}

// Additional useful metadata.
export interface ContentMeta {
  messages?: Message[];
  tokens?: number;
  isolated?: boolean;
  plugin?: string;
  model?: string;
  skip?: boolean;
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
  const cwd = fs.cwd();
  const gitignore = gitignoreParser.compile(
    await fs.readFile(normalize(join(cwd, ".gitignore"))).catch((e) => ".git")
  );
  const dir = (await fs.readdir(cwd)).filter(
    (d) => gitignore.accepts(d) && !d.endsWith(".git")
  );
  const entries = await Promise.all(dir.map((s) => fs.stat(s)));
  return partitionDirectory(entries);
}

export type Ordering =
  | { id: string; type: "prompt" | "response" }
  | { id: string; type: "system" }
  | { type: "ignore" };

function splitOrderedName(name: string): Ordering {
  if (name.startsWith(".aillyrc")) {
    return { type: "system", id: name };
  }
  if (name.startsWith("_")) {
    return { type: "ignore" };
  }
  const parts = name.match(/(?<i>.+)(?<r>\.ailly)?/);
  if (parts) {
    const { i, r } = parts.groups ?? {};
    const response = r !== undefined;
    return {
      type: response ? "response" : "prompt",
      id: i ?? name,
    };
  }

  return { type: "ignore" };
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
      const { content: prompt, data } = matter(
        await fs.readFile(join(cwd, file.name)).catch((e) => "")
      );
      const { content: response } = matter(
        await fs.readFile(join(cwd, `${ordering.id}.ailly`)).catch((e) => "")
      );
      return {
        name: ordering.id,
        system,
        path: join(fs.cwd(), file.name),
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
      const dir = dirname(c.path);
      const filename = `${c.name}.ailly`;
      console.log(`Writing response for ${filename}`);
      fs.writeFile(join(dir, filename), c.response);
    })
  );
}
