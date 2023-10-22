# Ailly - AI Writing Ally

Load your writing.
Train Ailly on your voice.
Write your outline.
Prompt Ailly to continue to continue the writing.
Edit its output, and retrain to get it even more like that.

Rhymes with Daily.

## Engines

- OpenAI `openai`
  - [Models documented by OpenAI](https://platform.openai.com/docs/models/continuous-model-upgrades)
- Bedrock `bedrock`
  - [Claude 2 and available models documented by AWS](https://docs.aws.amazon.com/bedrock/latest/userguide/api-methods-list.html)

To choose an engine, specify `engine: [bedrock|openai]` in an `.aillyrc` or provide `ailly --engine` on the command line.

## Installing Ailly Extension

- Clone the repo and install dependencies
  - `git clone git://github.com/davidsouther/ailly.git ; cd ailly ; npm install`
- Package the extension with `npm run package`
- In VSCode extensions, install `./extension/ailly-0.0.1.vsix` from vsix.
- Right click a file in content explorer and select `Ailly: Generate`

## Installing ailly command line

- Clone the repo and install dependencies
  - `git clone git://github.com/davidsouther/ailly.git ; cd ailly ; npm install`
- Compile the core module with `npx tsc -p core`
- Install ailly cli with `npm install -g ./cli`
- Set any environment variables for your engine
  - `export OPENAI_API_KEY=sk-...`
- Run ailly with `npx ailly`
  - Optionally, expose ailly directly with `alias ailly="$(PWD)/cli/index.js`

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

## Running the Extension in Dev Mode

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
