import { dirname } from "node:path";

import type { Content } from "./content.js";

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

  return [...directories.values()];
}
