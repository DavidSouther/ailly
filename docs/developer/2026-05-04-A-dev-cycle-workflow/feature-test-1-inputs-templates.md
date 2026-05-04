# Feature Test 1: Workflow inputs and `{{ template }}` substitution

*Draft 2026-05-04*

## User Story

**Given** a workflow file declares `[inputs.topic]` with a kebab-case `pattern` and a non-required `[inputs.goal]`,
**And** the harness seeds `WorkflowState` with `inputs = { topic = "dev-cycle-workflow", goal = "ship pause-and-resume" }`,
**And** the workflow's first task has a prompt body referencing `{{ session_dir }}`, `{{ vars.topic }}`, `{{ vars.goal }}`, and `{{ today }}`,
**When** the runtime dispatches that task,
**Then** the synthesized turn TOML on disk contains the fully substituted prompt body with no `{{ }}` markers remaining,
**And** `session_dir` resolves to `docs/developer/{{ today }}-A-dev-cycle-workflow` using the date persisted in `state.context_seed`,
**And** a workflow whose prompt references `{{ vars.tpic }}` halts with `WorkflowStopReason::TaskFailed` carrying `WorkflowError::UnresolvedTemplate { task: "first", placeholder: "vars.tpic" }`,
**And** a harness that supplies `topic = "Bad Slug"` (violating the `^[a-z][a-z0-9-]*$` pattern) halts before the first task runs with `WorkflowError::InputPatternMismatch`,
**And** a harness that omits the required `topic` input halts before the first task runs with `WorkflowError::MissingInput`.

## Sequencing

Cohort: parallel slice 1 of 3. Independent of slices 2 and 3. Slice 4 (pause and resume) consumes the substitution helper produced here for `ToolCall` args.

## Acceptance Criteria

- `Workflow::inputs: BTreeMap<String, InputSpec>` lands on the schema.
- `WorkflowState` persists `inputs: BTreeMap<String, String>` and `context_seed: { today: String, session_dir: String }`.
- A `substitute(text: &str, ctx: &Context) -> Result<String, WorkflowError>` helper handles single-pass `{{ name }}` replacement against a flat string-keyed context, with `vars.<name>` populated from inputs and `today` plus `session_dir` derived per the design.
- The same helper is exposed for `toml::Value` walks so slice 4 can apply it to ToolCall args without re-implementing the recursion.
- Substitution runs at task dispatch, not at workflow-load time.
- `Pattern` and `MissingInput` validation runs at `Runtime::new` time, before any task dispatches.

## Executable Feature Test

Drop the following into `src/workflow/runtime.rs` `mod tests` as the first commit of this slice. Expect it to fail to compile against `WorkflowError::{UnresolvedTemplate, InputPatternMismatch, MissingInput}` and `Workflow::inputs`.

```rust
#[tokio::test]
async fn workflow_inputs_substitute_into_prompt_text_and_validate_at_construction() {
    use crate::workflow::schema::{InputSpec, Workflow};
    use crate::workflow::state::WorkflowState;

    // Happy path: substitution renders into the synthesized turn file.
    let fs = mem_fs! { "root": {} };
    let root = fs.join("root").unwrap();

    let mut inputs = BTreeMap::new();
    inputs.insert(
        "topic".to_string(),
        InputSpec {
            description: "kebab-case slug".to_string(),
            required: true,
            pattern: Some("^[a-z][a-z0-9-]*$".to_string()),
        },
    );
    inputs.insert(
        "goal".to_string(),
        InputSpec {
            description: "what is being built".to_string(),
            required: false,
            pattern: None,
        },
    );

    let workflow = Workflow {
        name: "templated".to_string(),
        start: "first".to_string(),
        inputs: inputs.clone(),
        tasks: vec![task(
            "first",
            "Build {{ session_dir }}/design.md for {{ vars.topic }} on {{ today }}: {{ vars.goal }}.",
            &[],
        )],
    };

    let mut state = WorkflowState::initial(&workflow);
    state
        .inputs
        .insert("topic".to_string(), "dev-cycle-workflow".to_string());
    state
        .inputs
        .insert("goal".to_string(), "ship pause-and-resume".to_string());

    let runtime = Runtime::new(
        workflow,
        state,
        root.clone(),
        Arc::new(Noop::default()),
        Settings::default(),
    )
    .expect("inputs validate");
    let _: Vec<WorkflowEvent> = runtime.run().collect().await;

    let turn_name = root
        .read_dir()
        .unwrap()
        .map(|e| e.filename())
        .find(|n| n.ends_with("_first.toml"))
        .expect("turn file written");
    let turn_text = root.join(&turn_name).unwrap().read_to_string().unwrap();
    assert!(
        !turn_text.contains("{{"),
        "unsubstituted placeholder remains: {turn_text}"
    );
    assert!(
        turn_text.contains("docs/developer/")
            && turn_text.contains("-A-dev-cycle-workflow/design.md"),
        "session_dir not rendered: {turn_text}"
    );
    assert!(
        turn_text.contains("ship pause-and-resume"),
        "goal not rendered: {turn_text}"
    );

    // Unresolved placeholder surfaces as TaskFailed.
    let fs2 = mem_fs! { "root": {} };
    let root2 = fs2.join("root").unwrap();
    let workflow2 = Workflow {
        name: "typo".to_string(),
        start: "first".to_string(),
        inputs: inputs.clone(),
        tasks: vec![task("first", "{{ vars.tpic }}", &[])],
    };
    let mut state2 = WorkflowState::initial(&workflow2);
    state2
        .inputs
        .insert("topic".to_string(), "dev-cycle-workflow".to_string());
    let runtime2 = Runtime::new(
        workflow2,
        state2,
        root2,
        Arc::new(Noop::default()),
        Settings::default(),
    )
    .expect("construction OK; substitution runs at dispatch");
    let events2: Vec<WorkflowEvent> = runtime2.run().collect().await;
    match events2.last().unwrap() {
        WorkflowEvent::WorkflowFinished {
            reason: WorkflowStopReason::TaskFailed { task, error },
        } => {
            assert_eq!(task, "first");
            let msg = format!("{error:#}");
            assert!(
                msg.contains("vars.tpic"),
                "error must name the unresolved placeholder: {msg}"
            );
        }
        other => panic!("expected TaskFailed, got {other:?}"),
    }

    // Pattern violation halts at construction.
    let mut bad_inputs = WorkflowState::initial(&workflow);
    bad_inputs.inputs.insert("topic".to_string(), "Bad Slug".to_string());
    let err = Runtime::new(
        workflow.clone(),
        bad_inputs,
        root.clone(),
        Arc::new(Noop::default()),
        Settings::default(),
    )
    .expect_err("pattern mismatch must reject construction");
    let msg = format!("{err:#}");
    assert!(msg.contains("topic") && msg.contains("Bad Slug"), "{msg}");

    // Missing required input halts at construction.
    let empty = WorkflowState::initial(&workflow);
    let err2 = Runtime::new(
        workflow,
        empty,
        root,
        Arc::new(Noop::default()),
        Settings::default(),
    )
    .expect_err("missing required input must reject construction");
    let msg2 = format!("{err2:#}");
    assert!(msg2.contains("topic"), "{msg2}");
}
```

## Stop

Implementation does not begin in this session. The implementer of this slice clears the `*Draft 2026-05-04*` marker, drops the test into `mod tests`, and runs `developer:plan` against the failing test.
