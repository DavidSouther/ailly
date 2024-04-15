import { dirname } from "node:path";

import type { Content } from "./content";

export function partitionPrompts(
  content: string[],
  context: Record<string, Content>
): Content[][] {
  const directories = new Map<string, Content[]>();
  for (const c of content) {
    if (!context[c]) continue;
    const prefix = dirname(c);
    const entry = directories.get(prefix) ?? [];
    entry.push(context[c]);
    directories.set(prefix, entry);
  }

  for (const thread of directories.values()) {
    thread.sort((a, b) => a.name.localeCompare(b.name));
    if (!Boolean(thread.at(0)?.meta?.["isolated"])) {
      for (let i = thread.length - 1; i > 0; i--) {
        thread[i].context.predecessor = thread[i - 1].name;
      }
    }
  }

  return [...directories.values()];
}
