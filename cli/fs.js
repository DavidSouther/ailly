import { NodeFileSystemAdapter } from "@davidsouther/jiffies/lib/esm/fs_node.js";
import { join, normalize } from "node:path";
import * as ailly from "@ailly/core";

function cwdNormalize(path) {
  return normalize(path[0] == "/" ? path : join(process.cwd(), path));
}

export async function loadFs(args) {
  const root = cwdNormalize(args.values.root);
  const fs = new ailly.Ailly.GitignoreFs(new NodeFileSystemAdapter());
  fs.cd(root);
  const settings = {
    root,
    out: cwdNormalize(args.values.out ?? root),
    isolated: args.values.isolated,
    engine: args.values.engine,
    model: args.values.model,
    plugin: args.values.plugin,
    updateDb: args.values["update-db"],
    queryDb: args.values["query-db"],
    augment:
      args.values.augment ||
      args.values["update-db"] ||
      args.values["query-db"],
    no_overwrite: args.values["no-overwrite"],
  };
  let content = await ailly.content.load(
    fs,
    [args.values.prompt ?? ""],
    settings
  );

  const positionals =
    args.positionals.slice(2).length == 0
      ? [""]
      : args.positionals.slice(2).map(cwdNormalize);
  content = content.filter((c) =>
    positionals.some((p) => c.path.startsWith(p))
  );

  return { fs, settings, content };
}
