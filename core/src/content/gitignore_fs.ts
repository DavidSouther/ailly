import { FileSystem, SEP } from "@davidsouther/jiffies/lib/esm/fs.js";
import { join, normalize } from "path";
import { contentType } from "mime-types";
import * as gitignoreParser from "gitignore-parser";

export class GitignoreFs extends FileSystem {
  async readdir(path: string): Promise<string[]> {
    path = (this as unknown as { p(p: string): string }).p(path);
    const [drive, ...dirs] = this.cwd().split("/");
    const gitignores: Array<ReturnType<typeof gitignoreParser.compile>> = [];
    for (let i = 0; i <= dirs.length; i++) {
      const gitignorePath = normalize(
        drive + SEP + join(...dirs.slice(0, i), ".gitignore")
      );
      const gitignore = await this.readFile(gitignorePath).catch((e) => "");
      const parser = gitignoreParser.compile(gitignore);
      gitignores.push(parser);
    }
    const paths = await this.adapter.scandir(path);
    const filtered = paths.filter(
      (p) =>
        p.name !== ".git" &&
        isTextExtension(p.name) &&
        gitignores.every((g) =>
          p.isDirectory() ? g.accepts(p.name + "/") : g.accepts(p.name)
        )
    );
    return filtered.map((p) => p.name);
  }
}

function isTextExtension(name: string) {
  const contType = contentType(name) || "";
  return contType.startsWith("text") || contType.startsWith("application");
}
