# Ailly - AI Writing Ally

Load your writing.
Train Ailly on your voice.
Write your outline.
Prompt Ailly to continue to continue the writing.
Edit its output, and retrain to get it even more like that.

Rhymes with Daily.

## Quickstart

1. Create a folder, `content`.
2. Create a file, `content/.aillyrc`, and put your top-level prompt instructions.
3. Create several files, `content/01_big_point.md`, `content/02_second_point.md` etc.
4. Run ailly using NodeJS: `npx @ailly/cli@1.1.1 --root content`

## Engines

- OpenAI `openai`
  - [Models documented by OpenAI](https://platform.openai.com/docs/models/continuous-model-upgrades)
- Bedrock `bedrock`
  - [Claude 2 and available models documented by AWS](https://docs.aws.amazon.com/bedrock/latest/userguide/api-methods-list.html)
  - Enable models in Bedrock console. Remember that models are enabled per-region - you will need to enable them for each reach you call Bedrock from.
  - Each model behaves lightly differently, so the default formatter might not work. In that case, either open an issue or poke around in `./core/src/engine/bedrock`.

To choose an engine, export `AILLY_ENGINE=[bedrock|openai]` or provide `ailly --engine` on the command line.

## Installing ailly command line

- Clone the repo and install dependencies
  - `git clone git://github.com/davidsouther/ailly.git ; cd ailly ; npm install`
- Compile the core module with `npx tsc -p core`
- Install ailly cli with `npm install -g ./cli`
- Set any environment variables for your engine
  - `export OPENAI_API_KEY=sk-...`
  - `export AILLY_ENGINE=bedrock` default: openai, others depending on version.
- Run ailly with `npx ailly`
- Optionally, create an alias to run ailly
  - Directly with `alias ailly="$(PWD)/cli/index.js`
  - For zsh: `echo "alias ailly='$(PWD)/cli/index.js'" >> ~/.zshrc`
  - For bash: `echo "alias ailly='$(PWD)/cli/index.js'" >> ~/.bashrc`
  - General \*nix: `echo "alias ailly='$(PWD)/cli/index.js'" >> ~/.profile`

## Running Ailly Web

This is powered by [Next.js](https://nextjs.org/) using App Router.

- Clone the repo, install dependencies, and duplicate the env file for local keys.
  - `git clone git://github.com/davidsouther/ailly.git ; cd ailly ; npm install ; cp .env .env.local`
- Update `.env.local` with your [OpenAI API key](https://platform.openai.com/account/api-keys).
- Clear out the `content/` folder, and replace it with your writing.
  - Open the `content` folder and see the example system and prompt files.
  - Conversations are broken into two files, `<nn>p_<name>.md` and `<nn>r_<name>.md`.
  - The `<nn>p_<name>.md` is given to the AI, and the response is written to `<nn>r_<name>.md`
  - Files in the same folder are part of a single conversation, and kept in order by sorting based on the numeric values of `<nn>`.
  - The `<name>` is for you to keep a file name in mind.
  - See [Specification](./SPECIFICATION.md) for further details on how the `content` folder is organized.
  - TODO: Provide content importers for email
  - TODO: Provide instructions for "best practices" creating
- Start the project locally with `npm run dev --workspace packages/web`
  - Default at http://localhost:3000
- Visit the [`/content`](http://localhost:3000/content) route.
  - Generate all prompts with the "Generate all" button.
    - The token count is an estimate and the pricing is advisory; there are no limits placed on token size during the API request.
- Prepare the project for deployment with `npm run export`
- Good luck!

## Installing Ailly Extension

- Clone the repo and install dependencies
  - `git clone git://github.com/davidsouther/ailly.git ; cd ailly ; npm install`
- Package the extension with `npm run package`
- In VSCode extensions, install `./extension/ailly-0.0.1.vsix` from vsix.
- Right click a file in content explorer and select `Ailly: Generate`

### Running the Extension in Dev Mode

1. Run `npx tsc -w -p core`
1. Start the `Run Ailly Extension` task
   1. Choose `tsx: watch - extension/tsconfig.json`
1. When the new window appears, open a folder with your content.
   - Ailly currently only works on the first folder in a [workspace](https://code.visualstudio.com/docs/editor/workspaces).
   - In debug mode, Ailly disables other extensions and runs in a clean profile. Comment out the `"--profile-temp",` line in `launch.json` to use your current VSCode profile instead.
   - WARNING: Ailly may prompt for your OpenAI API Key. If it does so, it will store the key in workspace settings for later access. Do not commit this key to source control!
   - If you don't like that behavior, ensure you have `OPENAI_API_KEY` set in your environment for VSCode.
   - Or send a PR with a better way to load the key safely.
1. Right Click a file or folder -> Ailly: Generate
1. Open a file -> Cmd+P -> Ailly: Generate

## Ailly Plugins

Ailly can use plugins to provide additional data when calling the LLM models (retrieval augmented generation, or RAG).
The default plugin `rag` (`AILLY_PLUGIN=rag`, `--plugin=rag`) uses a [vectra]() database in `./vectors/index.json`.
You can provide custom plugins with `--plugin=file:/absolute/path/to/plugin.mjs`.
This plugin must export a default factory function matching the [`PluginBuilder`](./core/src/plugin/index.ts) interface in `core/src/plugin/index.ts`.

## Cookbook

### Writing Assistant

Create an outline of a larger document or written work.
Create a folder for the content, say `campaign`.
In this folder, create a file `.aillyrc`, with a high level description of the work.
This will be used as part of the **system** prompt for every request, so typical prompt engineering practices work well.
For instance, "You are a fantasy tabletop roleplaying game master. The book is a campaign for a high-magic, high-fantasy setting."

Within this folder, create additional folders for each _chapter_ sized section of the outline.
Within each of those folders, create another `.aillyrc` file with a description of that section.
Continuing the RPG campaign example, this could be "Chapter 1 describes the overall setting. This world is called MyLandia. It is one large continent with two political factions, a republic and an empire."

Then, create one file for each _section_.
Each section will be an individually generated response from the LLM.
Files will be generated in their alphabetical order, so files (and folders) should generally use two-digit alphanumeric prefixes to determine their sort order.
For typical writing projects, this will be a file in markdown syntax with an optional YAML frontmatter block.
Add YAML frontmatter with a property, `prompt`, which will be the **human** prompt to the LLM.

`10_republic.md`:

```
  ---
  prompt: >
    The Republic is an aristocratic society founded on a shifting structure of familial wealth.
    Families elect one member to the ruling senate.
    The senate elects a consol for 18 month terms.
    Describe more about the Republic's politics and society.
  ---
```

`20_empire.md`:

```
  ---
  prompt: >
    The Empire is a strict military hierarchy across their entire society.
    Denizens are not required to join the military, but that is the only path to citizenship.
    Leadership is meritocratic, with strictly defined qualification criteria for promotion.
    Describe more about the Empire's politics and society.
  ---
```

After creating several chapter folders and section files, your content is ready for a first round of Ailly.
From the command line, `cd` into the content folder. Then, run ailly:

```
$ npx ailly
```

You should see output similar to this:

```
Loading content from /.../70_mylandia
Loading content from /.../70_mylandia/10_setting
Found 2 at or below /.../70_mylandia/10_setting
Found 2 at or below /.../70_mylandia
Generating...
Ready to generate 1 messages
Running thread for sequence of 2 prompts
Calling openai [
  {
    role: 'system',
    content: 'You are a fantasy tabletop roleplaying game master...',
    tokens: undefined
  },
  {
    role: 'user',
    content: 'The Republic is an aristocratic society founded on...',
    tokens: undefined
  }
]
...
```

Review the generated files. Make any edits you want. Then, if you want to rerun just one file (based on, say, changing its prompt or editing earlier responses), provide that path on the command line.

```
$ npx ailly 10_setting/20_empire.md

Loading content from /.../70_mylandia
Loading content from /.../70_mylandia/10_setting
Found 2 at or below /.../70_mylandia/10_setting
Found 2 at or below /.../70_mylandia
Generating...
Ready to generate 1 messages
Running thread for sequence of 1 prompts
Calling openai [
  {
    role: 'system',
    content: 'You are a fantasy tabletop roleplaying game master...',
    tokens: undefined
  },
  {
    role: 'user',
    content: 'The Republic is an aristocratic society founded on...',
    tokens: undefined
  },
  {
    role: 'assistant',
    content: 'The Republic, known officially as the Resplendent ...',
    tokens: undefined
  },
  {
    role: 'user',
    content: 'The Empire is a strict military hierarchy across t...',
    tokens: undefined
  }
]
Response from OpenAI for 20_empire.md { id: 'chatcmpl-1234', finish_reason: 'stop' }
```

### How many files or folders?

This is a bit of an experimental / trial by doing issue.
Generally, late 2024 LLMs (Llama, Claude, ChatGPT) respond with "around" 700 words.
Similarly, while their context sizes vary, they seem to work best with "up to" 16,000 words.
Based on these limits, up to about 5 layers of folder depth and at most 20 files per folder generate the most reliably "good" results, but your milage will vary.
