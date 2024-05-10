import { basename } from "node:path";
import { parseArgs } from "node:util";

export function makeArgs(argv = process.argv) {
  const args = parseArgs({
    args: argv,
    allowPositionals: true,
    options: {
      root: { type: "string", default: process.cwd(), short: "r" },
      out: { type: "string", short: "o" },
      isolated: { type: "boolean", default: false, short: "i" },
      combined: { type: "boolean", default: false },
      "no-overwrite": { type: "boolean", default: false },
      edit: { type: 'boolean', default: false, short: 'e' },
      lines: { type: 'string', default: "", short: 'l' },
      engine: { type: "string", default: process.env["AILLY_ENGINE"] },
      model: { type: "string", default: process.env["AILLY_MODEL"] },
      plugin: { type: "string", default: process.env["AILLY_PLUGIN"] ?? "noop", },
      context: { type: "string", default: process.env["AILLY_CONTEXT"], short: "c" },
      "template-view": { type: "string", default: process.env["AILLY_TEMPLATE_VIEW"] ? [process.env["AILLY_TEMPLATE_VIEW"]] : [], multiple: true },
      prompt: { type: "string", default: process.env["AILLY_PROMPT"], short: "p" },
      system: { type: "string", default: process.env["AILLY_SYSTEM"], short: "s" },
      "request-limit": { type: "string", default: process.env["AILLY_REQUEST_LIMIT"] },
      "max-depth": { type: "string", default: "1" },
      temperature: { type: "string", default: "" },
      "update-db": { type: "boolean", default: false },
      "query-db": { type: "string", default: "" },
      augment: { type: "boolean", default: false },
      summary: { type: "boolean", default: false },
      yes: { type: "boolean", default: false, short: "y" },
      help: { type: "boolean", short: "h", default: false },
      version: { type: "boolean", default: false },
      "log-level": { type: "string", default: undefined },
      "log-format": { type: "string", default: undefined },
      verbose: { type: "boolean", default: false, short: "v" },
    },
  });

  // TODO assert context is content, folder, or none
  // TODO assert log-format is pretty, json, or empty

  // Remove node and ailly positionals
  args.positionals.splice(0, args.positionals[0].match(/node(\.exe)?/) ? 2 : 1)

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
    -i, --isolated will start in isolated mode, generating each file separately. Can be overridden with 'isolated: false' in .aillyrc files.
    --combined will force files to output as combined.
    -o, --out specify an output folder to work with responses. Defaults to --root. Will load responses from and write outputs to here, using .ailly file extensions.
    -c, --context conversation | folder | none
      'conversation' (default, unless --edit) loads files from the root folder and includes them alphabetically, chatbot history style, before the current file when generating.
      'folder' includes all files in the folder at the same level as the current file when generating. Default with --edit.
      'none' includes no additional content (including no system context) when generating.
      (note: context is separate from isolated. isolated: true with either 'content' or 'folder' will result in the same behavior with either. With 'none', Ailly will send _only_ the prompt when generating.)

    -e, --edit use Ailly in edit mode. Provide a single file in paths, an edit marker, and a prompt. The path will be updated with the edit marker at the prompt.
    -l, --lines the lines to edit as '[start]:[end]' with start inclusive, and end exclusive. With only '[start]', will insert after. With only ':[end]', will insert before.

    --engine will set the default engine. Can be set with AILLY_ENGINE environment variable. Default is bedrock. bedrock calls AWS Bedrock. noop is available for testing. (Probably? Check the code.)
    --model will set the model from the engine. Can be set with AILLY_MODEL environment variable. Default depends on the engine; bedrock is anthropic.claude-3-sonnet-20240229-v1:0, OpenAI is gpt-4-0613. (Probably? Check the code.)
    --temperature for models that support changing the stochastic temperature. (Usually between 0 and 1, but check the engine and model.)

    --plugin can load a custom RAG plugin. Specify a path to import with "file://./path/to/plugin.mjs". plugin.mjs must export a single default function that meets the PluginBuilder interface in core/src/plugin/index.ts
    --template-view loads a YAML or JSON file to use as a view for the prompt templates. This view will be merged after global, engine, and plugin views but before system and template views.
    --request-limit will limit the number of requests per call to the provided value. Default value is 5, except for Opus with a default of 1.
    --max-depth will allow loading content below the current root. Default 1, or only the root folder. 0 or negative numbers will load no content.

    --no-overwrite will not run generation on Content with an existing Response.
    --summary will show a pricing expectation before running and prompt for OK.
    -y, —-yes will skip any prompts.
    -v, --verbose, --log-level v and verbose will set log level to info; --log-level can be a string or number and use jefri/jiffies logging levels. Ailly uses warn for reporting details on errors, info for general runtime progress, and debug for details of requests and responses.
    --log-format json or pretty; default is pretty when run in a pipe. JSON prints in JSONL format.

    --version will print the cli and core versions
    -h, --help will print this message and exit.

    Engines:

    bedrock - Call LLM models using @aws-sdk/bedrock-runtime. While this can use any model available in bedrock, in practice, because of the difference in prompt APIs, Claude3 is the only currently supported model.
    openai - Call ChatGPT models using OpenAI's API.
    mistral - Attempt to run Mistral 7B instruct locally, using a Python subshell.
    noop - A testing model that returns with constant text (either a nonce with the name of the file, or the contents of the AILLY_NOOP_RESPONSE environment variable).
  `);

  // -n, --section use LLM + TreeSitter to find line numbers.
  // --kb
  // --sync will create and update a Vectra database with the current content. When available, a local Vectra db will augment retrieval data.
  // --augment will look up augmentations in the db.
}
