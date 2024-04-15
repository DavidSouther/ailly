import { parseArgs } from "node:util";

export function makeArgs(argv = process.argv) {
  const args = parseArgs({
    args: argv,
    allowPositionals: true,
    options: {
      root: {
        type: "string",
        short: "r",
        default: process.cwd(),
      },
      out: {
        type: "string",
        short: "o",
      },
      isolated: {
        type: "boolean",
        short: "i",
        default: false,
      },
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
      plugin: {
        type: "string",
        default: process.env["AILLY_PLUGIN"] ?? "noop",
      },
      "template-view": { type: "string", default: "" },
      prompt: { type: "string", default: "", short: "p" },
      "update-db": { type: "boolean", default: false },
      "query-db": { type: "string", default: "" },
      augment: { type: "boolean", default: false },

      summary: { type: "boolean", default: false, short: "s" },
      yes: { type: "boolean", default: false, short: "y" },
      help: { type: "boolean", short: "h", default: false },
      version: { type: "boolean", default: false },
      "log-level": { type: "string", default: "" },
      verbose: { type: "boolean", default: false, short: "v" }
    },
  });

  return args;
}

export function help() {
  console.log(`usage: ailly [options] [paths]
  paths:
    Folders or files to generate responses for. If unset, uses $(PWD).

  options:
    -r, --root sets base folder to search for content and system prompts.
    -p, --prompt sets an initial system prompt. If no paths are specified, --prompt will generate a single piece of content with the local folder as context and print the response to standard out.
    -i, --isolated will start in isolated mode, generating each file separately.  Can be overridden with 'isolated: false' in .aillyrc files, and set with AILLY_ISOLATED=true environment variable.
    -o, --out specify an output folder to work with responses. Defaults to --root. Will load responses from and write outputs to here, using .ailly file extensions.

    -e, --engine will set the default engine. Can be set with AILLY_ENGINE environment variable. Default is OpenAI. (Probably? Check the code.)
    -m, --model will set the model from the engine. Can be set with AILLY_MODEL environment variable. Default depends on the engine; OpenAI is gpt-4-0613, bedrock is anthropic-claude-3. (Probably? Check the code.)

    --plugin can load a custom RAG plugin. Specify a path to import with "file://./path/to/plugin.mjs". plugin.mjs must export a single default function that meets the PluginBuilder interface in core/src/plugin/index.ts
    --template-view loads a YAML or JSON file to use as a view for the prompt templates. This view will be merged after global, engine, and plugin views but before system and template views.

    --no-overwrite will not run generation on Content with an existing Response.
    --summary will show a pricing expectation before running and prompt for OK.
    -y, —-yes will skip any prompts.
    -v, --verbose, --log-level v and verbose will set log level to info; --log-level can be a string or number and use jefri/jiffies logging levels.

    --version will print the cli and core versions
    -h, --help will print this message and exit.
  `);

  // --update-db will create and update a Vectra database with the current content. When available, a local Vectra db will augment retrieval data.
  // --augment will look up augmentations in the db.
}
