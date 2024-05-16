import { version as core } from "@ailly/core/lib/index.js";
import { getRevision, getVersion } from "@ailly/core/lib/version.js";

export function version() {
  let dirname: string | undefined;
  try {
    dirname = __dirname;
  } catch (e) {}

  const cli = dirname ? getVersion(dirname) : "unknown";
  const rev = dirname ? getRevision(dirname) : "unknown";
  console.log(`cli@${cli} core@${core} ${rev}`);
}
