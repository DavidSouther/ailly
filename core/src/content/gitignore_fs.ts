import { FileSystem } from "@davidsouther/jiffies/lib/esm/fs.js";
import { join, normalize } from "path";
import * as gitignoreParser from "gitignore-parser";

export class GitignoreFs extends FileSystem {
  async readdir(path: string): Promise<string[]> {
    path = (this as unknown as { p(p: string): string }).p(path);
    const dirs = this.cwd().split("/");
    const gitignores: Array<ReturnType<typeof gitignoreParser.compile>> = [];
    for (let i = 1; i <= dirs.length; i++) {
      const gitignorePath = normalize(
        "/" + join(...dirs.slice(0, i), ".gitignore")
      );
      const gitignore = await this.readFile(gitignorePath).catch((e) => "");
      const parser = gitignoreParser.compile(gitignore);
      gitignores.push(parser);
    }
    const paths = await this.adapter.readdir(path);
    const filtered = paths.filter(
      (p) => p !== ".git" && gitignores.every((g) => g.accepts(p))
    );
    return filtered;
  }
}
