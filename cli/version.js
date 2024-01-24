import { execSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { normalize, join } from 'node:path';

import pkg from './package.json' assert {type: "json"};
import { version as core } from "@ailly/core";

const cli = pkg.version;

const OPTS = { cwd: normalize(join(fileURLToPath(import.meta.url), "..")) };
function run(cmd) {
  return execSync(cmd, OPTS).toString('utf-8');
}

export async function version() {
  let sha = "";
  let status = "";
  let changes = "";
  try {
    // Are there any outstanding git changes?
    sha = run("git rev-parse --short HEAD").trim();
    status = run("git status --porcelain=v1");
  } catch (e) { }
  if (sha.length > 0) changes = ` (${sha} ±${status.trim().split("\n").length})`
  console.log(`cli@${cli} core@${core}${changes}`)
}