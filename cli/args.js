import { parseArgs } from "node:util";
import { NodeFileSystem } from "@davidsouther/jiffies/lib/esm/fs_node.js";
import { join, normalize } from "node:path";
import * as ailly from "@ailly/core";

const args = parseArgs({
  allowPositionals: true,
  options: {
    root: {
      type: "string",
      short: "r",
      default: process.cwd(),
    },
    prompt: { type: "string", default: "", short: "p" },
    "no-overwrite": {
      type: "boolean",
      default: false,
    },
    engine: {
      type: "string",
      short: "e",
      default: process.env["AILLY_ENGINE"],
    },
    model: {
      type: "string",
      short: "m",
      default: process.env["AILLY_MODEL"],
    },
    isolated: {
      type: "boolean",
      short: "i",
      default: Boolean(process.env["AILLY_ISOLATED"]),
    },
    summary: { type: "boolean", default: false, short: "s" },
    tune: { type: "boolean", default: false },
    yes: { type: "boolean", default: false, short: "y" },
    help: { type: "boolean", short: "h", default: false },
  },
});

function cwdNormalize(path) {
  return normalize(path[0] == "/" ? path : join(process.cwd(), path));
}

const root = cwdNormalize(args.values.root);
const fs = new NodeFileSystem(root);
const settings = {
  isolated: args.values.isolated,
  engine: args.values.engine,
  model: args.values.model,
  tune: args.values.tune,
};
let content = await ailly.content.load(
  fs,
  [args.values.prompt ?? ""],
  settings
);

const positionals =
  args.positionals.length == 0
    ? [process.cwd()]
    : args.positionals.map(cwdNormalize);
content = content.filter((c) => positionals.some((p) => c.path.startsWith(p)));

export { fs, content, settings, args };

function help() {
  console.log(`usage: ailly [options] [paths]
  paths:
    Folders or files to generate responses for. If unset, uses $(PWD). 

  options:
    -r, --root sets base folder to search for additional system prompts.
    -p, --prompt sets an initial system prompt.
    -i, --isolated will start in isolated mode, generating each file separately.  Can be overridden with 'isolated: false' in .aillyrc files, and set with AILLY_ISOLATED=true environment variable.

    -e, --engine will set the default engine. Can be set with AILLY_ENGINE environment variable. Default is OpenAI. (Probably? Check the code.)
    -m, --model will set the model from the engine. Can be set with AILLY_MODEL environment variable. Default is gpt-4-0613. (Probably? Check the code.)

    --tune will start a new fine tuning job using the engine and model selected.

    --no-overwrite will not run generation on Content with an existing Response. (NOT YET IMPLEMENTED.)
    -s, --summary will show a pricing expectation before running and prompt for OK.
    -y, —yes will skip any prompts.

    -h, --help will print this message and exit.
  `);
}

if (args.values["help"]) {
  help();
  process.exit(1);
}
