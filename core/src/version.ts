// TODO move this to jiffies
import { execSync } from "node:child_process";
import { normalize, join } from "node:path";
import { readFileSync } from "node:fs";

export function getVersion(...root: string[]) {
  try {
    const cwd = normalize(join(...root));
    const packageJson = join(cwd, "./package.json");
    const pkg = JSON.parse(readFileSync(packageJson, { encoding: "utf8" }));
    return pkg.version;
  } catch (_) {
    return "unknown";
  }
}

export function getRevision(...root: string[]) {
  const cwd = normalize(join(...root));
  let sha = "";
  let status = "";
  let changes = "";
  try {
    // Are there any outstanding git changes?
    const run = (cmd: string) =>
      execSync(cmd, { encoding: "utf-8", cwd, stdio: "pipe" });
    sha = run("git rev-parse --short HEAD").trim();
    status = run("git status --porcelain=v1");
  } catch (e) {}
  if (sha.length > 0)
    changes = ` (${sha} ±${status.trim().split("\n").length})`;
  return changes;
}
