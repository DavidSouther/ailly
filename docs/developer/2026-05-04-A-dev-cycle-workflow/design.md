# Workflow For The DDD-Prescribed Development Cycle

## Problem Statement

The Ailly workflow engine drives an LLM through prompts whose responses route the next step via `task.next`. The DDD-prescribed development cycle (design → feature-test → plan → red-green-refactor) is human-gated. Each non-terminal phase ends with an artifact (`design.md`, `feature-test.md`, `plan.md`) carrying a `*Draft YYYY-MM-DD*` marker that only a human may clear. The engine today has no representation of "halt, wait for a human, then resume in a future session." A workflow that lands on a draft gate has two exits available: route on a known `next` key, or fail with `UnknownNext`. Neither of those is *pause*.

This component extends the workflow schema and runtime so a single workflow file can:

1. Express each gate as an evaluation that consults a tool registry instead of the engine.
2. Halt with an explicit `Paused` reason when the artifact still carries its draft marker, persisting `workflow.state.toml` with the gated task at the head of the queue.
3. Resume in a later session by re-checking the gate, advancing on `cleared`, halting again on `paused`.
4. Accept user-supplied inputs at workflow start (topic, goal) and propagate them into prompt text and tool args through `{{ template }}` substitution, so the workflow file does not hardcode session paths or per-cycle identifiers.
5. Declare which skills each task's prompt turn must have loaded, threading those names into the synthesized turn TOML so the engine resolves them through the existing skill-loading path.
6. Express in-turn user elicitation through a reserved `user.clarify` tool that halts with `AwaitingInput`. Tasks must not end their turn awaiting an unrouted answer, since the engine has no way to interpret that as a request for input.

The deliverable is the schema change, the runtime change, and `e2e/09_dev_cycle/dev_cycle.toml`, which is the literal DDD cycle expressed as a workflow.

## Prior Art

**1. `task.evaluation` as `TaskAction::Prompt`.** `src/workflow/runtime.rs:201-330` runs an evaluation prompt against a 2-turn conversation built via `Conversation::from_paths` and uses the trimmed response as the routing key. The eval path goes directly to `engine.stream` rather than through `Generator::run`, with `record_response` and per-turn `record_tool_call` / `record_tool_result` invoked inline. The new `TaskAction::ToolCall { tool, args }` slides into the same dispatch site as a sibling. The prompt-eval and tool-call-eval branches both produce a `String` result, from different sources: one from `engine.stream`, one from `ToolDyn::call(args_json) -> Result<String, ToolError>` resolved via the engine's `ToolRegistry`.

**2. `WorkflowState` persistence.** `src/workflow/state.rs` already persists `queue`, `history`, `last_result`. Resume across sessions is solved for the happy path. Pause is a structured halt that pushes the gated task to the head of `queue` and writes state, mirroring `requeue_and_persist` at `src/workflow/runtime.rs:389`.

**3. `WorkflowStopReason::UnknownNext` and `StatePersistFailed`.** `src/workflow/runtime.rs:33-51`. `Paused` is their sibling: an *expected*, *recoverable* halt distinguished from a routing typo or an unrelated persistence error.

**4. `ToolRegistry` trait, merged.** `src/engine/mod.rs:179-210`. The shipped signature is `fn resolve(&self, name: &str) -> Option<Arc<dyn rig::tool::ToolDyn>>` (sync, returns a tool instance). `EmptyRegistry` is the default; `HashMapRegistry` is the test/CLI registration shape. `Generator::new(...).with_registry(registry)` threads the registry through prompt turns. The workflow runtime today constructs `Generator::new(...)` without a registry (`src/workflow/runtime.rs:172`); this design adds the parameter and propagates it. Dispatch in this design uses the rig contract `ToolDyn::call(&self, args: String) -> Future<Result<String, ToolError>>`, where `args` is a JSON-encoded string the runtime synthesizes from the workflow file's `args` field.

**5. Per-turn tool declarations, merged.** `src/content/mod.rs` rounds `tools = [...]` and `parent_tools = "extend" | "replace"` through `.ailly.toml` and turn TOML, deduping by name and preserving order. `Settings::strict_tools` (default `true`) makes unknown names surface as `TurnEvent::Failed`; `false` warns and drops. The dev-cycle workflow's evaluation `ToolCall` does not appear on a turn's `tools` chain because it is dispatched by the runtime, not by the model. The same registry covers both call sites.

**6. `Generator::SkipReason::AlreadyHasResponse` variant exists; filter unimplemented.** `src/engine/generator.rs:17`. The variant landed with the engine slice but the run-loop never emits it; the actual skip filter is still deferred (`docs/developer/TASK-NOTES-engine-deferred.md`, tracked in `docs/developer/TASKS.md` under "deferred Generator behaviors"). On resume, the gated task already has a turn file with a recorded response. Re-running the prompt would re-spend tokens, overwrite the artifact via `convo.record_response`, and pollute history. This design's "skip the prompt when the turn already exists" lives in the workflow runtime and consults `state.history` directly; when the Generator filter ships, the runtime can delegate.

**7. Deferred swap of eval path to `Generator::run`.** `docs/developer/TASKS.md` line 40. The current direct `engine.stream` evaluation path was introduced to keep the task-evaluation slice contained while the overwrite filter remained deferred. Once `SkipReason::AlreadyHasResponse` ships, the eval turn becomes a second `Generator::run` over a 2-turn `Conversation`. This design adds `ToolCall` evaluation alongside the existing direct path; the swap to Generator is orthogonal and tracked separately.

**8. Deferred `TaskAction::ToolCall` (as `task`) and `PromptToolCall`.** Named in earlier task-evaluation design notes and in `docs/developer/TASKS.md`. This design lands the `ToolCall` variant for `evaluation` first; `task = ToolCall` and `task = PromptToolCall` remain deferred.

**9. `developer:ailly` SKILL.md.** `/Users/davidsouther/devel/davidsouther/domain-driven-design/developer/skills/ailly/SKILL.md`. Source of truth for the cycle. The example workflow file is a translation of the resume table on lines 20-28 of that file.

## Metrics

A workflow file at `e2e/09_dev_cycle/dev_cycle.toml`, exercised with a `Noop` engine, a `HashMapRegistry` carrying `fs.absent`, and a fixture-seeded session folder, demonstrates the full lifecycle measurably:

1. **Pause on first run.** Run the workflow with `design.md` pre-seeded as `*Draft 2026-05-04*`. The runtime drives the `design` prompt task (producing `01_design.toml` with a recorded response), then runs `evaluation = ToolCall { tool = "fs.absent", args = { path = "design.md", needle = "*Draft" } }`. The runtime resolves `fs.absent` via the registry, serializes args to JSON, calls `tool.call(args_json).await`, and receives `"paused"`. Workflow halts with `WorkflowStopReason::Paused { task: "design", result: "paused" }`. `workflow.state.toml` shows `queue = ["design"]`, history with one entry whose `result` is `paused`.

2. **Resume past the gate.** The harness rewrites `design.md` with the marker removed. Re-run the workflow against the same conversation root and persisted state. The runtime pops `design`, finds an existing turn file with a recorded response in `state.history`, **skips prompt synthesis and `Generator::run`**, runs only the evaluation. The eval returns `"cleared"`. The workflow routes via `next.cleared = "feature_test"` and continues. `01_design.toml` is byte-identical after the resume (no second `record_response`, no second write).

3. **End-to-end.** Repeat the pause/resume cycle for `feature_test`, `plan`. The `rgr` task uses a `Prompt` evaluation that emits `continue` (loop back to `rgr`) or `done` (route to terminal `complete`). The harness pre-seeds an `rgr_progress` counter file the eval references so the eval flips to `done` after N iterations. Workflow finishes with `WorkflowStopReason::Completed`.

4. **Tool failure surfaces as TaskFailed.** When a tool call returns `Err(ToolError)` for a non-pause reason (artifact unreadable, fs error), the runtime halts with `WorkflowStopReason::TaskFailed { task, error }` carrying the tool error. The gated task is preserved at the head of the queue. No `TurnEvent::Failed` is emitted because the eval `ToolCall` does not synthesize an eval turn file.

5. **Unknown tool fails fast.** A workflow whose `evaluation` names a tool not in the registry fails on first dispatch with `WorkflowError::UnknownEvalTool` referencing the task and tool name. The workflow does not partially execute past the unknown tool. The error surfaces as `WorkflowStopReason::TaskFailed`.

6. **Args encode error fails fast.** A workflow whose `args` cannot be serialized to JSON (today this is unreachable for any TOML literal a human would author, but the path exists for `Datetime` corner cases) fails with `WorkflowError::EvalArgsEncode`, surfacing as `TaskFailed`.

7. **Inputs collected, context resolved, paths render.** Run the workflow with `vars.topic = "dev-cycle-workflow"` and `vars.goal = "..."`. The `design` task's prompt body and eval `args.path` both reference `{{ session_dir }}`, which resolves to `docs/developer/2026-05-04-A-dev-cycle-workflow`. The synthesized turn TOML on disk shows the substituted path verbatim, with no `{{ }}` markers remaining. A workflow with a typo like `{{ vars.tpic }}` halts on the first task with `WorkflowStopReason::TaskFailed` carrying `WorkflowError::UnresolvedTemplate`.

8. **Skills propagate into the synthesized turn.** The `design` task's `01_design.toml` carries `skills = ["developer:design"]` in its turn TOML. The `rgr` task's turn TOML carries both `developer:red-green-refactor` and `developer:thinking`. Loading these names is exercised by the existing engine-layer skill resolution; the workflow-side metric is the round-trip through the synthesized turn file.

9. **Clarify pause-and-resume.** A `design` prompt that calls `user.clarify` with `{ question: "What database backs the inbox queue?" }` halts the workflow with `WorkflowStopReason::AwaitingInput { task: "design", question: "What database backs..." }`. `workflow.state.toml` shows `pending_clarification` populated. The harness writes the answer to `state.clarifications.design` and clears `pending_clarification`. Re-running the workflow replays the prompt turn. The same `user.clarify` call, finding its question now in `state.clarifications`, returns the recorded answer to the prompt without re-halting. The turn proceeds to write `design.md` and the eval routes the workflow normally.

10. **Clarify deduplicates within a run.** A second prompt iteration that calls `user.clarify` with the same `question` string in the same task returns the same recorded answer without halting. Two distinct questions in the same task each receive their own answer slot in `state.clarifications.<task>`.

## Specification

### Schema additions — `src/workflow/schema.rs`

The current schema (`src/workflow/schema.rs:28-32`) has only `Prompt`. The new variant is added alongside it.

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskAction {
    Prompt { text: String },
    /// Evaluation-only for this slice. `task = ToolCall` is deferred.
    /// `args` is parsed from TOML at workflow-load time, then re-serialized
    /// to a JSON string at dispatch to satisfy `ToolDyn::call(args: String)`.
    ToolCall { tool: String, args: toml::Value },
}

#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error("workflow {workflow:?} has no task named {name:?}")]
    UnknownTask { workflow: String, name: String },
    #[error("task {task:?} uses TaskAction::ToolCall, which is not yet supported as a `task` action (only as `evaluation`)")]
    ToolCallTaskNotImplemented { task: String },
    #[error("task {task:?} evaluation references unknown tool {tool:?}")]
    UnknownEvalTool { task: String, tool: String },
    #[error("task {task:?} evaluation tool {tool:?} could not encode args as JSON: {source}")]
    EvalArgsEncode {
        task: String,
        tool: String,
        #[source]
        source: serde_json::Error,
    },
    #[error("task {task:?} references unresolved template placeholder {placeholder:?}")]
    UnresolvedTemplate { task: String, placeholder: String },
    #[error("workflow input {name:?} is required but the harness supplied no value")]
    MissingInput { name: String },
    #[error("workflow input {name:?} value {value:?} does not match required pattern {pattern:?}")]
    InputPatternMismatch { name: String, value: String, pattern: String },
}
```

Task and Workflow gain explicit fields for declared skills and declared inputs. `Task::skills` lists the skills the task's prompt turn must have loaded. `Workflow::inputs` is the harness-bootstrapped variable map that feeds template substitution.

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Task {
    pub name: String,
    /// Skills the prompt turn must have loaded. Forwarded into the synthesized
    /// turn TOML's `skills = [...]` field. Unknown names surface at load time
    /// through the engine's existing skill-resolution path.
    #[serde(default)]
    pub skills: Vec<String>,
    pub task: TaskAction,
    pub evaluation: Option<TaskAction>,
    #[serde(default)]
    pub next: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InputSpec {
    pub description: String,
    #[serde(default)]
    pub required: bool,
    /// Optional regex the value must satisfy. Used to enforce kebab-case on
    /// `topic`, RFC3339 on dates, etc.
    pub pattern: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Workflow {
    pub name: String,
    pub start: String,
    /// Top-level inputs the harness must collect before the first task runs.
    /// Resolved values populate the template context as `{{ vars.<name> }}`.
    #[serde(default)]
    pub inputs: BTreeMap<String, InputSpec>,
    pub tasks: Vec<Task>,
}
```

`args` stays as `toml::Value` on the schema so the workflow file is authored as TOML, with the runtime serializing to a JSON string via `serde_json::to_string` at dispatch (see "Runtime evaluation dispatch" below). Keeping `toml::Value` here preserves the round-trip semantics already established by the workflow file format and keeps schema-time validation independent of any one wire encoding.

Two-step landing for the variant:

- `evaluation: Option<TaskAction>` accepts `ToolCall`. The runtime dispatches it via the tool registry.
- `task: TaskAction` accepts only `Prompt` for this slice. A workflow whose `task` is `ToolCall` errors on first execution with `WorkflowError::ToolCallTaskNotImplemented { task }`. The variant is present in the type without committing to its semantics yet.

### New stop reason — `src/workflow/runtime.rs`

```rust
pub enum WorkflowStopReason {
    Completed,
    UnknownNext { task: String, result: String },
    TaskFailed { task: String, error: Arc<anyhow::Error> },
    StatePersistFailed { error: Arc<anyhow::Error> },
    Cancelled,
    /// Eval returned the literal string `"paused"`. State is persisted
    /// with `task` at the head of the queue. Re-running the workflow
    /// re-evaluates the gate.
    Paused { task: String, result: String },
    /// The current task called the reserved `user.clarify` tool with a
    /// question that has no answer recorded in `state.clarifications`.
    /// State is persisted with the gated task at the head of the queue and
    /// `pending_clarification` populated. The harness collects the answer
    /// out-of-band and writes it back into state before re-running.
    AwaitingInput { task: String, question: String },
}
```

`"paused"` is reserved. A workflow author who names a `next` key `paused` is overridden by the runtime: pause halts before `next` is consulted. Documented in the schema rustdoc.

### Runtime evaluation dispatch

The eval block at `src/workflow/runtime.rs:201-330` gains a second branch (eval-result production split into a helper, with the existing inline `engine.stream` path moved into `run_prompt_evaluation`).

```rust
let eval_result: String = match &task.evaluation {
    Some(TaskAction::Prompt { text: eval_text }) => {
        // existing direct-engine.stream path
        run_prompt_evaluation(...).await?
    }
    Some(TaskAction::ToolCall { tool, args }) => {
        run_tool_call_evaluation(&tool_registry, &task.name, tool, args).await?
    }
    None => {
        last_stop_reason
            .as_ref()
            .map(StopReason::to_string)
            .unwrap_or_else(|| "error".to_string())
    }
};

if eval_result == "paused" {
    requeue_and_persist(&mut state, &conversation_root, &task_name);
    yield WorkflowEvent::WorkflowFinished {
        reason: WorkflowStopReason::Paused {
            task: task_name,
            result: eval_result,
        },
    };
    return;
}
```

`run_tool_call_evaluation` is the registry call site. It uses the merged `crate::engine::ToolRegistry`:

```rust
async fn run_tool_call_evaluation(
    registry: &dyn ToolRegistry,
    task_name: &str,
    tool_name: &str,
    args: &toml::Value,
) -> Result<String, anyhow::Error> {
    let tool = registry.resolve(tool_name).ok_or_else(|| {
        WorkflowError::UnknownEvalTool {
            task: task_name.to_string(),
            tool: tool_name.to_string(),
        }
    })?;
    // toml::Value -> serde_json::Value -> String. ToolDyn::call takes JSON.
    let json: serde_json::Value =
        serde_json::to_value(args).map_err(|e| WorkflowError::EvalArgsEncode {
            task: task_name.to_string(),
            tool: tool_name.to_string(),
            source: e,
        })?;
    let args_str = serde_json::to_string(&json).map_err(|e| WorkflowError::EvalArgsEncode {
        task: task_name.to_string(),
        tool: tool_name.to_string(),
        source: e,
    })?;
    Ok(tool.call(args_str).await?)
}
```

The `toml::Value` to `serde_json::Value` conversion is the standard serde re-serialize pattern; both crates implement `Serialize`. A `Datetime` value in TOML serializes through serde's standard mapping, which is acceptable since tool authors who need datetimes can encode them as RFC3339 strings.

`Runtime::new` gains a `tool_registry: Arc<dyn crate::engine::ToolRegistry>` parameter. The same registry is forwarded to the prompt-turn `Generator` via `Generator::new(...).with_registry(tool_registry.clone())` so any prompt task whose turn TOML declares `tools = [...]` can resolve the same names. Today the runtime constructs `Generator::new(...)` with no registry (`src/workflow/runtime.rs:172`); this slice changes that line.

**Concrete tool implementations remain deferred.** The dev-cycle workflow needs an `fs.absent` tool. Its implementation lives in a small `crate::tools` module added by this slice (one file, one struct implementing `rig::tool::Tool` with the lock-step contract below). Registration happens at the harness boundary via `HashMapRegistry::insert("fs.absent", ...)`. CLI registration of tools more broadly remains tracked in `TASK-NOTES-tools-cli.md`.

The `e2e/09_dev_cycle/` harness builds a `HashMapRegistry`, registers the workflow's required tools, and constructs the `Runtime` with it. No new feature flag is required.

### Resume semantics — skip the prompt when the turn already exists

When the runtime pops a task from the queue and is about to call `synthesize_turn_file`, it first checks `state.history` for the most recent entry with `task == task_name`. If found, the entry's `turn` field names a file in the conversation root. The runtime:

1. Resolves the existing path: `conversation_root.join(history_entry.turn)`.
2. Reads the turn file via `Conversation::single_turn(existing_path).await`.
3. Verifies the turn carries a recorded `[[response]]`. If not, the prior run failed mid-prompt; treat as fresh and re-prompt.
4. If a response is present: skip `synthesize_turn_file`, skip `Generator::run`, set `last_stop_reason = response.stop_reason`, set `turn_path = existing_path`, proceed directly to the evaluation.

This is the workflow-level analogue of the deferred `Generator::SkipReason::AlreadyHasResponse` filter. The variant exists in `src/engine/generator.rs:17` but the run-loop does not emit it today; the actual skip filter is tracked in `TASK-NOTES-engine-deferred.md`. This design does not depend on that filter shipping. The workflow runtime does its own check against `state.history`. When the deferred filter and the eval-via-`Generator::run` swap (TASKS.md line 40) both land, this code can defer to the engine-level skip and the workflow runtime's history-walk becomes redundant.

### Workflow inputs and template context

The workflow file declares top-level `[inputs.<name>]` entries. The harness collects each input before the first task runs and writes the resolved values to `workflow.state.toml` under `[inputs]`. The runtime resolves a flat string-keyed context map at task-dispatch time, populated from:

- `vars.<name>` for every declared input.
- `today` set to the RFC3339 date when the workflow first ran, persisted into `state.context_seed` so re-runs across days do not change the resolved session folder.
- `session_dir` set to `docs/developer/{{ today }}-A-{{ vars.topic }}` when both `today` and `vars.topic` are present. The harness can override this default by writing `session_dir` directly into state before the workflow starts, allowing workflows that target a different layout convention.

Substitution is `{{ name }}`. The substitution pass runs at task dispatch, not at workflow-load time, so a future deferred `task = ToolCall` whose output extends the context flows through to subsequent tasks. Substitution applies recursively to every string leaf in:

- `task.text` for `Prompt` actions.
- `args` for `ToolCall` actions, walking the `toml::Value` tree.
- `evaluation.text` and `evaluation.args` under the same rules.

`next` keys remain raw string literals because routing happens after the eval result is known and before context is consulted for the next task. Unresolved placeholders fail with `WorkflowError::UnresolvedTemplate { task, placeholder }`, surfacing as `WorkflowStopReason::TaskFailed`. The substitution is a single pass per field, so a value that contains `{{` after substitution does not retrigger.

The template engine is intentionally minimal. No conditionals, loops, or filters. A workflow that needs richer composition should call a dedicated tool whose output extends the context. That extension mechanism rides on the deferred `task = ToolCall` variant and is not part of this slice.

### Required skills propagation

`Task::skills` is forwarded into the synthesized turn TOML as `skills = [...]`. The engine's existing skill-loading plumbing (`src/content/mod.rs`) resolves the names and loads the corresponding SKILL.md content into the prompt turn before `Generator::run` issues the request. Unknown skill names surface at load time as a `TurnEvent::Failed` carrying the skill name, mirroring the `Settings::strict_tools` failure mode.

`skills` and `tools` are independent. A task may declare both, neither, or just one. The dev-cycle's `rgr` task declares both `developer:red-green-refactor` and `developer:thinking` so that the model has the thinking trigger available when red persists across multiple iterations.

Skill declarations on the workflow task do not propagate to the evaluation turn, since the evaluation prompt's job is routing rather than producing an artifact and does not need the producing skill loaded. If a future evaluation prompt needs a skill, the schema can grow `evaluation_skills` later.

### User elicitation via the `user.clarify` tool

A task that needs additional information from the user must obtain it through a tool call. Ending a turn without a recorded response, or with a response whose body asks a question and offers no routing key, is treated as "task done, run evaluation" and the workflow continues with no opportunity to capture the answer. Workflow authors must therefore route every user interaction through the reserved `user.clarify` tool.

`user.clarify` is registered automatically into every `Runtime`'s tool registry alongside any harness-provided tools. The tool accepts `{ question: String, default: Option<String> }` and is intercepted by name before dispatch reaches the registry's resolver:

1. The runtime checks `state.clarifications: BTreeMap<String, BTreeMap<String, String>>` keyed by `task_name -> question` for an existing answer.
2. If found, the answer is returned to the prompt turn as the tool result and the turn continues normally.
3. If not found, the runtime persists `state.pending_clarification = Some(PendingClarification { task, question, default })`, halts the workflow with `WorkflowStopReason::AwaitingInput { task, question }`, and the run exits.

On resume, the harness:

1. Reads `state.pending_clarification`.
2. Collects the answer through whatever surface the harness exposes (CLI prompt, IDE input, web form).
3. Writes the answer into `state.clarifications[task][question]` and clears `pending_clarification`.
4. Re-runs the workflow.

The runtime, on the second run, replays the prompt turn against the recorded transcript. When the tool call for `user.clarify` re-fires with the same `question` argument, the answer is now in `state.clarifications` and the runtime returns it directly without halting. Multiple clarifications within a single task are supported by storing one answer per question.

This pattern is parallel to but distinct from `paused`:

- `paused` halts after a task finishes its turn, gated by an evaluation. The artifact already exists. The human reviews it.
- `AwaitingInput` halts mid-turn, before the task can complete. The human supplies a missing input.

`user.clarify` is the only sanctioned mechanism for in-turn user interaction. A task that ends its turn awaiting an unrouted answer is a workflow author error and not a runtime feature.

### The dev-cycle workflow file — `e2e/09_dev_cycle/dev_cycle.toml`

```toml
name = "ailly-dev-cycle"
start = "design"

[inputs.topic]
description = "Short kebab-case slug naming this development cycle. Becomes part of the session folder path."
required = true
pattern = "^[a-z][a-z0-9-]*$"

[inputs.goal]
description = "Statement of what is being built and why."
required = true

[[tasks]]
name = "design"
skills = ["developer:design"]
task = { kind = "prompt", text = """
Use the developer:design skill to produce {{ session_dir }}/design.md for this goal:

{{ vars.goal }}

If any aspect of the goal is unclear before you begin, call the `user.clarify` tool with one focused question at a time. Do not end the turn awaiting a response. The harness will pause the workflow and inject the answer when you call the tool again.

Mark the document with *Draft {{ today }}* on the line after the title.
""" }
evaluation = { kind = "tool_call", tool = "fs.absent", args = { path = "{{ session_dir }}/design.md", needle = "*Draft" } }
next = { cleared = "feature_test" }

[[tasks]]
name = "feature_test"
skills = ["developer:feature-test"]
task = { kind = "prompt", text = """
Use developer:feature-test to produce {{ session_dir }}/feature-test.md and the executable test for the design at {{ session_dir }}/design.md. Use `user.clarify` for any missing fixture or boundary detail. Mark the document with *Draft {{ today }}*.
""" }
evaluation = { kind = "tool_call", tool = "fs.absent", args = { path = "{{ session_dir }}/feature-test.md", needle = "*Draft" } }
next = { cleared = "plan" }

[[tasks]]
name = "plan"
skills = ["developer:plan", "developer:writing-plans"]
task = { kind = "prompt", text = """
Use developer:plan to break {{ session_dir }}/feature-test.md into 3-7 incremental steps in {{ session_dir }}/plan.md. Mark the document with *Draft {{ today }}*.
""" }
evaluation = { kind = "tool_call", tool = "fs.absent", args = { path = "{{ session_dir }}/plan.md", needle = "*Draft" } }
next = { cleared = "rgr" }

[[tasks]]
name = "rgr"
skills = ["developer:red-green-refactor", "developer:thinking"]
task = { kind = "prompt", text = """
Use developer:red-green-refactor to implement the next unchecked step from {{ session_dir }}/plan.md, commit, and check it off.
""" }
evaluation = { kind = "prompt", text = "Reply with one word: 'continue' if {{ session_dir }}/plan.md still has unchecked steps, 'done' if all are checked, 'abort' if developer:thinking has already been invoked for the current error." }
next = { continue = "rgr", done = "complete", abort = "complete" }

[[tasks]]
name = "complete"
task = { kind = "prompt", text = "Workflow complete. The feature test at {{ session_dir }}/feature-test.md should now pass." }
```

`fs.absent` is the one tool this slice ships. It implements `rig::tool::Tool` with `Args { path: String, needle: String }` and `Output = String`, where `output` is the routing key. Path semantics: relative to the conversation root, which equals the session folder. `design.md` resolves to `docs/developer/YYYY-MM-DD-A-<topic>/design.md`. The tool returns `"cleared"` if the file exists and does not contain `needle`, `"paused"` if the file exists and contains `needle`, and `Err(ToolError)` if the file does not exist or cannot be read. The dev-cycle workflow always runs the producing prompt before the eval, so a missing artifact means the agent did not write the file — that is correctly a failure.

The tool reads through the same `VfsPath` interface used elsewhere in the codebase, not directly via `std::fs`, so the e2e harness can exercise it against a `mem_fs!` fixture. The conversation root is captured at tool-construction time; one `FsAbsent` instance is registered per `Runtime`, scoped to that runtime's session folder.

### Out of scope for this slice

- `task: TaskAction::ToolCall` semantics. Variant exists but errors on execution. Deferred.
- `task: TaskAction::PromptToolCall`. Deferred.
- Per-task `Settings` overrides.
- Tools other than `fs.absent` and `user.clarify`. `bash`, MCP-backed tools, and a CLI registration surface are tracked in `TASKS.md` under "Tools" and `TASK-NOTES-tools-cli.md` / `TASK-NOTES-tools-mcp.md`.
- Cancellation during a tool call. `ToolDyn::call` returns a future that the runtime `await`s without an explicit cancel branch. If the engine layer adds cancellation later, the workflow runtime can `tokio::select!` on its `cancel` token.
- Swapping the prompt-evaluation path from direct `engine.stream` to a second `Generator::run`. Tracked in `TASKS.md:40`. Orthogonal to this slice.
- Input types other than `String`. Number, datetime, and multi-select inputs can be added later by extending `InputSpec`.
- Conditional or looping template expressions. The substitution engine is single-pass with no logic. Workflows needing richer composition wait on `task = ToolCall` to extend context.
- Re-prompting a Clarify question after the user gives an unparseable answer. The harness owns answer validation today.
- Multi-question Clarify in a single tool call. One question per call.
- Promoting Clarify answers into the template context as `{{ clarifications.<task>.<question> }}`. Defer until a second consumer asks.

## Alternatives

Recorded for the future reader.

**Sketch A — `evaluation = AwaitReview { artifact, marker }`.** Rejected. Optimizes for narrow specificity at the cost of generality. The runtime would grow a per-gate-flavor variant; future gates (file exists, env var, HTTP probe, git status) each become another match arm. The "we have one general mechanism" argument wins because the parallel tool-registry branch is already paying for the general mechanism.

**Sketch B — `evaluation = Bash { command }`.** Rejected. Strictly worse than ToolCall once ToolCall is on the table: opaque shell strings, platform coupling, security surface, no per-tool static analysis. Bash remains available as one tool inside the registry, opt-in.

**Pause as a property of `next` instead of a reserved string.** A `pause_keys: Vec<String>` field on `Task` was considered. Rejected: adds schema where a reserved word does the same job. If a future gate needs richer halt vocabulary (e.g., distinct `paused-design` vs `paused-build`), the reserved word can be expanded into a marker prefix (`paused:*`) without breaking existing workflows.

**Inner-loop RGR as a separate workflow.** Considered splitting `rgr` into its own workflow file invoked as a sub-workflow. Rejected: sub-workflow invocation is its own feature, and the cycle reads more clearly when expressed as a single graph. The `rgr` task self-loops via `next.continue = "rgr"`, which is well-supported today.

**Top-level `[[phases]]` construct.** Rejected for first cut. A second top-level concept is too much new vocabulary for the workflow spec's first real consumer. Revisit if a second cycle (research workflow, ops workflow) shares the same phase-with-gate shape.

**Pause as `UnknownNext` with no new stop reason.** Rejected. UnknownNext is a *failure* state (typo in `next`). Conflating it with deliberate pause makes the consumer guess. A separate `Paused` variant lets a UI distinguish "needs human review" from "configuration error".

## Summary

Lands the workflow engine's pause-and-resume capability on the back of an extension to `task.evaluation`. The reserved result string `"paused"` produces a new `WorkflowStopReason::Paused`, requeues the gated task, and persists state. On resume, the runtime detects an existing turn file via `state.history` and skips re-running the prompt, re-running only the evaluation. The new `TaskAction::ToolCall { tool, args }` variant carries the gate. Its dispatch reuses the engine layer's already-merged `ToolRegistry` and the `rig::tool::ToolDyn::call(args: String) -> Result<String, ToolError>` contract. The `fs.absent` tool ships in the same slice. `Runtime::new` gains a `tool_registry` parameter, which is forwarded to the prompt-turn `Generator` so prompt tasks declaring `tools = [...]` resolve against the same registry.

Three companion features land alongside pause-and-resume so the workflow file can express the actual `developer:ailly` cycle without brittle assumptions:

- **Workflow inputs and `{{ template }}` substitution** carry user-supplied identifiers (topic, goal) into prompt text and tool args. The runtime computes `today` and `session_dir` from the `developer:ailly` convention so the workflow file no longer hardcodes paths like `design.md` that would break under any session layout.
- **`Task::skills`** declares the SKILL.md set the prompt turn must have loaded, propagating into the synthesized turn TOML. This makes the dependency between a task and its driving skill explicit and machine-checked rather than buried in prose.
- **`user.clarify` tool and `WorkflowStopReason::AwaitingInput`** make in-turn user elicitation a first-class workflow concept. Tasks that need information from the user obtain it through this tool, never by ending the turn awaiting a response. The pause-and-resume mechanics for `AwaitingInput` mirror those for `Paused`.

Deferred:

- **Concrete tools other than `fs.absent`.** `bash`, MCP-backed tools, and a CLI registration surface remain tracked in `TASKS.md` and the `TASK-NOTES-tools-*.md` files. This slice ships the one tool the dev-cycle workflow needs.
- **`task: TaskAction::ToolCall` and `task: TaskAction::PromptToolCall`.** Variant present in the schema; execution errors out via `WorkflowError::ToolCallTaskNotImplemented`.
- **Generator-level `SkipReason::AlreadyHasResponse`.** The variant exists in `src/engine/generator.rs:17` but the run-loop does not emit it. The workflow runtime does its own skip via `state.history`. When the engine filter lands, the runtime can defer to it. Tracked in `TASK-NOTES-engine-deferred.md`.
- **Eval path via `Generator::run`.** Tracked in `TASKS.md:40`. The current direct `engine.stream` path keeps this slice contained.
- **Sub-workflow invocation.** Out of scope; not needed for the dev-cycle.

Open questions:

- Whether `paused` as a literal string should instead be a typed `EvalResult::Paused` variant returned from tool dispatch. Argument for stronger typing and less stringly-typed magic. Argument against the change is that prompt evaluations also need to express pause and they only produce strings. Strings keep the two eval kinds symmetric. Defer until a second pause source motivates a refactor.
- Whether `fs.absent` should fail or return a third key (`missing`) when the artifact does not exist. Currently spec'd as failure (TaskFailed). Revisit if non-prompt-producing gates are added.
- Whether the workflow runtime should also expose `Settings::strict_tools` semantics for evaluation `ToolCall`. Today the eval path errors hard on an unknown tool name (`WorkflowError::UnknownEvalTool`), which is the strict behavior. A lenient eval mode that routes to a `unknown_tool` key seems strictly worse since it conflates configuration error with workflow signal. Keeping it strict by default matches the engine's `strict_tools = true` default.
- Whether `session_dir` derivation belongs in the workflow runtime or the harness. Argument for the runtime is that every workflow targeting Ailly cycles needs the same convention. Argument against is that workflows targeting other layouts must override anyway. Current design picks the runtime as the source of truth and lets the harness override by writing the value into state before start. Revisit if a second workflow with a divergent convention shows up.
- Whether `user.clarify` should accept a `key: Option<String>` so the same logical question phrased two ways still matches a previously-recorded answer. Today the question string is the cache key. A misphrase forces a second pause. Defer until observed in practice.
- Whether template substitution should also apply to skill names and tool names, allowing `skills = ["developer:{{ vars.flavor }}"]`. Argument against is that load-time validation becomes context-dependent. Argument for is parameterized workflows. Out of scope for the first cut.
