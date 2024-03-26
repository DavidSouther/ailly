import { NodeFileSystemAdapter } from "@davidsouther/jiffies/lib/esm/fs_node.js";
import { DEFAULT_LOGGER } from "@davidsouther/jiffies/lib/esm/log.js";
import { dirname, resolve } from "node:path";
import * as ailly from "@ailly/core";

export async function loadFs(args) {
  const root = resolve(args.values.root ?? '.');
  const fs = new ailly.Ailly.GitignoreFs(new NodeFileSystemAdapter());
  fs.cd(root);
  const settings = {
    root,
    out: resolve(args.values.out ?? root),
    isolated: args.values.isolated,
    engine: args.values.engine,
    model: args.values.model,
    plugin: args.values.plugin,
    updateDb: args.values["update-db"],
    queryDb: args.values["query-db"] ?? "",
    augment:
      args.values.augment ||
      args.values["update-db"] ||
      args.values["query-db"],
    overwrite: !args.values["no-overwrite"],
  };
  const positionals = args.positionals.slice(2).map(a => resolve(a));
  const isPipe = positionals.length == 0 && args.values.prompt;
  DEFAULT_LOGGER.level = isPipe ? 100 : 0;

  let content = await ailly.content.load(
    fs,
    [args.values.prompt ?? ""],
    settings
  );

  if (isPipe) {
    content.forEach(c => { c.meta = c.meta ?? {}; c.meta.skip = true; });
    content.push({ name: 'stdout', outPath: "/dev/stdout", path: "/dev/stdout", prompt: args.values.prompt, predecessor: content.filter(c => dirname(c.path) == root).at(-1) })
  } else {
    if (positionals.length == 0) positionals.push(root);
    content = content.filter((c) =>
      positionals.some((p) => c.path.startsWith(p))
    );
  }

  return { fs, settings, content };
}
