import { FileSystem, Stats } from "@davidsouther/jiffies/lib/esm/fs";
import { get_encoding } from "@dqbd/tiktoken";
import matter from "gray-matter";
import { join, normalize } from "path";
import * as gitignoreParser from "gitignore-parser";

type TODOGrayMatterData = Record<string, any>;
const isDefined = <T>(t: T | undefined): t is T => t !== undefined;

// Content is ordered on the file system using NN_id folders and nnp_id.md files.
// The Content needs to keep track of where in the file system it is, so that a Prompt can write a Response.
// It also needs the predecessor Content at the same level of the file system to build the larger context of its message pairs.
export interface Content {
  path: string;
  system: string[];
  order: number;
  prompt: string;
  response?: string;
  id: string;
  predecessor?: Content;
  head?: TODOGrayMatterData;
  messages?: Message[];
  tokens?: number;
}

const MISSING: Stats = {
  isDirectory: () => false,
  isFile: () => false,
  name: "MISSING",
};

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
      const { content, data } = matter(
        await fs.readFile(join(cwd, file.name)).catch((e) => "")
      );
      const { content: response } = matter(
        await fs
          .readFile(join(cwd, `${ordering.order}r_${ordering.id}`))
          .catch((e) => "")
      );
      return {
        order: ordering.order,
        id: ordering.id,
        system,
        path: join(fs.cwd(), file.name),
        prompt: content,
        response,
        head: { ...head, data },
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

  return [...files, ...folders];
}

const encoding = get_encoding("cl100k_base");
export async function addMessagesToContent(
  contents: Content[]
): Promise<Summary> {
  const summary: Summary = { prompts: 0, tokens: 0 };
  for (const content of contents) {
    content.messages = getMessages(content);
    content.tokens = 0;
    for (const message of content.messages) {
      content.tokens += message.tokens = (
        await encoding.encode(message.content)
      ).length;
    }
    summary.prompts += 1;
    summary.tokens +=
      content.tokens -
      (content.messages?.at(-1)?.role == "assistant"
        ? content.messages.at(-1)?.tokens ?? 0
        : 0);
  }
  return summary;
}

export interface Message {
  role: "system" | "user" | "assistant";
  content: string;
  tokens?: number;
}

export function getMessages(content: Content): Message[] {
  const system = content.system.join("\n");
  const history: Content[] = [];
  while (content) {
    history.push(content);
    content = content.predecessor!;
  }
  history.reverse();
  return [
    { role: "system", content: system },
    ...history
      .map<Array<Message | undefined>>((content) => [
        {
          role: "user",
          content: content.prompt,
        },
        content.response
          ? { role: "assistant", content: content.response }
          : undefined,
      ])
      .flat()
      .filter(isDefined),
  ];
}

export interface Summary {
  tokens: number;
  prompts: number;
}
