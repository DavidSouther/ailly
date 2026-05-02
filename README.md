# Ailly: AI Writing Ally

Don't review AI output. Edit it.

Ailly lets you control the loop. At every step, it writes the entire conversation locally. At any time, you can stop it, edit what it's done, and continue as if nothing had changed.

* Edit thinking mid-thought
  * All conversation messages saved locally
  * Stop, edit, and resume at any time.
* Fixed outer workflows
  * Well-defined workflows drive the agentic loop.
  * LLMs are given specific tasks within a workflow, but do not spend tokens deciding what to do next.
  * Shipped workflows:
    * Researcher: Parallel web and project search
    * Developer: Design, Feature Test, Types, Red, Green, Refactor
    * Experiment: Prepare isolated prompts to evaluate in parallel and compare
* Improved context window snippets
  * Skills Selection
  * Codebase Map
  * Knowledge Graph
* Thinking, Fast and Slow
  * Local fast models perform context engineering to prepare and evaluate tasks
  * Remote slow models perform do the task with thinking, tools, etc.

Rhymes with _daily_.

Ailly's best feature is rapid prompt engineering iteration. By keeping your prompts in snippets on the file system, you can make very fine-grained changes to your prompt and immediately see the difference in the output. You can also use all of your normal source control tooling to track changes over time: both your changes, and those from the LLM.

## CLI Quickstart

To get started on the command line, follow these steps:

1. Ask for a joke - `./target/debug/ailly --prompt 'Tell me a joke'`
1. Create a folder named `jokes` and change directory into it.
1. Create a file named `10_chickens.md` with "Tell me a joke about chickens" as the content.
1. Run Ailly using NodeJS: `../target/debug/ailly`
   - See the joke in `10_chickens.toml`
1. Create a file named `.aillyrc.toml` with "\[system]\\nYou are a farmer writing jokes for your other barnyard animals."
   - Include other system prompts, level setting expectations. etc.
   - Run Ailly with the same command, and see how the joke changes.
1. Create more numbered files, such as `20_knock_knock.md` which contains the following text: "Turn the chicken joke into a knock-knock joke."
1. Run Ailly using NodeJS: `../target/debug/ailly 20_knock_knock.toml`
   - `20_knock_knock.md.ailly.md` now contains the new knock knock joke based on the updated chicken joke it wrote!

To use Ailly more easily, install the latest version with `cargo install`, after which the command `ailly` will run Ailly.

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

- **`parent`** `root` | `always` | `never`
  - `root` (default) start the chain of system prompts from the loaded `.aillyrc` file.
  - `always` include the .aillyrc file in the parent directory as part of this system prompt.
  - `never` don't include any other system prompts.
  - Note: `always` goes up one level, and then `parent` gets reapplied. To include several ancestors, have `parent: always` in each, with `root` as the base `.aillyrc` of the project.
- **`isolated`**: `boolean` (default `false`) If `true`, the LLM inference only includes the system prompt, and not the prior context in this folder.
- **`skip`**: `boolean` (default `false`) If `true`, the prompt is not sent through the LLM to (re)generate the response (but it is part of the conversation). Default `false`, unless `no-overwrite` is set.
- **`overwrite`** `boolean` (default `false`) when `true` and there is already a response, run this prompt regardless. When `false`, only run this prompt if there is no response.

### PLAN

PLAN to use Ailly effectively. Iterate often. Provide context. Put words in Ailly's mouth.

- **Prepare** a precise prompt (by writing an aillyrc system prompt, providing supporting documents, and specifying individual prompt steps).
- **Leverage** LLM models (by running Ailly on some or all parts of the context chain).
- **Assess** the generated content (as Ailly and the LLM write output, make sure they're on the right track).
- **Narrow** your context (by editing Ailly's generated content to keep the conversation going where you want it to go).

## Engines

See Rig documentation

### Developing

* See [ARCHITECTURE.md](./ARCHITECTURE.md) for an overview of the packages and components in Ailly.
* See [DEVELOPING.md](./DEVELOPING.md) for details on how to run and debug various Ailly components.
* See [CONTRIBUTING.md](./CONTRIBUTING.md) for instructions on making a pull request. (There are no special instructions at this time.)
* See [DESIGN.md](./DESIGN.md) for historical notes on why some decisions were made. (Not exhaustive, but hopefully interesting.)

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

Instead, with Ailly, the developer created a new file with only the list and an instruction on how to create URLs from the list items, saved it as `list.toml` (with `isolated=true`), and ran `ailly list.toml`.

The LLM followed the instructions, generated just the updated list, and the developer copied that list into the original (generated) README.md.
In later prompts, the context window included the entire list of URLs, and the agent model could intelligently request to download their contents.

To the author's knowledge, no other LLM interface provides this level of interaction with LLMs.
