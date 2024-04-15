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
      context: { type: "string", default: "content", short: "c" },
      "template-view": { type: "string", default: "" },
      prompt: { type: "string", default: "", short: "p" },
      system: { type: "string", default: "", short: "s" },
      "update-db": { type: "boolean", default: false },
      "query-db": { type: "string", default: "" },
      augment: { type: "boolean", default: false },

      summary: { type: "boolean", default: false },
      yes: { type: "boolean", default: false, short: "y" },
      help: { type: "boolean", short: "h", default: false },
      version: { type: "boolean", default: false },
      "log-level": { type: "string", default: "" },
      verbose: { type: "boolean", default: false, short: "v" }
    },
  });

  // TODO assert context is content, folder, or none

  return args;
}

export function help() {
  console.log(`usage: ailly [options] [paths]
  paths:
    Folders or files to generate responses for. If unset, uses $(PWD).

  options:
    -r, --root sets base folder to search for content and system prompts.
    -s, --system sets an initial system prompt.
    -p, --prompt generate a final, single piece of content and print the response to standard out.
    -i, --isolated will start in isolated mode, generating each file separately.  Can be overridden with 'isolated: false' in .aillyrc files, and set with AILLY_ISOLATED=true environment variable.
    -o, --out specify an output folder to work with responses. Defaults to --root. Will load responses from and write outputs to here, using .ailly file extensions.
    -c, --context conversation | folder | none
      'conversation' (default) loads files from the root folder and includes them alphabetically, chatbot history style, before the current file when generating.
      'folder' includes all files in the folder at the same level as the current file when generating.
      'none' includes no additional content (including no system context) when generating.
      (note: context is separate from isolated. isolated: true with either 'content' or 'folder' will result in the same behavior with either. With 'none', Ailly will send _only_ the prompt when generating.)

    -e, --engine will set the default engine. Can be set with AILLY_ENGINE environment variable. Default is bedrock. bedrock calls AWS Bedrock. noop is available for testing. (Probably? Check the code.)
    -m, --model will set the model from the engine. Can be set with AILLY_MODEL environment variable. Default depends on the engine; bedrock is anthropic.claude-3-sonnet-20240229-v1:0, OpenAI is gpt-4-0613. (Probably? Check the code.)

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
