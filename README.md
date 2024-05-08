# Ailly - AI Writing Ally

Load your writing.
Guide Ailly to your voice.
Write your outline.
Prompt Ailly to continue to continue the writing.
Edit its output, and get even more like that.

Rhymes with Daily.

Ailly's best feature is rapidly iterating on prompt engineering. By keeping your prompts in snippets on the file system, you can make very fine-grained changes to your prompt and immediately see the difference. You can also use all your normal source control tooling to track changes over time - both your changes, and what the LLM does.

## Quickstart

1. Create a folder, `content`.
2. Create a file, `content/.aillyrc`, and put your top-level prompt instructions.
   - Include system prompts, level setting expectations. etc.
3. Create several files, `content/01_big_point.md`, `content/02_second_point.md` etc.
4. Run ailly using NodeJS: `npx @ailly/cli --root content`

### Properties

These properties can be set in a combination of places, including the command line, .aillyrc, and greymatter. Later settings override earlier.

- **`combined`** `boolean` If true, the file's body is the response and the prompt is in the greymatter key `prompt`. If false, the file's body is the prompt and the response is in `{file_name}.ailly.md`. Default false.
- **`skip`** `boolean` If true, the prompt will not be sent through the LLM (but it will be part of the context).
- **`isolated`** `boolean` If true, the LLM inference will only include the system prompt, and not the prior context in this folder.
- **`context`** `conversation` | `folder` | `none`
  - `conversation` (default) loads files from the root folder and includes them alphabetically, chatbot history style, before the current file when generating.
  - `folder` includes all files in the folder at the same level as the current file when generating.
  - `none` includes no additional content (including no system context) when generating.
  - (note: context is separate from isolated. isolated: true with either 'content' or 'folder' will result in the same behavior with either. With 'none', Ailly will send _only_ the prompt when generating.)
- **`template-view`** `[path]` loads variables to use with [mustache](https://mustache.github.io/) templates in your prompts.

### PLAN

PLAN to use Ailly effectively. Iterate often. Provide context. Put words in Ailly's mouth.

- **Prepare** a precise prompt (by writing an aillyrc system prompt, providing supporting documents, and giving individual prompt steps).
- **Leverage** LLM models (by running Ailly on some or all parts of the context chain).
- **Assess** the generated content (as Ailly and the LLM writes output, make sure it's on the right track).
- **Narrow** your context (by editing Ailly's generated content to keep the conversation going where you want it to).

## Engines

- OpenAI `openai`
  - [Models documented by OpenAI](https://platform.openai.com/docs/models/continuous-model-upgrades)
- Bedrock `bedrock`
  - [Claude 2 and available models documented by AWS](https://docs.aws.amazon.com/bedrock/latest/userguide/api-methods-list.html)
  - [Enable models in Bedrock console.](https://docs.aws.amazon.com/bedrock/latest/userguide/model-access.html) Remember that models are enabled per-region - you will need to enable them for each reach you call Bedrock from.
  - Each model behaves lightly differently, so the default formatter might not work. In that case, either open an issue or poke around in `./core/src/engine/bedrock`.

To choose an engine, export `AILLY_ENGINE=[bedrock|openai]` or provide `ailly --engine` on the command line.

## Installing Ailly Extension

- Clone the repo and install dependencies
  - `git clone https://github.com/davidsouther/ailly.git ; cd ailly ; npm install`
- Package the extension with `npm run package`
- In VSCode extensions, install `./extension/ailly-0.*.*.vsix` from vsix.
- Right click a file in content explorer and select `Ailly: Generate`
- Make a selection and edit with `Ailly: Edit` in the command palette.

Update May 8 2024: the extension is also available as an artifact in the most recent [workflow run](https://github.com/DavidSouther/ailly/actions/workflows/extension.yaml).

### Developing

See [DEVELOPING.md](./DEVELOPING.md) for details on how to run and debug various Ailly components.

## Ailly Plugins

Ailly can use plugins to provide additional data when calling the LLM models (retrieval augmented generation, or RAG).
The default plugin `rag` (`AILLY_PLUGIN=rag`, `--plugin=rag`) uses a [vectra]() database in `./vectors/index.json`.
You can provide custom plugins with `--plugin=file:/absolute/path/to/plugin.mjs`.
This plugin must export a default factory function matching the [`PluginBuilder`](./core/src/plugin/index.ts) interface in `core/src/plugin/index.ts`.

## Conversational History

In LLM Chatbots like ChatGPT or chains like Langchain, the history of the conversation is kept in the sequence of human, assistant interactions.
This is typically kept in memory, or at least in an inaccessible format to the user.
The user can only regenerate sequences, or add their next prompt at the end.

Ailly removes this limitation by using your file system as the conversational history.
The writer maintains full control over the sequence of prompts, up to and including editing the LLM's response before (re)generating the next prompt!
This lets the writer decide how the conversation should evolve.
By editing an LLM prompt, they can take the best of what the LLM did in some cases, and modify it in others.
Using Ailly's filesystem based conversational history, each piece of the session can be stored in source control.
Version tracking lets the author see how their prompts and responses have changed over time, and unlock a number of long-term process improvements that are difficult to impossible with chat interfaces.

In one session, a developer was working on a long sequence of prompts to build a software project.
While reviewing an LLM written draft of the README, the developer wanted the list of API calls to be links to the reference documentation.
With a chat conversational history, the developer would have needed to modify the instructions for the entire prompt to encourage creating the list, rerun the generation, and hope the rest of the README came out similarly.
Instead, with Ailly, the developer created a new file with only the list and an instruction on how to create URLs from the list items, saved it as `list.md` (with `isolated: true` in the combined head), and ran `ailly list.md`.
The LLM followed the instructions, generated just the updated list, and the developer copied that list into the original (generated) README.md.
In later prompts, the context window included the entire URLs, and the agent model was able to intelligently request to download their contents.

To the author's knowledge, no other LLM interface provides this level of interaction with LLMs.
