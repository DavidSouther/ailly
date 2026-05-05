# Implementation Plan: Workflow inputs and `{{ template }}` substitution

**Feature test:** `src/workflow/runtime.rs` `tests::workflow_inputs_substitute_into_prompt_text_and_validate_at_construction` (sourced from `docs/developer/2026-05-04-A-dev-cycle-workflow/feature-test-1-inputs-templates.md`).

**User story:** A workflow declares typed inputs, the harness seeds them into `WorkflowState`, the runtime validates them at construction, and prompt bodies dispatch with `{{ vars.* }}`, `{{ today }}`, and `{{ session_dir }}` rendered into the synthesized turn TOML.

**Steps:**

- [ ] Step 0: Domain types and feature-test placement
- [ ] Step 1: Construction-time input validation (`MissingInput`, `InputPatternMismatch`)
- [ ] Step 2: `substitute` helper for strings and `toml::Value` walks
- [ ] Step 3: Dispatch-time substitution and `UnresolvedTemplate` propagation

## Step 0: Domain types and feature-test placement

**Enables:** the feature test compiles. All four assertions still fail or panic at runtime, but the type surface the test names is now real.

Introduce the schema and state surface the feature test references. No behavior beyond what the existing tests already exercise.

### Schema additions — `src/workflow/schema.rs`

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WorkflowInput {
    pub description: String,
    #[serde(default)]
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_empty")]
    pub pattern: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Workflow {
    pub name: String,
    pub start: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, WorkflowInput>,
    pub tasks: Vec<Task>,
}

#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error("workflow {workflow:?} has no task named {name:?}")]
    UnknownTask { workflow: String, name: String },
    #[error("task {task:?} references unresolved template placeholder {placeholder:?}")]
    UnresolvedTemplate { task: String, placeholder: String },
    #[error("workflow input {name:?} is required but the harness supplied no value")]
    MissingInput { name: String },
    #[error(
        "workflow input {name:?} value {value:?} does not match required pattern {pattern:?}"
    )]
    InputPatternMismatch { name: String, value: String, pattern: String },
}
```

The `Task::skills`, `TaskAction::ToolCall`, and the other `WorkflowError` variants from the design land in slices 2–4. This slice keeps the type surface minimal and only adds what the feature test names.

### State additions — `src/workflow/state.rs`

```rust
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct ContextSeed {
    pub today: Option<String>,
    pub session_dir: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct WorkflowState {
    pub workflow: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "is_default_context_seed")]
    pub context_seed: ContextSeed,
    #[serde(default)]
    pub queue: VecDeque<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_result: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<HistoryEntry>,
}
```

`WorkflowState::initial` keeps its current shape and leaves `inputs` and `context_seed` at their defaults.

### Runtime construction signature — `src/workflow/runtime.rs`

`Runtime::new` becomes fallible:

```rust
pub fn new(
    workflow: Workflow,
    state: WorkflowState,
    conversation_root: VfsPath,
    engine: Arc<dyn Engine>,
    settings: Settings,
) -> Result<Self, WorkflowError>
```

For Step 0 the body still always returns `Ok(Self { ... })`. The signature change is what unblocks the feature test from compiling against `.expect("inputs validate")` and `.expect_err("...")`.

### Drop the executable test

Append the test from `feature-test-1-inputs-templates.md` to the existing `tests` module in `src/workflow/runtime.rs`. Update every existing call site of `Runtime::new` in the same module to add `.unwrap()` so the existing tests still pass.

After this step:
- `cargo build` and `cargo test --lib workflow::state` are green.
- The new feature test compiles and runs.
- Three of its four assertions fail at runtime: substitution does not run (the synthesized turn body still contains `{{ ... }}`), the typo workflow finishes `Completed` instead of `TaskFailed`, and `expect_err` on the bad-pattern and missing-input branches panics because `Runtime::new` always returns `Ok`.

## Step 1: Construction-time input validation

**Enables:** the bottom two assertions of the feature test (pattern violation and missing input both halt at construction with `expect_err`).

Add the `regex` crate as a direct dependency. Inside `Runtime::new`, before the existing struct construction, walk `workflow.inputs`:

```rust
for (name, spec) in &workflow.inputs {
    match state.inputs.get(name) {
        None if spec.required => {
            return Err(WorkflowError::MissingInput { name: name.clone() });
        }
        Some(value) => {
            if let Some(pattern) = &spec.pattern {
                let re = Regex::new(pattern).map_err(|_| /* surface as InputPatternMismatch */ ...)?;
                if !re.is_match(value) {
                    return Err(WorkflowError::InputPatternMismatch {
                        name: name.clone(),
                        value: value.clone(),
                        pattern: pattern.clone(),
                    });
                }
            }
        }
        None => {} // optional input, not supplied
    }
}
```

Open question handled in this step: a malformed `pattern` in the workflow file. Surface it as `InputPatternMismatch { pattern, ... }` with a `value` of `"<pattern compile error>"` so the workflow author sees which input's regex is broken without a third error variant. The feature test does not exercise this path, but do write a unit test for it.

After this step:
- The `expect_err("pattern mismatch must reject construction")` and `expect_err("missing required input must reject construction")` assertions pass.
- The happy path and unresolved-placeholder assertions still fail (substitution not yet wired).

## Step 2: `substitute` helper for strings and `toml::Value` walks

**Enables:** unit-test coverage of the substitution surface that Step 3 calls into. The feature test's runtime assertions still depend on Step 3 to wire this helper into the dispatch path.

New module `src/workflow/template.rs`:

```rust
pub struct Context {
    values: BTreeMap<String, String>,
}

impl Context {
    pub fn new() -> Self { ... }
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) { ... }
    pub fn get(&self, key: &str) -> Option<&str> { ... }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedPlaceholder {
    pub placeholder: String,
}

/// Single-pass `{{ name }}` substitution. Whitespace inside the braces is
/// trimmed. Returns the first unresolved placeholder it encounters.
pub fn substitute(text: &str, ctx: &Context) -> Result<String, UnresolvedPlaceholder>;

/// Walks every string leaf in `value`, applying `substitute` in place.
pub fn substitute_value(
    value: &mut toml::Value,
    ctx: &Context,
) -> Result<(), UnresolvedPlaceholder>;
```

Single-pass means the helper does not re-scan the output. A value that contains `{{` after substitution does not retrigger.

Unit tests in the same module:

- `substitute_replaces_named_placeholders`: `"hi {{ name }}"` with `name = "x"` returns `"hi x"`.
- `substitute_trims_whitespace_inside_braces`: `"{{name}}"`, `"{{ name }}"`, `"{{  name  }}"` all resolve.
- `substitute_returns_first_unresolved`: `"{{ a }} and {{ b }}"` with only `a` set returns `Err(UnresolvedPlaceholder { placeholder: "b" })`.
- `substitute_passes_through_text_with_no_placeholders`.
- `substitute_value_walks_nested_tables_and_arrays`: build a `toml::Value::Table` containing nested tables and arrays of strings, confirm every leaf is rewritten.
- `substitute_value_returns_first_unresolved_in_walk_order`.

After this step:
- The new module is wired into `src/workflow/mod.rs` (`pub mod template`).
- All four runtime feature-test assertions remain in their Step 1 state.

## Step 3: Dispatch-time substitution and `UnresolvedTemplate` propagation

**Enables:** the top two assertions of the feature test (substituted body, typo halts as `TaskFailed`).

Wire substitution into the run loop in `src/workflow/runtime.rs`. At task dispatch, before `synthesize_turn_file`:

1. Build a `Context` from `state.inputs` and `state.context_seed`:
   - For each `(name, value)` in `state.inputs`, insert `vars.<name> = value`.
   - If `state.context_seed.today` is `None`, populate it with the current local date as `YYYY-MM-DD` and persist (`state.write` is already called after each task; the seed write happens once at first dispatch).
   - If `state.context_seed.session_dir` is `None`, derive `docs/developer/{today}-A-{vars.topic}` when both are present and persist. When `vars.topic` is absent, leave `session_dir` unresolved; a workflow that references it will fail with `UnresolvedTemplate` at substitution time, which is the correct outcome.
   - Insert `today = ...` and `session_dir = ...` into the context using the resolved seed values.
2. Call `template::substitute(main_prompt, &ctx)`.
3. On `Err(UnresolvedPlaceholder { placeholder })`, yield `WorkflowStopReason::TaskFailed` with `WorkflowError::UnresolvedTemplate { task: task_name, placeholder }` wrapped through `Arc::new(anyhow::Error::new(...))` (matches existing `task_failed` helper).
4. On `Ok(rendered)`, pass `&rendered` to `synthesize_turn_file` instead of `main_prompt`.

The substitution helper from Step 2 is the only caller; no logic duplicated inline.

Date source: `chrono::Local::now().date_naive().format("%Y-%m-%d")`. Add `chrono` as a direct dependency with `default-features = false, features = ["clock"]`. The feature test asserts `turn_text.contains("docs/developer/") && turn_text.contains("-A-dev-cycle-workflow/design.md")`, so the exact date is not pinned.

`session_dir` derivation lives in a `resolve_context_seed(state: &mut WorkflowState, workflow: &Workflow)` helper so the same logic is reachable from a future test that supplies a pre-seeded `context_seed` and asserts no clock read happens.

After this step:
- All four feature-test assertions pass.
- `WorkflowState::write` is called once after seed resolution so the persisted state shows the resolved `today` and `session_dir`. This satisfies the design's "RFC3339 date when the workflow first ran, persisted into `state.context_seed`" requirement.
- Existing `e2e/` tests are untouched. The `e2e/09_dev_cycle/` harness lands in slice 4 and is out of scope here.

## Out of scope

Items the design names that this slice does not deliver:

- `Task::skills` field forwarding into the synthesized turn TOML. Slice 2.
- `TaskAction::ToolCall` and the `evaluation = ToolCall { ... }` evaluator. Slice 4 (depends on the `substitute_value` helper this slice exposes).
- `user.clarify` and `WorkflowStopReason::AwaitingInput`. Slice 3.
- `WorkflowStopReason::Paused` and the resume-skip path. Slice 4.
- `WorkflowError::ToolCallTaskNotImplemented`, `UnknownEvalTool`, `EvalArgsEncode`. Slices 2/4.
- The `e2e/09_dev_cycle/dev_cycle.toml` fixture. Slice 4.
