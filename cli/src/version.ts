import { getRevision, getVersion } from "@ailly/core/lib/version.js";
import { version as core } from "@ailly/core/lib/ailly.js";

export function version() {
  const cli = getVersion(__dirname);
  const rev = getRevision(__dirname);
  console.log(`cli@${cli} core@${core} ${rev}`);
}
