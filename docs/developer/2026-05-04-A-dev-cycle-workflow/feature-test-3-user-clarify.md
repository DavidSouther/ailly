# Feature Test 3: `user.clarify` tool and `WorkflowStopReason::AwaitingInput`

*Draft 2026-05-04*

## User Story

**Given** a workflow whose first task is a prompt that triggers a `user.clarify` tool call with `{ question: "What database backs the inbox queue?" }`,
**And** an engine that emits exactly that tool call on its first turn,
**When** the runtime runs the workflow against an empty `state.clarifications`,
**Then** the workflow halts with `WorkflowStopReason::AwaitingInput { task: "design", question: "What database backs the inbox queue?" }`,
**And** `workflow.state.toml` on disk shows `pending_clarification` populated with the same task and question,
**And** the gated task is at the head of `state.queue` for resume,
**When** the harness writes the answer `"Postgres"` into `state.clarifications.design[question]`, clears `pending_clarification`, and re-runs the workflow,
**Then** the runtime replays the prompt turn, the same `user.clarify` call returns `"Postgres"` to the model without halting,
**And** the workflow proceeds to the next task,
**And** a second `user.clarify` call within the same task with the same question string returns the same answer without re-halting,
**And** two distinct questions in the same task each occupy their own slot in `state.clarifications.design`.

## Sequencing

Cohort: parallel slice 3 of 3. Independent of slices 1 and 2. Slice 4 (pause and resume) shares the requeue-and-persist plumbing this slice introduces but uses a different stop reason and a different gate source.

## Acceptance Criteria

- `WorkflowStopReason::AwaitingInput { task, question }` lands on the runtime.
- `WorkflowState` persists `pending_clarification: Option<PendingClarification>` and `clarifications: BTreeMap<String, BTreeMap<String, String>>`.
- The runtime registers `user.clarify` into every `Runtime`'s tool registry by default, intercepting the call before it reaches a harness-supplied registry entry.
- The interceptor consults `state.clarifications[task][question]`, returns the cached answer when present, and halts with `AwaitingInput` plus persisted `pending_clarification` when absent.
- On resume, the prompt turn replays against the recorded transcript and the cached answer is returned to the model without halting.

## Precondition

The Noop engine must be able to emit a `user.clarify` tool call on demand so this test does not require a real `CompletionModel`. If `Noop` does not yet support synthetic tool emission (tracked in `TASKS.md` under "Tools"), this slice ships the minimum surface required: a `Noop::with_tool_call(name, args_json)` constructor that emits one `ToolCall` on first turn and then `end_turn`. The slice's plan must include this surface before the failing feature test can become green.

## Executable Feature Test

Drop into `src/workflow/runtime.rs` `mod tests`. Expect failure against `WorkflowStopReason::AwaitingInput`, `state.clarifications`, `state.pending_clarification`, and the `user.clarify` interceptor.

```rust
#[tokio::test]
async fn user_clarify_halts_with_awaiting_input_then_replays_recorded_answer_on_resume() {
    let fs = mem_fs! { "root": {} };
    let root = fs.join("root").unwrap();

    let question = "What database backs the inbox queue?";

    let workflow = Workflow {
        name: "asks".to_string(),
        start: "design".to_string(),
        inputs: BTreeMap::new(),
        tasks: vec![
            Task {
                name: "design".to_string(),
                skills: vec![],
                task: TaskAction::Prompt {
                    text: format!("Call user.clarify with question {:?}.", question),
                },
                evaluation: None,
                next: BTreeMap::from([("end_turn".to_string(), "second".to_string())]),
            },
            Task {
                name: "second".to_string(),
                skills: vec![],
                task: TaskAction::Prompt { text: "Done.".to_string() },
                evaluation: None,
                next: BTreeMap::new(),
            },
        ],
    };
    let state = WorkflowState::initial(&workflow);

    // First run: engine emits the user.clarify call, runtime intercepts and halts.
    let args_json = serde_json::json!({ "question": question }).to_string();
    let engine = Arc::new(Noop::with_tool_call("user.clarify", args_json.clone()));
    let runtime = Runtime::new(
        workflow.clone(),
        state,
        root.clone(),
        engine,
        Settings::default(),
    )
    .expect("construction OK");
    let events: Vec<WorkflowEvent> = runtime.run().collect().await;

    match events.last().unwrap() {
        WorkflowEvent::WorkflowFinished {
            reason:
                WorkflowStopReason::AwaitingInput {
                    task,
                    question: q,
                },
        } => {
            assert_eq!(task, "design");
            assert_eq!(q, question);
        }
        other => panic!("expected AwaitingInput, got {other:?}"),
    }

    let persisted = WorkflowState::read(&root).unwrap().expect("state written");
    assert_eq!(persisted.queue.front().map(String::as_str), Some("design"));
    let pending = persisted
        .pending_clarification
        .as_ref()
        .expect("pending_clarification populated");
    assert_eq!(pending.task, "design");
    assert_eq!(pending.question, question);
    assert!(
        persisted
            .clarifications
            .get("design")
            .map(|m| m.is_empty())
            .unwrap_or(true),
        "no answer recorded yet"
    );

    // Harness records the answer, clears pending, resumes.
    let mut resumed = persisted;
    resumed
        .clarifications
        .entry("design".to_string())
        .or_default()
        .insert(question.to_string(), "Postgres".to_string());
    resumed.pending_clarification = None;

    let engine2 = Arc::new(Noop::with_tool_call("user.clarify", args_json));
    let runtime2 = Runtime::new(
        workflow,
        resumed,
        root.clone(),
        engine2,
        Settings::default(),
    )
    .expect("construction OK");
    let events2: Vec<WorkflowEvent> = runtime2.run().collect().await;

    let started_names: Vec<String> = events2
        .iter()
        .filter_map(|e| match e {
            WorkflowEvent::TaskStarted { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    assert!(
        started_names.contains(&"second".to_string()),
        "resume must reach `second` after answer is recorded; got {started_names:?}"
    );
    assert!(matches!(
        events2.last().unwrap(),
        WorkflowEvent::WorkflowFinished {
            reason: WorkflowStopReason::Completed,
        }
    ));

    let final_state = WorkflowState::read(&root).unwrap().expect("state written");
    assert!(
        final_state.pending_clarification.is_none(),
        "pending_clarification must be cleared after resume succeeds"
    );
    assert_eq!(
        final_state
            .clarifications
            .get("design")
            .and_then(|m| m.get(question))
            .map(String::as_str),
        Some("Postgres")
    );
}
```

The exact `Noop::with_tool_call` constructor is the synthetic-tool-emission surface the slice must add. If a different shape (configuration struct, builder) fits the existing `Noop` better, the implementer adapts the test to match.

## Stop

Implementation does not begin in this session. The implementer clears the draft marker and runs `developer:plan` next.
