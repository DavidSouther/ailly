# Developing

These are instructions on how to run various Ailly components.

## Developing ailly command line

- Clone the repo and install dependencies
  - `git clone https://github.com/davidsouther/ailly.git ; cd ailly ; npm install`
- Compile the core module with `npx tsc -p core`
  - Rerun this step for any edits in `core`
- Install ailly cli with `npm install -g ./cli`
- Set any environment variables for your engine
  - `export OPENAI_API_KEY=sk-...`
  - `export AILLY_ENGINE=bedrock` default: openai, others depending on version.
- Run ailly with `npx ailly`
  - `cd content/33_dad_jokes`
  - `npx ailly .`
- Optionally, create an alias to run ailly
  - Directly with `alias ailly="$(PWD)/cli/index.js`
  - For zsh: `echo "alias ailly='$(PWD)/cli/index.js'" >> ~/.zshrc`
  - For bash: `echo "alias ailly='$(PWD)/cli/index.js'" >> ~/.bashrc`
  - General \*nix: `echo "alias ailly='$(PWD)/cli/index.js'" >> ~/.profile`

## Running Ailly.dev

This is powered by [Next.js](https://nextjs.org/) using App Router.

- Clone the repo, install dependencies, and duplicate the env file for local keys.
  - `git clone https://github.com/davidsouther/ailly.git ; cd ailly ; npm install ; cp .env .env.local`
- Start the project locally with `npm run dev --workspace packages/web`
  - Default at http://localhost:3000
- Visit the [`root`](http://localhost:3000/) route.
  - Follow guidance on Prompt Engineering
- Good luck!

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

# Publishing a new version

1. Bump the versions with
   - `npm version -ws [version]`
   - `npm install -ws @ailly/core@^[version]`
   - `npm un -w core @ailly/core`
2. Submit PR with only updated `package.json`s
   - `git switch -c release-[version]`
   - `git commit --all --message 'Release [version]`
   - `git push`
3. Tag merge pr with `v[version]`.
4. Publish to NPM with `npm publish -w core -w cli`
5. Prepare a new release in GitHub
6. Add the built extension .vsix to the release.
