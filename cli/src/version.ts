import { version as core } from "@ailly/core/dist/index.js";
import { getRevision, getVersion } from "@ailly/core/dist/version.js";
import { join, normalize } from "node:path";
import { fileURLToPath } from "node:url";

export function version() {
  let dirname: string | undefined;
  try {
    dirname = __dirname;
  } catch (e) {}

  const cli = dirname ? getVersion(dirname) : "unknown";
  const rev = dirname ? getRevision(dirname) : "unknown";
  console.log(`cli@${cli} core@${core} ${rev}`);
}
