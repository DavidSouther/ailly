# Feature Test 4: Workflow pause and resume

*Draft 2026-05-04*

## User Story

**Given** a workflow whose first task has `evaluation = ToolCall { tool = "fs.absent", args = { path = "{{ session_dir }}/design.md", needle = "*Draft" } }`,
**And** a `HashMapRegistry` that registers `fs.absent` against the runtime's session folder,
**And** the session folder contains `design.md` with a `*Draft 2026-05-04*` marker,
**When** the runtime runs the workflow,
**Then** the runtime drives the prompt task, producing `01_design.toml` with a recorded response,
**And** the runtime resolves `fs.absent` from the registry, serializes the substituted args to JSON, dispatches the call, and receives `"paused"`,
**And** the workflow halts with `WorkflowStopReason::Paused { task: "design", result: "paused" }`,
**And** `workflow.state.toml` shows `queue = ["design"]` with one history entry whose `result` is `"paused"`,
**When** the harness rewrites `design.md` without the `*Draft` marker and re-runs the workflow against the same conversation root and persisted state,
**Then** the runtime pops `design`, finds the existing turn file in `state.history`, skips prompt synthesis and `Generator::run`, and runs only the evaluation,
**And** the eval returns `"cleared"`, routing via `next.cleared` to the next task,
**And** `01_design.toml` is byte-identical before and after the resume.

## Sequencing

Serial slice 4 of 4. Lands after slices 1, 2, and 3 are green. This slice depends on slice 1's substitution helper for ToolCall args, exercises slice 2's `Task::skills` field on each dev-cycle task, and shares the requeue-and-persist machinery validated by slice 3's `AwaitingInput` story.

## Acceptance Criteria

- `TaskAction::ToolCall { tool, args }` lands on the schema. `task = ToolCall` errors at execution with `WorkflowError::ToolCallTaskNotImplemented`. `evaluation = ToolCall` dispatches.
- `WorkflowStopReason::Paused { task, result }` lands on the runtime.
- `Runtime::new` accepts a `tool_registry: Arc<dyn crate::engine::ToolRegistry>` parameter, threading it both to the eval-tool dispatch path and to the prompt-turn `Generator`.
- The `fs.absent` tool ships in this slice as `crate::tools::FsAbsent`, scoped to the runtime's conversation root, returning `"cleared"` when the file lacks the needle, `"paused"` when it contains the needle, and `Err(ToolError)` when the file is unreadable.
- The runtime's resume path inspects `state.history` for a matching task entry. When the entry's referenced turn file already carries a `[[response]]`, the prompt is skipped and only the evaluation runs.
- The resume run produces no second `record_response` call. Byte equality of the turn file before and after resume is the metric.
- The reserved result string `"paused"` is checked before `next` is consulted. A workflow author who adds `next.paused = "..."` is overridden.

## Executable Feature Test

Drop into `src/workflow/runtime.rs` `mod tests`. Expect failure against `TaskAction::ToolCall`, `WorkflowStopReason::Paused`, the `tool_registry` parameter on `Runtime::new`, and the `crate::tools::FsAbsent` type.

```rust
#[tokio::test]
async fn workflow_pauses_on_draft_marker_then_resumes_byte_identically_after_clear() {
    use crate::engine::{HashMapRegistry, ToolRegistry};
    use crate::tools::FsAbsent;

    let fs = mem_fs! {
        "root": {
            "design.md": "# Title\n\n*Draft 2026-05-04*\n\nbody\n",
        }
    };
    let root = fs.join("root").unwrap();

    let mut inputs = BTreeMap::new();
    inputs.insert(
        "topic".to_string(),
        crate::workflow::schema::InputSpec {
            description: "slug".to_string(),
            required: true,
            pattern: None,
        },
    );

    let mut design_args = toml::value::Table::new();
    design_args.insert(
        "path".to_string(),
        toml::Value::String("design.md".to_string()),
    );
    design_args.insert(
        "needle".to_string(),
        toml::Value::String("*Draft".to_string()),
    );

    let workflow = Workflow {
        name: "ailly-dev-cycle".to_string(),
        start: "design".to_string(),
        inputs,
        tasks: vec![
            Task {
                name: "design".to_string(),
                skills: vec!["developer:design".to_string()],
                task: TaskAction::Prompt {
                    text: "Produce design.md.".to_string(),
                },
                evaluation: Some(TaskAction::ToolCall {
                    tool: "fs.absent".to_string(),
                    args: toml::Value::Table(design_args),
                }),
                next: BTreeMap::from([("cleared".to_string(), "feature_test".to_string())]),
            },
            Task {
                name: "feature_test".to_string(),
                skills: vec!["developer:feature-test".to_string()],
                task: TaskAction::Prompt {
                    text: "Produce feature-test.md.".to_string(),
                },
                evaluation: None,
                next: BTreeMap::new(),
            },
        ],
    };

    let mut state = WorkflowState::initial(&workflow);
    state.inputs.insert("topic".to_string(), "demo".to_string());

    let mut registry = HashMapRegistry::default();
    registry.insert("fs.absent", Arc::new(FsAbsent::new(root.clone())));
    let registry: Arc<dyn ToolRegistry> = Arc::new(registry);

    // First run: paused on draft marker.
    let runtime = Runtime::new_with_registry(
        workflow.clone(),
        state,
        root.clone(),
        Arc::new(Noop::default()),
        registry.clone(),
        Settings::default(),
    )
    .expect("construction OK");
    let events: Vec<WorkflowEvent> = runtime.run().collect().await;

    match events.last().unwrap() {
        WorkflowEvent::WorkflowFinished {
            reason: WorkflowStopReason::Paused { task, result },
        } => {
            assert_eq!(task, "design");
            assert_eq!(result, "paused");
        }
        other => panic!("expected Paused, got {other:?}"),
    }

    let design_turn_name = root
        .read_dir()
        .unwrap()
        .map(|e| e.filename())
        .find(|n| n.ends_with("_design.toml"))
        .expect("design turn file written");
    let design_turn_path = root.join(&design_turn_name).unwrap();
    let turn_text_before = design_turn_path.read_to_string().unwrap();
    assert!(turn_text_before.contains("[[response]]"));

    let persisted = WorkflowState::read(&root).unwrap().expect("state written");
    assert_eq!(persisted.queue.front().map(String::as_str), Some("design"));
    assert_eq!(persisted.history.len(), 1);
    assert_eq!(persisted.history[0].task, "design");
    assert_eq!(persisted.history[0].result, "paused");

    // Harness clears the draft marker.
    root.join("design.md")
        .unwrap()
        .write("# Title\n\nbody\n")
        .unwrap();

    // Second run: resume past the gate without re-running the prompt.
    let runtime2 = Runtime::new_with_registry(
        workflow,
        persisted,
        root.clone(),
        Arc::new(Noop::default()),
        registry,
        Settings::default(),
    )
    .expect("construction OK");
    let events2: Vec<WorkflowEvent> = runtime2.run().collect().await;

    // The design prompt must NOT re-emit a turn-Started event for its main turn,
    // since the recorded response is reused. Only the eval runs against the tool.
    let main_turn_path_str = design_turn_path.as_str().to_string();
    let prompt_started_again = events2.iter().any(|e| matches!(
        e,
        WorkflowEvent::TaskTurn(TurnEvent::Started { path }) if path.as_str() == main_turn_path_str
    ));
    assert!(
        !prompt_started_again,
        "resume must skip prompt synthesis for the design task"
    );

    // Workflow advances to feature_test, then completes.
    let started_names: Vec<String> = events2
        .iter()
        .filter_map(|e| match e {
            WorkflowEvent::TaskStarted { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    assert!(
        started_names.contains(&"feature_test".to_string()),
        "resume must route cleared -> feature_test; got {started_names:?}"
    );
    assert!(matches!(
        events2.last().unwrap(),
        WorkflowEvent::WorkflowFinished {
            reason: WorkflowStopReason::Completed,
        }
    ));

    // Byte equality before/after resume.
    let turn_text_after = design_turn_path.read_to_string().unwrap();
    assert_eq!(
        turn_text_before, turn_text_after,
        "01_design.toml must be byte-identical across resume"
    );
}
```

## Stop

Implementation of this slice does not begin until the prior three slices are green. The implementer clears the draft marker, drops the test into `mod tests`, and runs `developer:plan` against the failing test.
