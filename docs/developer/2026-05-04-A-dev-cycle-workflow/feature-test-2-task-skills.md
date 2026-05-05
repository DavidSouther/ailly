# Feature Test 2: `Task::skills`

## User Story

**Given** a workflow file declares a task with `skills = ["developer:design", "developer:thinking"]`,
**When** the runtime dispatches that task and synthesizes the turn TOML,
**Then** the file on disk carries `skills = ["developer:design", "developer:thinking"]` in the turn TOML body,
**And** the order matches the workflow declaration,
**And** when the same workflow runs against an engine and conversation that resolves SKILL.md content, the turn's resolved skill set surfaces both names through the existing skill-loading path,
**And** an unknown skill name surfaces at conversation-load time as a content error referencing the missing skill, not silently dropped.

## Sequencing

Cohort: parallel slice 2 of 3. Independent of slices 1 and 3. Slice 4 reuses this field on each task in the dev-cycle workflow.

## Acceptance Criteria

- `Task::skills: Vec<String>` lands on the schema with `#[serde(default)]`.
- The runtime's `synthesize_turn_file` writes `skills = [...]` into the synthesized TOML when the slice is non-empty, omits the field entirely when empty, and preserves authored order.
- Round-trip parity: a workflow file declaring `skills = ["x", "y"]` produces a turn TOML whose `Conversation::single_turn` load surfaces the same two names in the same order.
- Unknown skill names surface through the engine's existing skill-resolution failure mode, not as a workflow-layer fallback.

## Executable Feature Test

Drop into `src/workflow/runtime.rs` `mod tests`. Expect failure against the missing `Task::skills` field.

```rust
#[tokio::test]
async fn task_skills_propagate_into_synthesized_turn_toml_in_declared_order() {
    use crate::content::Conversation;

    let fs = mem_fs! { "root": {} };
    let root = fs.join("root").unwrap();

    let workflow = Workflow {
        name: "skilled".to_string(),
        start: "design".to_string(),
        inputs: BTreeMap::new(),
        tasks: vec![
            Task {
                name: "design".to_string(),
                skills: vec![
                    "developer:design".to_string(),
                    "developer:thinking".to_string(),
                ],
                task: TaskAction::Prompt {
                    text: "Produce design.md.".to_string(),
                },
                evaluation: None,
                next: BTreeMap::new(),
            },
            Task {
                name: "bare".to_string(),
                skills: vec![],
                task: TaskAction::Prompt {
                    text: "No skills declared.".to_string(),
                },
                evaluation: None,
                next: BTreeMap::new(),
            },
        ],
    };
    let state = WorkflowState::initial(&workflow);

    let runtime = Runtime::new(
        workflow,
        state,
        root.clone(),
        Arc::new(Noop::default()),
        Settings::default(),
    )
    .expect("construction OK");
    let _: Vec<WorkflowEvent> = runtime.run().collect().await;

    // Synthesized turn for `design` carries both skills in declared order.
    let design_name = root
        .read_dir()
        .unwrap()
        .map(|e| e.filename())
        .find(|n| n.ends_with("_design.toml"))
        .expect("design turn file written");
    let design_path = root.join(&design_name).unwrap();
    let design_text = design_path.read_to_string().unwrap();
    let skills_idx = design_text
        .find("skills")
        .expect("skills field missing in synthesized turn");
    let after = &design_text[skills_idx..];
    let dev_design_idx = after
        .find("developer:design")
        .expect("developer:design absent");
    let dev_thinking_idx = after
        .find("developer:thinking")
        .expect("developer:thinking absent");
    assert!(
        dev_design_idx < dev_thinking_idx,
        "skills must preserve declared order: {design_text}"
    );

    // Round-trip via Conversation surfaces the same two names.
    let convo = Conversation::single_turn(design_path).await.unwrap();
    let resolved: Vec<String> = convo
        .turns()
        .last()
        .expect("one turn")
        .skills()
        .iter()
        .map(|s| s.to_string())
        .collect();
    assert_eq!(
        resolved,
        vec![
            "developer:design".to_string(),
            "developer:thinking".to_string(),
        ]
    );

    // Bare task omits the skills field entirely.
    let bare_name = root
        .read_dir()
        .unwrap()
        .map(|e| e.filename())
        .find(|n| n.ends_with("_bare.toml"))
        .expect("bare turn file written");
    let bare_text = root.join(&bare_name).unwrap().read_to_string().unwrap();
    assert!(
        !bare_text.contains("skills"),
        "empty skills slice must omit the field: {bare_text}"
    );
}
```

The exact accessor used here (`turns().last().skills()`) is illustrative. The implementer should match whatever surface `Conversation` actually exposes for the resolved skill list. The behavior under test is round-trip parity through the synthesized file.

## Stop

Implementation does not begin in this session. The implementer clears the draft marker and runs `developer:plan` next.
