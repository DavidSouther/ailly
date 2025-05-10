import { join, normalize } from "node:path";
import {
  FileSystem,
  SEP,
  type Stats,
} from "@davidsouther/jiffies/lib/cjs/fs.js";
import * as gitignoreParser from "gitignore-parser";
import {
  BINARY_EXTENSIONS,
  IGNORED_NAMES,
  SIGNATURES,
  TEXT_EXTENSIONS,
} from "./gitignore_fs_constants.js";

export class GitignoreFs extends FileSystem {
  async readdir(path: string): Promise<string[]> {
    // biome-ignore lint/style/noParameterAssign: update path based on CWD
    path = (this as unknown as { p(p: string): string }).p(path);
    const [drive, ...dirs] = path.split("/");
    const gitignores: Array<{
      path: string;
      gitignore: string;
      accepts: ReturnType<typeof gitignoreParser.compile>["accepts"];
    }> = [];
    for (let i = 0; i <= dirs.length; i++) {
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
          (stats.isDirectory() || (await this.isTextFile(path, stats))) && // This test was intended to limit us to text files only
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

  private async isTextFile(path: string, p: Stats): Promise<boolean> {
    const ext = p.name.substring(p.name.lastIndexOf(".")).toLowerCase();
    if (TEXT_EXTENSIONS.has(ext)) return true;
    if (BINARY_EXTENSIONS.has(ext)) return false;

    try {
      const content = await this.readFile(join(path, p.name));

      if (hasMagicSignature(content)) {
        return false;
      }

      return !isBinByControlChars(content);
    } catch (e) {
      return false;
    }
  }
}

function hasMagicSignature(content: string) {
  return SIGNATURES.some((prefix) => content.startsWith(prefix));
}

function isBinByControlChars(
  sample: string,
  {
    sampleSize,
    controlCharThreshold,
    extendedThreshold,
  }: {
    sampleSize?: number;
    controlCharThreshold?: number;
    extendedThreshold?: { base: number; control: number };
  } = {},
): boolean {
  // Check for high percentage of control or non-printable characters
  // Sample the first N bytes to save performance on large files
  sampleSize = Math.min(sample.length, sampleSize ?? 512);

  let controlChars = 0;
  let extendedChars = 0;

  for (let i = 0; i < sampleSize; i++) {
    const charCode = sample.charCodeAt(i);

    // null character is immediately binary
    if (charCode === 0) {
      return true;
    }

    // Control characters (except common whitespace)
    if (
      (charCode < 32 &&
        // Tab, newline, carriage return
        charCode !== 9 &&
        charCode !== 10 &&
        charCode !== 13) ||
      charCode === 127 // del
    ) {
      controlChars++;
    }

    // Extended ASCII and beyond - not necessarily binary but can be part of heuristic
    if (charCode > 127) {
      extendedChars++;
    }
  }

  // Calculate percentages
  const controlCharPercentage = (controlChars / sampleSize) * 100;
  const extendedCharPercentage = (extendedChars / sampleSize) * 100;

  // If more than 10% are control characters, likely binary
  controlCharThreshold = controlCharThreshold ?? 10;
  if (controlCharPercentage > controlCharThreshold) {
    return true;
  }

  // Special handling for extended characters to avoid misclassifying non-English text
  // as binary. If we have many extended chars but few control chars, it's likely
  // a text file in a non-ASCII encoding (like UTF-8 for Chinese, Japanese, etc.)
  extendedThreshold = extendedThreshold ?? { base: 30, control: 5 };
  if (
    extendedCharPercentage > extendedThreshold.base &&
    controlCharPercentage > extendedThreshold.control
  ) {
    return true;
  }

  return false;
}
