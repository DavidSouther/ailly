import { FileSystem, Stats } from "@davidsouther/jiffies/lib/esm/fs";
import matter from "gray-matter";
import { join, normalize } from "path";
import * as gitignoreParser from "gitignore-parser";
import { Message } from "./openai";
import { isDefined } from "./util";

type TODOGrayMatterData = Record<string, any>;

// Content is ordered on the file system using NN_name folders and nnp_name.md files.
// The Content needs to keep track of where in the file system it is, so that a Prompt can write a Response.
// It also needs the predecessor Content at the same level of the file system to build the larger context of its message pairs.
export interface Content {
  path: string; // The absolute path in the local file system
  name: string; // The extracted name
  order: number; // The extracted order
  system: string[]; // The list of system prompts above this content
  prompt: string;
  response?: string;
  predecessor?: Content;
  meta?: ContentMeta;
}

// Additional useful metadata.
export interface ContentMeta {
  head?: TODOGrayMatterData; // Any graymatter metadata
  messages?: Message[];
  tokens?: number;
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
  | { order: number; id: string; type: "prompt" | "response" }
  | { id: string; type: "system" }
  | { type: "ignore" };

function splitOrderedName(name: string): Ordering {
  if (name.startsWith("_s")) {
    return { type: "system", id: name.substring(2) };
  }
  if (name.startsWith("_")) {
    return { type: "ignore" };
  }
  const parts = name.match(/(\d+)([pr]?)_(.+)/);
  if (parts) {
    return {
      type: parts[2] === "p" ? "prompt" : "response",
      order: Number(parts[1]),
      id: parts[3],
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
        await fs
          .readFile(join(cwd, `${ordering.order}r_${ordering.id}`))
          .catch((e) => "")
      );
      return {
        name: ordering.id,
        system,
        path: join(fs.cwd(), file.name),
        prompt,
        response,
        order: ordering.order,
        meta: {
          head: { ...head, data },
        },
      };
    default:
      return undefined;
  }
}

export async function loadContent(
  fs: FileSystem,
  system: string[] = [],
  head: TODOGrayMatterData = {}
): Promise<Content[]> {
  const sys = matter(await fs.readFile("_s.md").catch((e) => ""));
  head = { ...head, ...sys.data };
  system = [...system, sys.content];
  if (head["skip"]) {
    return [];
  }
  const dir = await loadDir(fs).catch((e) => defaultPartitionedDirectory());
  const files: Content[] = (
    await Promise.all(dir.files.map((file) => loadFile(fs, file, system, head)))
  ).filter(isDefined);
  if (!Boolean(head.isolated)) {
    files.sort((a, b) => a.order - b.order);
    for (let i = files.length - 1; i > 0; i--) {
      files[i].predecessor = files[i - 1];
    }
  }

  const folders: Content[] = [];
  for (const folder of dir.folders) {
    fs.pushd(folder.name);
    let contents = await loadContent(fs, system, head);
    folders.push(...contents);
    fs.popd();
  }

  const content = [...files, ...folders];
  return content;
}
