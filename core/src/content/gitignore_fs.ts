import { isAscii } from "node:buffer";
import { join, normalize } from "node:path";
import {
  FileSystem,
  SEP,
  type Stats,
} from "@davidsouther/jiffies/lib/cjs/fs.js";
import * as gitignoreParser from "gitignore-parser";

const IGNORED_NAMES = [".git", ".gitignore"];
export class GitignoreFs extends FileSystem {
  async readdir(path: string): Promise<string[]> {
    // biome-ignore lint/style/noParameterAssign: update path based on CWD
    path = (this as unknown as { p(p: string): string }).p(path);
    const [drive, ...dirs] = this.cwd().split("/");
    const gitignores: Array<{
      path: string;
      gitignore: string;
      accepts: ReturnType<typeof gitignoreParser.compile>["accepts"];
    }> = [];
    for (let i = 1; i <= dirs.length; i++) {
      const gitignorePath = normalize(
        drive + SEP + join(...dirs.slice(0, i), ".gitignore"),
      );
      const gitignore = await this.readFile(gitignorePath).catch((e) => "");
      const parser = gitignoreParser.compile(gitignore);
      gitignores.push({
        path: gitignorePath,
        gitignore,
        accepts: (input) => parser.accepts(input),
      });
    }
    const paths = await this.adapter.scandir(path);
    const filtered: Stats[] = [];
    await Promise.all(
      paths.map(async (stats) => {
        const include =
          !IGNORED_NAMES.includes(stats.name) &&
          (stats.isDirectory() || (await this.isTextFile(stats))) && // This test was intended to limit us to text files only
          gitignores.every((g) =>
            stats.isDirectory()
              ? g.accepts(`${stats.name}/`)
              : g.accepts(stats.name),
          );
        if (include) {
          filtered.push(stats);
        }
      }),
    );
    return filtered.map((p) => p.name);
  }

  private async isTextFile(p: Stats): Promise<boolean> {
    try {
      const content = await this.readFile(p.name);
      return isAscii(Buffer.from(content.slice(0, 5), "utf-8"));
    } catch (e) {
      return false;
    }
  }
}

function isTextExtension(name: string) {
  // If theres no extension, err on the side
  // of caution and don't include it in the context.
  if (!name.includes(".")) {
    return false;
  }

  // From https://github.com/bevry/binaryextensions/blob/master/source/index.ts
  const binaryExtensions = [
    "dds",
    "eot",
    "gif",
    "ico",
    "jar",
    "jpeg",
    "jpg",
    "pdf",
    "png",
    "swf",
    "tga",
    "ttf",
    "zip",
  ];

  const ext = name.split(".").pop() || "";
  return !binaryExtensions.includes(ext);
}
