import { FileSystem, SEP } from "@davidsouther/jiffies/lib/cjs/fs.js";
import { join, normalize } from "path";
import * as gitignoreParser from "gitignore-parser";

export class GitignoreFs extends FileSystem {
  async readdir(path: string): Promise<string[]> {
    path = (this as unknown as { p(p: string): string }).p(path);
    const [drive, ...dirs] = this.cwd().split("/");
    const gitignores: Array<{
      path: string;
      gitignore: string;
      accepts: ReturnType<typeof gitignoreParser.compile>["accepts"];
    }> = [];
    for (let i = 1; i <= dirs.length; i++) {
      const gitignorePath = normalize(
        drive + SEP + join(...dirs.slice(0, i), ".gitignore")
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
    const ignoredNames = [".git", ".gitignore"];
    const filtered = paths.filter(
      (p) =>
        !ignoredNames.includes(p.name) &&
        (p.isDirectory() || isTextExtension(p.name)) &&
        gitignores.every((g) =>
          p.isDirectory() ? g.accepts(p.name + "/") : g.accepts(p.name)
        )
    );
    return filtered.map((p) => p.name);
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
