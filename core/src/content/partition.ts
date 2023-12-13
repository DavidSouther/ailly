import { dirname } from "node:path";

import type { Content } from "./content";

export function partitionPrompts(content: Content[]): Content[][] {
  const directories = new Map<string, Content[]>();
  for (const c of content) {
    const prefix = dirname(c.path);
    const entry = directories.get(prefix) ?? [];
    entry.push(c);
    directories.set(prefix, entry);
  }

  for (const thread of directories.values()) {
    thread.sort((a, b) => a.name.localeCompare(b.name));
    if (!Boolean(thread.at(0)?.meta?.["isolated"])) {
      for (let i = thread.length - 1; i > 0; i--) {
        thread[i].predecessor = thread[i - 1];
      }
    }
  }

  return [...directories.values()];
}
