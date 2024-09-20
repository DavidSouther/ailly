# Ailly: AI Writing Ally

Load your writing.
Guide Ailly to your voice.
Write your outline.
Prompt Ailly to continue to keep writing.
Edit its output, and get even more text like that.

Rhymes with _daily_.

Ailly's best feature is rapid prompt engineering iteration. By keeping your prompts in snippets on the file system, you can make very fine-grained changes to your prompt and immediately see the difference in the output. You can also use all of your normal source control tooling to track changes over time: both your changes, and those from the LLM.

## VSCode extension

The VSCode extension provides several commands.

`Ailly: Run` runs Ailly on a file. You can activate it with either the right-click context menu or the command palette.

- When you run Ailly from the context menu, it runs on the file with its parent folder as the root.
- When you run Ailly from the command palette, it runs on the current active file (or does nothing if there's no active file).

`Ailly: Run All` runs Ailly on a folder and all files below it in the file system.

`Ailly: Continue` sends the response as an `Assistant` message, to have the LLM continue generating.
This is especially useful when a request ends before the LLM is 'finished', for instance, asking it to repeat and edit a long document.

`Ailly: Clean` and `Ailly: Clean All` on a file or folder clean Ailly details from the file. This includes removing debug data from the metadata of each prompt file and removing the response contents.

`Ailly: Edit` runs from the command palette on the current active file (or does nothing if there's no active file).

- `Ailly: Edit` uses the current file and its root, in the `folder` context, and includes the current highlighted selection as the edit range.
- With multiple selections, it only uses the first selection.

### Installing the Ailly extension

1. Download the extension's `.vsix` file from the [latest release](https://github.com/DavidSouther/ailly/releases/latest).
2. Open the VSCode Extensions side panel (Cmd/Ctrl + Shift + X).
3. From the Extensions side panel top three-dot menu, choose "Install from VSIX..."
4. Select the `.vsix` file downloaded in step 1.

### The Ailly extension and Bedrock

When using the Bedrock engine, the Ailly extension uses your current [AWS configuration](https://docs.aws.amazon.com/cli/latest/userguide/cli-configure-files.html).
Ailly also exposes two settings, `Ailly: AWS Profile` and `Ailly: AWS Region`, to choose which profile and (optionally) which Region to use for Bedrock requests.
Use `aws configure` or the equivalent command to configure `~/.aws/config` and `~/.aws/credentials`.
If Ailly shows a credentials error, check this configuration first.

### Ailly Getting Started

1. Install Ailly (see above).
2. Authenticate with Bedrock, or set the Ailly OpenAI API key.
3. Create a new folder, open it with VSCode, and create a file named `10_chickens.md` that contains the following text: "Tell me a joke about chickens!"
4. In the explorer view on the left, right-click `10_chickens.md` and choose `Ailly: Run`.
5. When Ailly finishes, laugh at the joke in `10_chickens.md.ailly.md`!
6. Create a file named `.aillyrc` that contains the following text: "You are a farmer writing jokes for your other barnyard animals."
7. Create a file named `20_knock_knock.md` that contains the following text: "Turn the chicken joke into a knock knock joke."
8. Right-click the folder in the Explorer view, and choose `Ailly: Run All`.
9. Review the new jokes!

## CLI Quickstart

To get started on the command line, follow these steps:

1. Ask for a joke - `npx @ailly/cli --prompt 'Tell me a joke'`
1. Create a folder named `jokes` and change directory into it.
1. Create a file named `10_chickens.md` with "Tell me a joke about chickens" as the content.
1. Run Ailly using NodeJS: `npx @ailly/cli`
   - See the joke in `10_chickens.md.ailly.md`
1. Create a file named `.aillyrc` with "You are a farmer writing jokes for your other barnyard animals."
   - Include other system prompts, level setting expectations. etc.
   - Run Ailly with the same command, and see how the joke changes.
1. Create more numbered files, such as `20_knock_knock.md` that contains teh following text: "Turn the chicken joke into a knock-knock joke."
1. Run Ailly using NodeJS: `npx @ailly/cli 20_knock_knock.md`
   - `20_knock_knock.md.ailly.md` now contains the new knock knock joke based on the updated chicken joke it wrote!

To use Ailly more easily, install the latest version with `npm install @ailly/cli`, after which the command `ailly` will run Ailly.

### System Context

System prompts provide grounding and background information for LLMs.
There are a number of techniques and "best practices" for developing LLM system prompts.
In Ailly, these are in files with the name `.aillyrc`, and apply to all files in the current folder when using Ailly.
These files can also specify properties that control how the LLM prompts are constructed, again for every file.

### Properties

Ailly generates LLM responses for one file at a time.
It prepares the conversational history and context by using the file system and folders the file is in.
There are variations in how Ailly composes the prompt, which can be controlled with several properties.

You can set these properties in a combination of locations, including the command line, `.aillyrc` files, and greymatter in each file.
Later settings override earlier settings.

- **`combined`**: `boolean` If `true`, the file's body is the response and the prompt is in the greymatter key `prompt`. If `false`, the file's body is the prompt and the response is written to `{file_name}.ailly.md`. The default is `false`.
- **`skip`**: `boolean` If `true`, the prompt is not sent through the LLM (but is part of the context).
- **`isolated`**: `boolean` If `true`, the LLM inference only includes the system prompt, and not the prior context in this folder.
- **`context`** `conversation` | `folder` | `none`
  - `conversation` (default, unless editing) loads files from the root folder and includes them alphabetically, chatbot history style, before the current file.
  - `folder` includes all files in the folder at the same level as the current file. (Default when editing.)
  - `none` includes no additional content (including no system context).
- **`parent`** `root` | `always` | `never`
  - `root` (default) start the chain of system prompts from the loaded `.aillyrc` file.
  - `always` include the .aillyrc file in the parent directory as part of this system prompt.
  - `never` don't include any other system prompts.
  - Note: `always` goes up one level, and then `parent` gets reapplied. To include several ancestors, have `parent: always` in each, with `root` as the base `.aillyrc` of the project.
- **`template-view`** `[path]` loads variables to use with [mustache](https://mustache.github.io/) templates in your prompts.

### PLAN

PLAN to use Ailly effectively. Iterate often. Provide context. Put words in Ailly's mouth.

- **Prepare** a precise prompt (by writing an aillyrc system prompt, providing supporting documents, and specifying individual prompt steps).
- **Leverage** LLM models (by running Ailly on some or all parts of the context chain).
- **Assess** the generated content (as Ailly and the LLM write output, make sure they're on the right track).
- **Narrow** your context (by editing Ailly's generated content to keep the conversation going where you want it to go).

## Engines

- OpenAI (Use `openai`)
  - [Models documented by OpenAI](https://platform.openai.com/docs/models/continuous-model-upgrades)
- Amazon Bedrock (Use `bedrock`)
  - [Claude 2 and available models documented by AWS](https://docs.aws.amazon.com/bedrock/latest/userguide/api-methods-list.html)
  - [Enable models in Bedrock console.](https://docs.aws.amazon.com/bedrock/latest/userguide/model-access.html) Remember that you must enable a model for each Region from which you call Bedrock.
  - Each model behaves differently, so the default forematter might not work for a particular model. In that case, either open an issue or poke around in `./core/src/engine/bedrock`.

To choose an engine, export `AILLY_ENGINE=[bedrock|openai]` or provide `ailly --engine` on the command line.

### Developing

* See [ARCHITECTURE.md](./ARCHITECTURE.md) for an overview of the packages and components in Ailly.
* See [DEVELOPING.md](./DEVELOPING.md) for details on how to run and debug various Ailly components.
* See [CONTRIBUTING.md](./CONTRIBUTING.md) for instructions on making a pull request. (There are no special instructions at this time.)
* See [DESIGN.md](./DESIGN.md) for historical notes on why some decisions were made. (Not exhaustive, but hopefully interesting.)

## Ailly plugins

Ailly can use plugins to provide additional data when calling the LLM models using retrieval augmented generation (RAG).

The default plugin `rag` (`AILLY_PLUGIN=rag`, `--plugin=rag`) uses a [vectra]() database in `./vectors/index.json`.

You can provide custom plugins with `--plugin=file:/absolute/path/to/plugin.mjs`.
This plugin must export a default factory function matching the [`PluginBuilder`](./core/src/plugin/index.ts) interface in `core/src/plugin/index.ts`.

## Conversational history

In LLM chat interfaces like ChatGPT or chains like Langchain, the history of the conversation remains in the sequence of interactions between the human and the assistant.

This history is typically in a format that is inaccessible to the user.
The user can only regenerate sequences, or add their next prompt at the end.

Ailly removes this limitation by using your file system as the conversational history.

You maintain full control over the sequence of prompts, including editing the LLM's response before (re)generating the next prompt so you can decide how the conversation should evolve.

By editing an LLM prompt, you can keep the best of what the LLM produced and modify the rest.
Using this filesystem-based conversational history, Ailly stores each piece of the session in source control.
With version tracking, you can see how your prompts and responses have changed over time, and unlock long-term process improvements that are difficult or even impossible with chat interfaces.

In one session, a developer was working on a long sequence of prompts to build a software project.
While reviewing an LLM-generated draft of the README, the developer wanted the list of API calls to be links to the reference documentation.
With a chat conversational history, the developer would have needed to modify the instructions for the entire prompt to encourage creating the list, rerun the generation, and hoped the rest of the README came out similarly.

Instead, with Ailly, the developer created a new file with only the list and an instruction on how to create URLs from the list items, saved it as `list.md` (with `isolated: true` in the combined head), and ran `ailly list.md`.

The LLM followed the instructions, generated just the updated list, and the developer copied that list into the original (generated) README.md.
In later prompts, the context window included the entire list of URLs, and the agent model could intelligently request to download their contents.

To the author's knowledge, no other LLM interface provides this level of interaction with LLMs.
