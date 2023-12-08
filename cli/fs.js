import { NodeFileSystem } from "@davidsouther/jiffies/lib/esm/fs_node.js";
import { join, normalize } from "node:path";
import * as ailly from "@ailly/core";

function cwdNormalize(path) {
  return normalize(path[0] == "/" ? path : join(process.cwd(), path));
}

export async function loadFs(args) {
  const root = cwdNormalize(args.values.root);
  const fs = new NodeFileSystem(root);
  const settings = {
    root: args.values.root,
    isolated: args.values.isolated,
    engine: args.values.engine,
    model: args.values.model,
    tune: args.values.tune,
    updateDb: args.values["update-db"],
    queryDb: args.values["query-db"],
    augment:
      args.values.augment ||
      args.values["update-db"] ||
      args.values["query-db"],
    no_overwrite: args.values["no-overwrite"],
  };

  if ((settings.queryDb ?? "").startsWith("file://")) {
    settings.queryDb = await fs.readFile(
      settings.queryDb.replace(/^file:\/\//, "")
    );
  }

  let content = await ailly.content.load(
    fs,
    [args.values.prompt ?? ""],
    settings
  );

  const positionals =
    args.positionals.length == 0
      ? [process.cwd()]
      : args.positionals.map(cwdNormalize);
  content = content.filter((c) =>
    positionals.some((p) => c.path.startsWith(p))
  );

  return { fs, settings, content };
}
