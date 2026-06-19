# Follow-on project: Additional tool implementations

Seeds for the tools deferred out of the **tool-calls** vertical-slice project
([.ailly/developer/2026-06-14-A-tool-calls/design.md](2026-06-14-A-tool-calls/design.md)).
That project delivers the harness (Step-0 contract: `ToolDefinition`, `meta.tools`,
`ToolExecutor`, the `Conversation::run` tool loop, `NoopToolExecutor`) plus the two
web tools and the e2e. These additional tools reuse that Step-0 contract unchanged.
Each is one parallel feature with its own design, plan, and build cycle, and its own
unit test exercising the tool logic directly (harness tests stay on `NoopToolExecutor`).

This is a **follow-on project**, not a single feature. It is several tool features
that share the harness but otherwise deliver independently. Size it with its own
research and design pass when picked up.

## Tools and their smallest versions (from research.md)

All live in `src/knowledge/tools/`.

### File: `knowledge/tools/file.rs`

- `File:Glob` lists directories. `./path/*` lists files, `./path/**/*` recurses,
  and `./path/*.txt` plus brace patterns `./path/*{foo,bar}.{ts,tsx}` filter.
  **Open:** pick an appropriate globbing library. The project already has glob
  handling in `context.glob_concat` and the `Context` prefix block, so check
  whether `**` recursion is supported there. Recursive `**` is itself a deferred
  project-layout item.
- `File:Read` reads a file, with optional line ranges.
- `File:Edit` edits a file by line ranges, including full-file replace or write.
- Sandboxing: these read and write host files. Reconcile with the `external`
  prefix-block sandbox model and the "configured source roots" follow-up before
  exposing writes.

### Script: `knowledge/tools/script.rs`

- `Script:Bash` runs a shell script via a bash on PATH, and the same for Python
  and Node.
- **If a runner is not available, do not advertise the tool.** The tool
  advertisement must include the version of the scripting engine it has, and must
  provide ways to specify `cwd` and `env`. Lots to research here: runtime
  detection, version probing, cwd/env plumbing.
- Note the existing `knowledge/script_runner.rs` and the eval `script`/`program`
  assertion machinery. Reuse, do not duplicate, the subprocess plus cleared-env
  plus allowlist contract already established there.

### Clarify: `knowledge/tools/general.rs` (`General:Clarify`)

- Takes an LLM question and attempts to research an answer in a subagent. If the
  results are inconclusive or contradictory, asks the user to resolve the question.
- Depends on the Subagent tool (below) for the research step, and on an interactive
  user-prompt path for the fallback.

### Subagent: `knowledge/tools/subagent.rs`

- Same shape as other agentic frameworks' subagent skills: spawn a nested
  conversation/run and return its result as a tool result.

### Todo: `knowledge/tools/todo.rs` (`Todo:*`)

- Named only in research.md's *Scope / In* listing (`Todo:*`), not in its main
  tool list or smallest-version table, so the intended shape is unspecified.
  Carried here so it is not silently dropped. Scope it during the follow-on
  project's research pass (a task-list read/write tool, mirroring other agent
  frameworks' todo tools), or drop it explicitly if it was a stray entry.

## Carried-over deferrals these features will need to revisit

- Streaming tool responses, the `is_error: true` retry loop, per-turn tool
  variation, and `Message.cache` forwarding on tool turns. All deferred in the
  slice project.
- Recursive `**` glob support (a project-layout deferred decision). `File:Glob`
  depends on it.

**Trigger:** the harness from the tool-calls slice project has landed (its Closing
Bell passed), and a harness or e2e needs one of File / Script / Clarify / Subagent /
Todo.
