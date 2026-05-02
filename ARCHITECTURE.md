# Ailly Architecture Components

## Content
Use `vfs` crate to store LLM converstaion turns in .toml files. Each .toml file starts with a user prompt (possibly multiple messages), and then assistant and user messages (possibly back and forth between user and assistant messages for MCP tools or reference lookup) until the model has stopped with reason `end_turn`, `stop_sequence`, `refusal`, or similar. 

`.aillyrc` files store system prompts and configure how Ailly manages content in a folder. 

- **`parent`** `root` | `always` | `never`
  - `root` (default) start the chain of system prompts from the loaded `.aillyrc` file.
  - `always` include the .aillyrc file in the parent directory as part of this system prompt.
  - `never` don't include any other system prompts.
  - Note: `always` goes up one level, and then `parent` gets reapplied. To include several ancestors, have `parent: always` in each, with `root` as the base `.aillyrc` of the project.
- **`isolated`**: `boolean` (default `false`) If `true`, the LLM inference only includes the system prompt, and not the prior context in this folder.
- **`skip`**: `boolean` (default `false`) If `true`, the prompt is not sent through the LLM to (re)generate the response (but it is part of the conversation). Default `false`, unless `no-overwrite` is set.
- **`overwrite`** `boolean` (default `false`) when `true` and there is already a response, run this prompt regardless. When `false`, only run this prompt if there is no response.

Skip: Don't run this ever. No Overwrite: Only run this if no response.

## Engine
Use rig, rmcp, and agent-client-protocol to manage collections of requests. Takes Content, runs the LLM backend, and stream writes back the updated content. Handles `tool_use`, `pause_turn`, or similar stop reasons that do not require user input to continue the conversation. Handles lifecycle hooks and permissions. Permissions follow a "reads are safe, writes are dangerous" model. Writes within the project root are considered safe (it's assumed files are managed in source control, limiting blast radius).

## Workflow
Use Sayiir crate to manage workflows of tasks. A task is a name, the task to complete, any tools and resources to use when completing the task, a completion condition to evaluate for the task, and a branching of any tasks to trigger next.

### Tasks
- **Name** A string name to identify this workflow task.
- **Task** Prompt or ToolCall or PromptToolCall. Prompt: plain text description of what the LLM should do. ToolCall: A specific tool call action to take. PromptToolCall: A prompt with an explicit `tool_choice` provided, to force the model to use that tool.
- **Evaluation** Prompt or ToolCall or PromptToolCall. An evaluation that the step has, in fact, been completed. Results in `success`, `failure`, or another string to determine what next action to take.
- **Next** Map<EvaluationResult, Name> matches the evaluation result (`success`, `failure`, or other) to the name of another task to execute next. If no match is found, or the named task is unknown, no further action is taken.

## Knowledge
CodeGraph-rust and other knowledge base things to do semantic rag both on the project's source code and supporting documentation artifacts. Loads [Agent Skills](https://agentskills.io/home). Handles MCP sessions.

## Project
The primary API layer, exposing a Project with Project::instruct(&mut self, prompt: String) that runs a user prompt to make modifications to a project, and Project::resources(&self, query: String) which runs a user prompt to find resources within a project matching the prompt. A project has a root folder, stores its conversations, settings, etc in `.ailly`. Loads Skills, MCP configurations, etc. 

## cli

```
usage: ailly [options] [paths]
paths:
  Folders or files to generate responses for. If unset, uses $(PWD).

options:
  -r, --root sets base folder to search for content and system prompts. If unset, uses $(PWD).
  -s, --system sets an initial system prompt.
  -p, --prompt generate a final, single piece of content and print the response to standard out.
  -i, --isolated will start in isolated mode, generating each file separately. Can be overridden with 'isolated: false' in .aillyrc files.
  --continue when a response is present without a stop_reason, will include that response as an "assistant" message rather than overwriting it.

  -w, --workflow `workflow:task` starts workflow at a task (or at the first task, if not given).

  --clean resets all your ailly files to have no debug, no response, and minimal head matter settings.

  --engine will set the default engine. Can be set with AILLY_ENGINE environment variable. `noop` is available for testing.
  --model will set the model from the engine. Can be set with AILLY_MODEL environment variable. Default depends on the engine.

  --request-limit will limit the number of requests per call to the provided value. Default value is 5, except for Opus with a default of 1.
  --max-depth will allow loading content below the current root. Default 1, or only the root folder. 0 or negative numbers will load no content.

  --overwrite will run generation on Content with an existing Response. Changes default `overwrite` to `true`, but file and folder level settings take precedence.
  -y, —-yes will skip any prompts.
  -v, --verbose, --log-level v and verbose will set log level to info; --log-level can be a string or number and use env-filter logging levels. Ailly uses warn for reporting details on errors, info for general runtime progress, and debug for details of requests and responses.
  --log-format json or pretty; default is pretty. JSON prints in JSONL format.

  --version will print the cli and core versions
  -h, --help will print this message and exit.

Engines:
  (See Rig and re-evaluate)
  openai - Call ChatGPT models using OpenAI's API.
  noop - A testing model that returns with constant text (either a nonce with the name of the file, or the contents of the AILLY_NOOP_RESPONSE environment variable).
```
  
## Ratatui

## Agent Client Protocol
