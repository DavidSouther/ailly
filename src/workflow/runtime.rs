use std::pin::Pin;
use std::sync::Arc;

use futures::{Stream, StreamExt};
use tokio_util::sync::CancellationToken;
use vfs::VfsPath;

use crate::content::{
    AssistantResponse, ContentError, ContentMeta, Conversation, ConversationTurn,
};
use crate::engine::{Engine, EngineEvent, EngineInput, Generator, Settings, StopReason, TurnEvent};
use crate::workflow::schema::{TaskAction, Workflow};
use crate::workflow::state::{HistoryEntry, WorkflowState};

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum WorkflowEvent {
    TaskStarted {
        name: String,
        turn: VfsPath,
    },
    TaskTurn(TurnEvent),
    TaskFinished {
        name: String,
        result: String,
        next: Option<String>,
    },
    WorkflowFinished {
        reason: WorkflowStopReason,
    },
}

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum WorkflowStopReason {
    Completed,
    UnknownNext {
        task: String,
        result: String,
    },
    TaskFailed {
        task: String,
        error: Arc<anyhow::Error>,
    },
    /// The workflow finished its queue but `workflow.state.toml` could not be
    /// persisted. Distinct from `TaskFailed` because no task is responsible.
    StatePersistFailed {
        error: Arc<anyhow::Error>,
    },
    Cancelled,
}

pub struct Runtime {
    workflow: Workflow,
    state: WorkflowState,
    conversation_root: VfsPath,
    engine: Arc<dyn Engine>,
    settings: Settings,
    cancel: CancellationToken,
}

impl Runtime {
    /// Build a Runtime ready to drive `workflow` against `engine`.
    ///
    /// `state` is either freshly initialized with `[workflow.start]` or
    /// read from `workflow.state.toml` at `conversation_root`. The runtime
    /// persists `state` after each task.
    pub fn new(
        workflow: Workflow,
        state: WorkflowState,
        conversation_root: VfsPath,
        engine: Arc<dyn Engine>,
        settings: Settings,
    ) -> Self {
        Self {
            workflow,
            state,
            conversation_root,
            engine,
            settings,
            cancel: CancellationToken::new(),
        }
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    pub fn run(self) -> Pin<Box<dyn Stream<Item = WorkflowEvent> + Send>> {
        Box::pin(async_stream::stream! {
            let Self {
                workflow,
                mut state,
                conversation_root,
                engine,
                settings,
                cancel,
            } = self;

            loop {
                if cancel.is_cancelled() {
                    yield WorkflowEvent::WorkflowFinished {
                        reason: WorkflowStopReason::Cancelled,
                    };
                    return;
                }

                let Some(task_name) = state.queue.pop_front() else {
                    if let Err(error) = state.write(&conversation_root) {
                        yield WorkflowEvent::WorkflowFinished {
                            reason: WorkflowStopReason::StatePersistFailed {
                                error: Arc::new(error.into()),
                            },
                        };
                        return;
                    }
                    yield WorkflowEvent::WorkflowFinished {
                        reason: WorkflowStopReason::Completed,
                    };
                    return;
                };

                let task = match workflow.find_task(&task_name) {
                    Ok(t) => t.clone(),
                    Err(_) => {
                        requeue_and_persist(&mut state, &conversation_root, &task_name);
                        yield WorkflowEvent::WorkflowFinished {
                            reason: WorkflowStopReason::UnknownNext {
                                task: task_name,
                                result: String::new(),
                            },
                        };
                        return;
                    }
                };

                let TaskAction::Prompt { text: main_prompt } = &task.task;
                let turn_path = match synthesize_turn_file(
                    &conversation_root,
                    &task.name,
                    &task.name,
                    main_prompt,
                )
                .await
                {
                    Ok(p) => p,
                    Err(error) => {
                        requeue_and_persist(&mut state, &conversation_root, &task_name);
                        yield WorkflowEvent::WorkflowFinished {
                            reason: task_failed(&task_name, error),
                        };
                        return;
                    }
                };

                yield WorkflowEvent::TaskStarted {
                    name: task_name.clone(),
                    turn: turn_path.clone(),
                };

                let convo = match Conversation::single_turn(turn_path.clone()).await {
                    Ok(c) => c,
                    Err(error) => {
                        requeue_and_persist(&mut state, &conversation_root, &task_name);
                        yield WorkflowEvent::WorkflowFinished {
                            reason: task_failed(&task_name, error),
                        };
                        return;
                    }
                };

                let generator = Generator::new(convo, engine.clone(), settings.clone());
                let mut events = generator.run();
                let mut last_stop_reason: Option<StopReason> = None;
                let mut failure: Option<Arc<anyhow::Error>> = None;

                while let Some(ev) = events.next().await {
                    match &ev {
                        TurnEvent::Failed { error, .. } => {
                            failure = Some(error.clone());
                        }
                        TurnEvent::Finished { stop_reason, .. } => {
                            last_stop_reason = Some(stop_reason.clone());
                        }
                        _ => {}
                    }
                    yield WorkflowEvent::TaskTurn(ev);
                }

                if let Some(error) = failure {
                    requeue_and_persist(&mut state, &conversation_root, &task_name);
                    yield WorkflowEvent::WorkflowFinished {
                        reason: WorkflowStopReason::TaskFailed {
                            task: task_name,
                            error,
                        },
                    };
                    return;
                }

                let eval_result: String;
                if let Some(TaskAction::Prompt { text: eval_text }) = &task.evaluation {
                    let eval_label = format!("{}_eval", task.name);
                    let eval_path = match synthesize_turn_file(
                        &conversation_root,
                        &eval_label,
                        &eval_label,
                        eval_text,
                    )
                    .await
                    {
                        Ok(p) => p,
                        Err(error) => {
                            requeue_and_persist(&mut state, &conversation_root, &task_name);
                            yield WorkflowEvent::WorkflowFinished {
                                reason: task_failed(&task_name, error),
                            };
                            return;
                        }
                    };

                    yield WorkflowEvent::TaskTurn(TurnEvent::Started {
                        path: eval_path.clone(),
                    });

                    let mut convo = match Conversation::from_paths(vec![
                        turn_path.clone(),
                        eval_path.clone(),
                    ])
                    .await
                    {
                        Ok(c) => c,
                        Err(error) => {
                            requeue_and_persist(&mut state, &conversation_root, &task_name);
                            yield WorkflowEvent::WorkflowFinished {
                                reason: task_failed(&task_name, error),
                            };
                            return;
                        }
                    };

                    // Eval turn is index 1 by construction of the from_paths call above.
                    // Every later `convo.turn(1)` / `record_response(1, ...)` relies on
                    // this ordering.
                    debug_assert_eq!(convo.turn_count(), 2);
                    const EVAL_TURN_IDX: usize = 1;
                    let history = EngineInput::with_history(convo.history_for(convo.turn(EVAL_TURN_IDX)));

                    let mut events = match engine.stream(history, &settings, &[], eval_path.as_str()) {
                        Ok(s) => s,
                        Err(error) => {
                            let arc_err = Arc::new(error);
                            yield WorkflowEvent::TaskTurn(TurnEvent::Failed {
                                path: eval_path.clone(),
                                error: arc_err.clone(),
                            });
                            requeue_and_persist(&mut state, &conversation_root, &task_name);
                            yield WorkflowEvent::WorkflowFinished {
                                reason: WorkflowStopReason::TaskFailed {
                                    task: task_name,
                                    error: arc_err,
                                },
                            };
                            return;
                        }
                    };

                    let mut eval_final_text: Option<String> = None;
                    while let Some(ev) = events.next().await {
                        match ev {
                            EngineEvent::Text(t) => {
                                yield WorkflowEvent::TaskTurn(TurnEvent::Delta {
                                    path: eval_path.clone(),
                                    text: t,
                                });
                            }
                            EngineEvent::ToolCall(call) => {
                                convo.record_tool_call(EVAL_TURN_IDX, call.clone());
                                yield WorkflowEvent::TaskTurn(TurnEvent::ToolCall {
                                    path: eval_path.clone(),
                                    call,
                                });
                            }
                            EngineEvent::ToolResult(result) => {
                                convo.record_tool_result(EVAL_TURN_IDX, result.clone());
                                yield WorkflowEvent::TaskTurn(TurnEvent::ToolResult {
                                    path: eval_path.clone(),
                                    result,
                                });
                            }
                            EngineEvent::Final(mut r) => {
                                r.engine_name = engine.name().into();
                                let response: AssistantResponse = (&r).into();
                                convo.record_response(EVAL_TURN_IDX, response);
                                if let Err(err) = convo.turn(EVAL_TURN_IDX).write().await {
                                    let arc_err =
                                        Arc::new(anyhow::Error::new(err));
                                    yield WorkflowEvent::TaskTurn(TurnEvent::Failed {
                                        path: eval_path.clone(),
                                        error: arc_err.clone(),
                                    });
                                    requeue_and_persist(
                                        &mut state,
                                        &conversation_root,
                                        &task_name,
                                    );
                                    yield WorkflowEvent::WorkflowFinished {
                                        reason: WorkflowStopReason::TaskFailed {
                                            task: task_name,
                                            error: arc_err,
                                        },
                                    };
                                    return;
                                }
                                yield WorkflowEvent::TaskTurn(TurnEvent::Finished {
                                    path: eval_path.clone(),
                                    response: r.text.clone(),
                                    stop_reason: r.stop_reason,
                                    usage: r.usage,
                                });
                                eval_final_text = Some(r.text);
                                break;
                            }
                        }
                    }

                    eval_result = match eval_final_text {
                        Some(t) => t.trim().to_string(),
                        None => "error".to_string(),
                    };
                } else {
                    eval_result = last_stop_reason
                        .as_ref()
                        .map(StopReason::to_string)
                        .unwrap_or_else(|| "error".to_string());
                }

                state.history.push(HistoryEntry {
                    task: task_name.clone(),
                    turn: turn_path.filename(),
                    result: eval_result.clone(),
                });
                state.last_result = Some(eval_result.clone());

                let next_in_map = task.next.get(&eval_result).cloned();
                let unknown_next = next_in_map.is_none() && !task.next.is_empty();

                if let Some(n) = next_in_map {
                    state.queue.push_back(n);
                }

                if let Err(error) = state.write(&conversation_root) {
                    yield WorkflowEvent::WorkflowFinished {
                        reason: task_failed(&task_name, error),
                    };
                    return;
                }

                yield WorkflowEvent::TaskFinished {
                    name: task_name.clone(),
                    result: eval_result.clone(),
                    next: state.queue.front().cloned(),
                };

                if unknown_next {
                    yield WorkflowEvent::WorkflowFinished {
                        reason: WorkflowStopReason::UnknownNext {
                            task: task_name,
                            result: eval_result,
                        },
                    };
                    return;
                }
            }
        })
    }
}

/// Build a `TaskFailed` stop reason from any error.
fn task_failed(task: &str, error: impl Into<anyhow::Error>) -> WorkflowStopReason {
    WorkflowStopReason::TaskFailed {
        task: task.to_string(),
        error: Arc::new(error.into()),
    }
}

/// Restore `task_name` to the head of the queue and best-effort persist
/// state, so a re-run picks up where the failure interrupted.
fn requeue_and_persist(state: &mut WorkflowState, root: &VfsPath, task_name: &str) {
    state.queue.push_front(task_name.to_string());
    let _ = state.write(root);
}

async fn synthesize_turn_file(
    root: &VfsPath,
    suffix: &str,
    step: &str,
    prompt: &str,
) -> Result<VfsPath, ContentError> {
    let next_n = find_next_n(root)?;

    let filename = format!("{:02}_{}.toml", next_n, suffix);
    let path = root
        .join(&filename)
        .map_err(|source| ContentError::ResolvePath {
            path: filename.clone(),
            source,
        })?;

    let meta = ContentMeta::with_step(step);
    let turn = ConversationTurn::new(path.clone(), meta, Vec::new(), prompt.to_string());
    turn.write().await?;

    Ok(path)
}

fn find_next_n(root: &VfsPath) -> Result<u32, ContentError> {
    let mut max_n: u32 = 0;
    let entries = root.read_dir().map_err(|source| ContentError::ListDir {
        path: root.as_str().to_string(),
        source,
    })?;
    for entry in entries {
        if !entry.is_file().unwrap_or(false) {
            continue;
        }
        let name = entry.filename();
        if !name.ends_with(".toml") {
            continue;
        }
        let prefix: String = name.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(n) = prefix.parse::<u32>()
            && n > max_n
        {
            max_n = n;
        }
    }
    let next_n = max_n + 1;
    if next_n > 99 {
        Err(ContentError::NextNWidthExceeded {
            path: root.as_str().to_string(),
        })
    } else {
        Ok(next_n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Noop;
    use crate::mem_fs;
    use crate::workflow::schema::{Task, TaskAction, Workflow};
    use crate::workflow::state::WorkflowState;
    use std::collections::BTreeMap;

    fn task(name: &str, prompt: &str, next: &[(&str, &str)]) -> Task {
        let mut next_map = BTreeMap::new();
        for (k, v) in next {
            next_map.insert((*k).to_string(), (*v).to_string());
        }
        Task {
            name: name.to_string(),
            task: TaskAction::Prompt {
                text: prompt.to_string(),
            },
            evaluation: None,
            next: next_map,
        }
    }

    #[tokio::test]
    async fn one_task_workflow_completes_with_completed_reason() {
        let fs = mem_fs! { "root": {} };
        let root = fs.join("root").unwrap();

        let workflow = Workflow {
            name: "lone".to_string(),
            start: "only".to_string(),
            tasks: vec![task("only", "Do it.", &[])],
        };
        let state = WorkflowState::initial(&workflow);

        let runtime = Runtime::new(
            workflow,
            state,
            root.clone(),
            Arc::new(Noop::default()),
            Settings::default(),
        );
        let events: Vec<WorkflowEvent> = runtime.run().collect().await;

        assert!(matches!(
            events.first(),
            Some(WorkflowEvent::TaskStarted { .. })
        ));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, WorkflowEvent::TaskTurn(TurnEvent::Started { .. })))
        );
        let finished_idx = events
            .iter()
            .position(|e| {
                matches!(
                    e,
                    WorkflowEvent::TaskFinished { result, next, .. }
                        if result == "end_turn" && next.is_none()
                )
            })
            .expect("TaskFinished present");
        let last = events.last().expect("at least one event");
        assert!(matches!(
            last,
            WorkflowEvent::WorkflowFinished {
                reason: WorkflowStopReason::Completed,
            }
        ));
        assert!(finished_idx < events.len() - 1);

        let entries: Vec<String> = root.read_dir().unwrap().map(|e| e.filename()).collect();
        assert!(
            entries.iter().any(|n| n.ends_with("_only.toml")),
            "synthesized turn missing: {entries:?}"
        );
        assert!(entries.contains(&"workflow.state.toml".to_string()));

        let reloaded = WorkflowState::read(&root).unwrap().expect("state written");
        assert!(reloaded.queue.is_empty());
        assert_eq!(reloaded.history.len(), 1);
        assert_eq!(reloaded.history[0].task, "only");
        assert_eq!(reloaded.history[0].result, "end_turn");
    }

    #[tokio::test]
    async fn two_task_workflow_runs_first_then_second() {
        let fs = mem_fs! { "root": {} };
        let root = fs.join("root").unwrap();

        let workflow = Workflow {
            name: "basic".to_string(),
            start: "first".to_string(),
            tasks: vec![
                task("first", "First task.", &[("end_turn", "second")]),
                task("second", "Second task.", &[]),
            ],
        };
        let state = WorkflowState::initial(&workflow);

        let runtime = Runtime::new(
            workflow,
            state,
            root.clone(),
            Arc::new(Noop::default()),
            Settings::default(),
        );
        let events: Vec<WorkflowEvent> = runtime.run().collect().await;

        let started_names: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                WorkflowEvent::TaskStarted { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            started_names,
            vec!["first".to_string(), "second".to_string()]
        );

        assert!(matches!(
            events.last().unwrap(),
            WorkflowEvent::WorkflowFinished {
                reason: WorkflowStopReason::Completed,
            }
        ));

        let mut entries: Vec<String> = root.read_dir().unwrap().map(|e| e.filename()).collect();
        entries.sort();
        let turn_files: Vec<&String> = entries
            .iter()
            .filter(|n| n.ends_with(".toml") && **n != "workflow.state.toml")
            .collect();
        assert_eq!(turn_files.len(), 2);
        assert!(turn_files[0].ends_with("_first.toml"));
        assert!(turn_files[1].ends_with("_second.toml"));

        let reloaded = WorkflowState::read(&root).unwrap().expect("state written");
        assert!(reloaded.queue.is_empty());
        assert_eq!(reloaded.history.len(), 2);
        assert_eq!(reloaded.history[0].task, "first");
        assert_eq!(reloaded.history[1].task, "second");
        assert_eq!(reloaded.last_result.as_deref(), Some("end_turn"));
    }

    #[tokio::test]
    async fn missing_start_task_yields_unknown_next_and_preserves_queue() {
        let fs = mem_fs! { "root": {} };
        let root = fs.join("root").unwrap();

        let workflow = Workflow {
            name: "broken".to_string(),
            start: "nope".to_string(),
            tasks: vec![task("only", "Do it.", &[])],
        };
        let state = WorkflowState::initial(&workflow);

        let runtime = Runtime::new(
            workflow,
            state,
            root.clone(),
            Arc::new(Noop::default()),
            Settings::default(),
        );
        let events: Vec<WorkflowEvent> = runtime.run().collect().await;

        match events.last().unwrap() {
            WorkflowEvent::WorkflowFinished {
                reason: WorkflowStopReason::UnknownNext { task, .. },
            } => assert_eq!(task, "nope"),
            other => panic!("expected UnknownNext, got {other:?}"),
        }

        let reloaded = WorkflowState::read(&root).unwrap().expect("state written");
        assert_eq!(reloaded.queue.front().map(String::as_str), Some("nope"));
    }

    fn noop_with(response: &str) -> Noop {
        Noop {
            chunk: crate::engine::noop::DEFAULT_CHUNK_BYTES,
            override_response: Some(response.to_string()),
        }
    }

    #[tokio::test]
    async fn task_with_prompt_evaluation_routes_on_trimmed_response() {
        let fs = mem_fs! { "root": {} };
        let root = fs.join("root").unwrap();

        let mut first = task("first", "First task.", &[("approved", "second")]);
        first.evaluation = Some(TaskAction::Prompt {
            text: "Reply with one word: approved.".to_string(),
        });
        let second = task("second", "Second task.", &[]);
        let workflow = Workflow {
            name: "evald".to_string(),
            start: "first".to_string(),
            tasks: vec![first, second],
        };
        let state = WorkflowState::initial(&workflow);

        let runtime = Runtime::new(
            workflow,
            state,
            root.clone(),
            Arc::new(noop_with("approved")),
            Settings::default(),
        );
        let events: Vec<WorkflowEvent> = runtime.run().collect().await;

        let started_names: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                WorkflowEvent::TaskStarted { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            started_names,
            vec!["first".to_string(), "second".to_string()],
            "exactly one TaskStarted per logical task; eval must not emit a second one"
        );

        let eval_path = root.join("02_first_eval.toml").unwrap();
        let eval_path_str = eval_path.as_str().to_string();
        let eval_started = events.iter().any(|e| matches!(
            e,
            WorkflowEvent::TaskTurn(TurnEvent::Started { path }) if path.as_str() == eval_path_str
        ));
        assert!(eval_started, "eval Started event missing");
        let eval_finished = events.iter().any(|e| {
            matches!(
                e,
                WorkflowEvent::TaskTurn(TurnEvent::Finished { path, .. })
                    if path.as_str() == eval_path_str
            )
        });
        assert!(eval_finished, "eval Finished event missing");

        let routed = events.iter().any(|e| {
            matches!(
                e,
                WorkflowEvent::TaskFinished { name, result, next }
                    if name == "first"
                        && result == "approved"
                        && next.as_deref() == Some("second")
            )
        });
        assert!(routed, "first should route on approved -> second");

        assert!(matches!(
            events.last().unwrap(),
            WorkflowEvent::WorkflowFinished {
                reason: WorkflowStopReason::Completed,
            }
        ));

        let eval_text = eval_path.read_to_string().unwrap();
        assert!(
            eval_text.contains("[[response]]"),
            "eval file missing recorded response: {eval_text}"
        );
        assert!(
            eval_text.contains("role = \"assistant\""),
            "eval file missing assistant role: {eval_text}"
        );
    }

    #[tokio::test]
    async fn task_with_prompt_evaluation_unknown_label_yields_unknown_next() {
        let fs = mem_fs! { "root": {} };
        let root = fs.join("root").unwrap();

        let mut first = task("first", "First task.", &[("approved", "second")]);
        first.evaluation = Some(TaskAction::Prompt {
            text: "Reply with one word: approved.".to_string(),
        });
        let second = task("second", "Second task.", &[]);
        let workflow = Workflow {
            name: "evald".to_string(),
            start: "first".to_string(),
            tasks: vec![first, second],
        };
        let state = WorkflowState::initial(&workflow);

        let runtime = Runtime::new(
            workflow,
            state,
            root.clone(),
            Arc::new(noop_with("rejected")),
            Settings::default(),
        );
        let events: Vec<WorkflowEvent> = runtime.run().collect().await;

        match events.last().unwrap() {
            WorkflowEvent::WorkflowFinished {
                reason: WorkflowStopReason::UnknownNext { task, result },
            } => {
                assert_eq!(task, "first");
                assert_eq!(result, "rejected");
            }
            other => panic!("expected UnknownNext, got {other:?}"),
        }

        let eval_text = root
            .join("02_first_eval.toml")
            .unwrap()
            .read_to_string()
            .unwrap();
        assert!(
            eval_text.contains("[[response]]"),
            "eval file missing recorded response: {eval_text}"
        );
    }
}
