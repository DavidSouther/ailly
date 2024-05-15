import { fileURLToPath } from "node:url";
import { join, normalize } from "node:path";
import { getRevision, getVersion } from "@ailly/core/dist/version.js";
import { version as core } from "@ailly/core/dist/index.js";

export function version() {
  let dirname: string | undefined;
  try {
    dirname = __dirname;
  } catch (e) {}
  try {
    dirname = normalize(join(fileURLToPath(import.meta.url), ".."));
  } catch (e) {}

  const cli = dirname ? getVersion(dirname) : "unknown";
  const rev = dirname ? getRevision(dirname) : "unknown";
  console.log(`cli@${cli} core@${core} ${rev}`);
}
