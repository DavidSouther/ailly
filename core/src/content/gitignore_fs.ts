import { FileSystem, SEP } from "@davidsouther/jiffies/lib/cjs/fs.js";
import { join, normalize } from "path";
import { isText } from "istextorbinary";
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
    const nameFilter = [".gitignore", ".git"];
    const filtered = paths.filter(
      (p) =>
        !nameFilter.includes(p.name) &&
        (p.isDirectory() || isText(p.name)) &&
        gitignores.every((g) =>
          p.isDirectory() ? g.accepts(p.name + "/") : g.accepts(p.name)
        )
    );
    return filtered.map((p) => p.name);
  }
}
