import { getRevision, getVersion, version as core } from '@ailly/core';

export function version() {
  const cli = getVersion(import.meta.url);
  const rev = getRevision(import.meta.url)
  console.log(`cli@${cli} core@${core} ${rev}`)
}