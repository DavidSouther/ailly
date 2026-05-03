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

The Knowledge subsystem injects additional information into the conversation. CodeGraph 

Kahneman's work "thinking fast and slow" describes a two-tier reasoning system in the human brain. The fast brain is responsible for immediate attention needing tasks, while the slow brain takes time to work through reasoning tasks.

Coding agents circa 2026 use Foundational models for all aspects of their output. The agent takes a user prompt, combines certain preloaded bits of context, and responds to LLM tool use calls to get additional context data. (Claude's "Assist" tool is taking a stab at this.)

A hybrid approach, inspired by Thinking Fast and Slow, will use local, small, fast models to determine a better initial set of context. The remote foundation model then only requests clarification for any points, rather than driving the LLM through tool usage.

A new tool, `Clarify`, can be used when additional context is desired. An agent can implement this tool as "search or ask". That is, for questions that are likely answerable via information local to the project, the local agent will use local LSP or file tools to answer. Otherwise, it can ask the user for guidance.

### Project map and code map

The Knowledge subsystem provides a code map utility. Graph RAG 

- **In-process crate.** `anvanster/codegraph` v0.2.0 (Apache 2.0, RocksDB backend, 16 languages, native Rust query API). Linked directly into the Ailly binary. Parser-agnostic core, so Ailly still owns the tree-sitter feeding step.
- **MCP server.** `Jakedismo/codegraph-rust` (MIT, SurrealDB plus HNSW, ~13 languages, hybrid 70/30 vector and lexical retrieval, agentic MCP tools). Spawned as a subprocess and consumed through `rmcp`.

The choice between these two is a real one and belongs in the implementation design doc. The architecture commits to one of them as the indexing backbone, not to a hand-rolled tree-sitter plus `petgraph` substitute.

The Knowledge layer adds three concerns CodeGraph does not cover:

1. **Documentation corpus.** `.md` and `.toml` artifacts, plus the `.aillyrc` chains, indexed separately from the code graph. Embedded with `rig::embeddings::EmbeddingModel` and held behind `rig::vector_store::VectorStoreIndexDyn`. Default backend is rig's `InMemoryVectorStore`. Persistent backend is `rig-lancedb`, writing under `.ailly/lance/`.
2. **Repo map preface.** A PageRank pass over CodeGraph's graph, rendered as a token-budgeted (default 1000) summary of definitions and prepended to every request inside the system chain. Aider-style. Cheap, always on, independent of per-turn retrieval.
3. **rig glue.** A `KnowledgeIndex` type that returns `impl VectorStoreIndexDyn`, so the Engine wires retrieval through `agent.dynamic_context(k, idx)` without seeing CodeGraph directly.

Retrieval is hybrid. A query asks CodeGraph for graph-structural matches (callers, definitions, imports) and the documentation index for semantic matches, merges and reranks the two streams, and splices the result into the message stream as separate user messages between the system chain and the predecessor history.

### Agent Skills

The Knowledge subsystem loads Agent Skills following the open standard at [agentskills.io](https://agentskills.io/specification), version 1.0 (open release 2025-12-18). A Skill is a directory whose `SKILL.md` carries YAML frontmatter and a Markdown body. Required frontmatter fields are `name` (lowercase, 1 to 64 chars, equal to the directory name) and `description` (1 to 1024 chars). Optional fields are `license`, `compatibility`, `metadata`, and `allowed-tools`.

Loading is three-tier progressive disclosure:

1. **Discovery.** At project load time, walk the skill search paths and read only `name` and `description`. The discovery index is folded into the system chain. Budget is roughly 100 tokens per skill.
2. **Activation.** When the model requests a skill, or the user names one, load the full `SKILL.md` body. Recommended cap is 5000 tokens.
3. **Execution.** Files under `scripts/`, `references/`, and `assets/` load only when the skill body refers to them.

Search paths, in order of precedence: `<project>/.ailly/skills/`, then `~/.ailly/skills/`, then `~/.claude/skills/`. The optional `allowed-tools` frontmatter composes with the Engine's "reads safe, writes dangerous" permission model rather than replacing it. The loader is hand-rolled on `walkdir` plus a YAML parser. No skills crate is taken as a runtime dependency.

### MCP sessions

The Knowledge subsystem owns MCP sessions for **resources**, distinct from the Engine's use of MCP for **tools**. A configured MCP server can expose document URIs, search endpoints, or other live read-only data. The Knowledge layer maintains the client connection through `rmcp` (gated by rig-core's `rmcp` feature), surfaces resources to the `augment` hook for inclusion in retrieval, and shuts the session through `clean`. Tool-bearing MCP servers stay with the Engine. A single MCP server may expose both surfaces, in which case the Engine and Knowledge subsystems share the underlying `rmcp` connection but consume different parts of its catalog.

### Notes for the Rust port

The Rust crate currently has no Knowledge code. Implementation lands after the Engine slice ([docs/developer/2026-05-01-A-engine/](docs/developer/2026-05-01-A-engine/)) and follows the same design-then-feature-test cadence. The CodeGraph choice drives the rest of the dependency set:

- **In-process route:** `anvanster/codegraph` v0.2.0 plus `tree-sitter` and the language grammars (codegraph is parser-agnostic), plus rig-core's embeddings and vector-store surface for the documentation corpus, plus `walkdir` and a YAML parser for the Skills loader.
- **MCP route:** `rmcp` (gated by rig-core's `rmcp` feature) for the CodeGraph subprocess, plus rig-core's embeddings and vector-store surface, plus `walkdir` and a YAML parser. No tree-sitter in our process.

Persistent vector storage (`rig-lancedb`) is deferred to a second slice behind rig's `InMemoryVectorStore`.

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
