use std::pin::Pin;
use std::sync::Arc;

use futures::{Stream, StreamExt};
use tokio_util::sync::CancellationToken;
use vfs::VfsPath;

use crate::content::{
    AssistantResponse, ContentError, ContentMeta, Conversation, ConversationTurn,
};
use crate::engine::{
    Engine, EngineEvent, EngineInput, Generator, Settings, StopReason, ToolRegistry, TurnEvent,
};
use crate::workflow::schema::{TaskAction, Workflow, WorkflowError};
use crate::workflow::state::{HistoryEntry, WorkflowState};
use crate::workflow::template::{self, Context, UnresolvedPlaceholder};

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
    /// The most recent evaluation returned the reserved string `"paused"`.
    /// State is persisted with `task` at the head of the queue, ready to
    /// resume on the next run. The reserved-word check precedes the `next`
    /// lookup, so a workflow author who declares `next.paused = "..."` is
    /// overridden.
    Paused {
        task: String,
        result: String,
    },
}

pub struct Runtime {
    workflow: Workflow,
    state: WorkflowState,
    conversation_root: crate::project::ConversationRoot,
    #[allow(dead_code)]
    knowledge: Arc<dyn crate::knowledge::base::KnowledgeBase>,
    engine: Arc<dyn Engine>,
    tool_registry: Arc<dyn ToolRegistry>,
    settings: Settings,
    cancel: CancellationToken,
}

impl std::fmt::Debug for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Runtime")
            .field("workflow", &self.workflow.name)
            .field(
                "conversation_root",
                &self.conversation_root.as_path().as_str(),
            )
            .finish_non_exhaustive()
    }
}

impl Runtime {
    /// Build a Runtime ready to drive `workflow` against `engine`.
    ///
    /// `state` is either freshly initialized with `[workflow.start]` or
    /// read from `workflow.state.toml` at `conversation`. The runtime
    /// persists `state` after each task. `knowledge` carries the
    /// `KnowledgeBase` that produced `workflow` and is reserved for
    /// follow-on slices that resolve skills mid-workflow.
    pub fn new(
        workflow: Workflow,
        state: WorkflowState,
        conversation: crate::project::ConversationRoot,
        knowledge: Arc<dyn crate::knowledge::base::KnowledgeBase>,
        engine: Arc<dyn Engine>,
        tool_registry: Arc<dyn ToolRegistry>,
        settings: Settings,
    ) -> Result<Self, WorkflowError> {
        validate_inputs(&workflow, &state)?;
        Ok(Self {
            workflow,
            state,
            conversation_root: conversation,
            knowledge,
            engine,
            tool_registry,
            settings,
            cancel: CancellationToken::new(),
        })
    }

    pub fn conversation_root(&self) -> &crate::project::ConversationRoot {
        &self.conversation_root
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
                knowledge: _knowledge,
                engine,
                tool_registry,
                settings,
                cancel,
            } = self;
            let conversation_root = conversation_root.as_path().clone();

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

                resolve_context_seed(&mut state);
                let ctx = build_template_context(&state);

                let resumed = find_resumable_turn(&state, &conversation_root, &task_name).await;

                let turn_path: VfsPath;
                let mut last_stop_reason: Option<StopReason> = None;
                let mut resumed_stop_str: Option<String> = None;

                if let Some((path, recorded_stop)) = resumed {
                    turn_path = path;
                    resumed_stop_str = recorded_stop;

                    yield WorkflowEvent::TaskStarted {
                        name: task_name.clone(),
                        turn: turn_path.clone(),
                    };
                } else {
                    let main_prompt = match &task.task {
                        TaskAction::Prompt { text } => text,
                        TaskAction::ToolCall { .. } => {
                            requeue_and_persist(&mut state, &conversation_root, &task_name);
                            yield WorkflowEvent::WorkflowFinished {
                                reason: task_failed(
                                    &task_name,
                                    WorkflowError::ToolCallTaskNotImplemented {
                                        task: task_name.clone(),
                                    },
                                ),
                            };
                            return;
                        }
                    };
                    let rendered_prompt = match template::substitute(main_prompt, &ctx) {
                        Ok(text) => text,
                        Err(UnresolvedPlaceholder { placeholder }) => {
                            requeue_and_persist(&mut state, &conversation_root, &task_name);
                            yield WorkflowEvent::WorkflowFinished {
                                reason: task_failed(
                                    &task_name,
                                    WorkflowError::UnresolvedTemplate {
                                        task: task_name.clone(),
                                        placeholder,
                                    },
                                ),
                            };
                            return;
                        }
                    };

                    turn_path = match synthesize_turn_file(
                        &conversation_root,
                        &task.name,
                        &task.name,
                        &task.skills,
                        &rendered_prompt,
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

                    let generator = Generator::new(convo, engine.clone(), settings.clone())
                        .with_registry(tool_registry.clone());
                    let mut events = generator.run();
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
                }

                let eval_result: String;
                if let Some(TaskAction::Prompt { text: eval_text }) = &task.evaluation {
                    let eval_label = format!("{}_eval", task.name);
                    let eval_path = match synthesize_turn_file(
                        &conversation_root,
                        &eval_label,
                        &eval_label,
                        &[],
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
                            EngineEvent::Reasoning(_) | EngineEvent::ReasoningDelta { .. } => {
                                // Workflow evaluator turns ignore reasoning events;
                                // the generator path is the recorder for thinking.
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
                } else if let Some(TaskAction::ToolCall {
                    tool: tool_name,
                    args: tool_args,
                }) = &task.evaluation
                {
                    let mut substituted = tool_args.clone();
                    if let Err(UnresolvedPlaceholder { placeholder }) =
                        template::substitute_value(&mut substituted, &ctx)
                    {
                        requeue_and_persist(&mut state, &conversation_root, &task_name);
                        yield WorkflowEvent::WorkflowFinished {
                            reason: task_failed(
                                &task_name,
                                WorkflowError::UnresolvedTemplate {
                                    task: task_name.clone(),
                                    placeholder,
                                },
                            ),
                        };
                        return;
                    }
                    let tool = match tool_registry.resolve(tool_name) {
                        Some(t) => t,
                        None => {
                            requeue_and_persist(&mut state, &conversation_root, &task_name);
                            yield WorkflowEvent::WorkflowFinished {
                                reason: task_failed(
                                    &task_name,
                                    WorkflowError::UnknownEvalTool {
                                        task: task_name.clone(),
                                        tool: tool_name.clone(),
                                    },
                                ),
                            };
                            return;
                        }
                    };
                    let json_value = match serde_json::to_value(&substituted) {
                        Ok(v) => v,
                        Err(source) => {
                            requeue_and_persist(&mut state, &conversation_root, &task_name);
                            yield WorkflowEvent::WorkflowFinished {
                                reason: task_failed(
                                    &task_name,
                                    WorkflowError::EvalArgsEncode {
                                        task: task_name.clone(),
                                        tool: tool_name.clone(),
                                        source,
                                    },
                                ),
                            };
                            return;
                        }
                    };
                    let args_str = match serde_json::to_string(&json_value) {
                        Ok(s) => s,
                        Err(source) => {
                            requeue_and_persist(&mut state, &conversation_root, &task_name);
                            yield WorkflowEvent::WorkflowFinished {
                                reason: task_failed(
                                    &task_name,
                                    WorkflowError::EvalArgsEncode {
                                        task: task_name.clone(),
                                        tool: tool_name.clone(),
                                        source,
                                    },
                                ),
                            };
                            return;
                        }
                    };
                    eval_result = match tool.call(args_str).await {
                        Ok(s) => s.trim().to_string(),
                        Err(err) => {
                            let arc_err = Arc::new(anyhow::Error::new(err));
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
                } else {
                    eval_result = if let Some(s) = resumed_stop_str.clone() {
                        s
                    } else {
                        last_stop_reason
                            .as_ref()
                            .map(StopReason::to_string)
                            .unwrap_or_else(|| "error".to_string())
                    };
                }

                let turn_filename = turn_path.filename();

                if eval_result == "paused" {
                    record_history_entry(
                        &mut state.history,
                        &task_name,
                        &turn_filename,
                        &eval_result,
                    );
                    state.last_result = Some(eval_result.clone());
                    requeue_and_persist(&mut state, &conversation_root, &task_name);
                    yield WorkflowEvent::WorkflowFinished {
                        reason: WorkflowStopReason::Paused {
                            task: task_name,
                            result: eval_result,
                        },
                    };
                    return;
                }

                record_history_entry(
                    &mut state.history,
                    &task_name,
                    &turn_filename,
                    &eval_result,
                );
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

/// Fill `state.context_seed` for the current run. Sets `today` to the local
/// date in `YYYY-MM-DD` when absent, and derives `session_dir` as
/// `docs/developer/{today}-A-{vars.topic}` when both `today` and
/// `state.inputs["topic"]` are present. Idempotent: a pre-seeded
/// `context_seed` is left untouched so a test can pin both fields without
/// reading the clock.
fn resolve_context_seed(state: &mut WorkflowState) {
    if state.context_seed.today.is_none() {
        let today = chrono::Local::now()
            .date_naive()
            .format("%Y-%m-%d")
            .to_string();
        state.context_seed.today = Some(today);
    }
    if state.context_seed.session_dir.is_none()
        && let Some(today) = state.context_seed.today.as_deref()
        && let Some(topic) = state.inputs.get("topic")
    {
        state.context_seed.session_dir = Some(format!("docs/developer/{today}-A-{topic}"));
    }
}

/// Build a substitution context populated from `state.inputs` (under
/// `vars.<name>`) and the resolved `context_seed`.
fn build_template_context(state: &WorkflowState) -> Context {
    let mut ctx = Context::new();
    for (name, value) in &state.inputs {
        ctx.insert(format!("vars.{name}"), value.clone());
    }
    if let Some(today) = &state.context_seed.today {
        ctx.insert("today", today.clone());
    }
    if let Some(session_dir) = &state.context_seed.session_dir {
        ctx.insert("session_dir", session_dir.clone());
    }
    ctx
}

/// Validate `state.inputs` against `workflow.inputs`. Required inputs must be
/// present; supplied inputs whose spec carries a `pattern` must match. A
/// malformed regex in `pattern` surfaces as `InputPatternMismatch` with a
/// diagnostic value so the workflow author sees which input's pattern is
/// broken.
fn validate_inputs(workflow: &Workflow, state: &WorkflowState) -> Result<(), WorkflowError> {
    for (name, spec) in &workflow.inputs {
        match state.inputs.get(name) {
            None => {
                if spec.required {
                    return Err(WorkflowError::MissingInput { name: name.clone() });
                }
            }
            Some(value) => {
                if let Some(pattern) = &spec.pattern {
                    let re = regex::Regex::new(pattern).map_err(|_| {
                        WorkflowError::InputPatternMismatch {
                            name: name.clone(),
                            value: "<pattern compile error>".to_string(),
                            pattern: pattern.clone(),
                        }
                    })?;
                    if !re.is_match(value) {
                        return Err(WorkflowError::InputPatternMismatch {
                            name: name.clone(),
                            value: value.clone(),
                            pattern: pattern.clone(),
                        });
                    }
                }
            }
        }
    }
    Ok(())
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

/// Push a history entry, or replace the most recent matching one when its
/// `(task, turn)` already names this turn file. Keeps `state.history`
/// 1:1 with turn files in a single complete run, while still recording
/// the eval-result transitions across pauses.
fn record_history_entry(
    history: &mut Vec<HistoryEntry>,
    task_name: &str,
    turn_filename: &str,
    eval_result: &str,
) {
    if let Some(existing) = history
        .iter_mut()
        .rev()
        .find(|h| h.task == task_name && h.turn == turn_filename)
    {
        existing.result = eval_result.to_string();
        return;
    }
    history.push(HistoryEntry {
        task: task_name.to_string(),
        turn: turn_filename.to_string(),
        result: eval_result.to_string(),
    });
}

/// Walk `state.history` in reverse for the most recent entry naming
/// `task_name`. When the entry's referenced turn file already carries a
/// recorded `[[response]]`, return its path along with the recorded
/// `stop_reason` (or `None` when the response did not carry one). The
/// runtime uses this to skip prompt synthesis and re-running the engine
/// across a workflow resume.
async fn find_resumable_turn(
    state: &WorkflowState,
    root: &VfsPath,
    task_name: &str,
) -> Option<(VfsPath, Option<String>)> {
    let entry = state.history.iter().rev().find(|h| h.task == task_name)?;
    let path = root.join(&entry.turn).ok()?;
    if !path.exists().ok().unwrap_or(false) {
        return None;
    }
    let convo = Conversation::single_turn(path.clone()).await.ok()?;
    let recorded = convo.turn(0).recorded_response()?;
    Some((path, recorded.stop_reason.clone()))
}

async fn synthesize_turn_file(
    root: &VfsPath,
    suffix: &str,
    step: &str,
    skills: &[String],
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
    let turn = ConversationTurn::new(
        path.clone(),
        meta,
        Vec::new(),
        skills.to_vec(),
        prompt.to_string(),
    );
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
    use crate::engine::{EmptyRegistry, Noop};
    use crate::knowledge::base::KnowledgeBase;
    use crate::knowledge::base::EmptyKnowledgeBase;
    use crate::project::ConversationRoot;

    fn empty_knowledge() -> Arc<dyn KnowledgeBase> {
        Arc::new(EmptyKnowledgeBase)
    }

    fn empty_registry() -> Arc<dyn ToolRegistry> {
        Arc::new(EmptyRegistry)
    }
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
            skills: Vec::new(),
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
            inputs: BTreeMap::new(),
            tasks: vec![task("only", "Do it.", &[])],
        };
        let state = WorkflowState::initial(&workflow);

        let runtime = Runtime::new(
            workflow,
            state,
            ConversationRoot::try_from(root.clone()).unwrap(),
            empty_knowledge(),
            Arc::new(Noop::default()),
            empty_registry(),
            Settings::default(),
        )
        .unwrap();
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
            inputs: BTreeMap::new(),
            tasks: vec![
                task("first", "First task.", &[("end_turn", "second")]),
                task("second", "Second task.", &[]),
            ],
        };
        let state = WorkflowState::initial(&workflow);

        let runtime = Runtime::new(
            workflow,
            state,
            ConversationRoot::try_from(root.clone()).unwrap(),
            empty_knowledge(),
            Arc::new(Noop::default()),
            empty_registry(),
            Settings::default(),
        )
        .unwrap();
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
            inputs: BTreeMap::new(),
            tasks: vec![task("only", "Do it.", &[])],
        };
        let state = WorkflowState::initial(&workflow);

        let runtime = Runtime::new(
            workflow,
            state,
            ConversationRoot::try_from(root.clone()).unwrap(),
            empty_knowledge(),
            Arc::new(Noop::default()),
            empty_registry(),
            Settings::default(),
        )
        .unwrap();
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
            inputs: BTreeMap::new(),
            tasks: vec![first, second],
        };
        let state = WorkflowState::initial(&workflow);

        let runtime = Runtime::new(
            workflow,
            state,
            ConversationRoot::try_from(root.clone()).unwrap(),
            empty_knowledge(),
            Arc::new(noop_with("approved")),
            empty_registry(),
            Settings::default(),
        )
        .unwrap();
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
            inputs: BTreeMap::new(),
            tasks: vec![first, second],
        };
        let state = WorkflowState::initial(&workflow);

        let runtime = Runtime::new(
            workflow,
            state,
            ConversationRoot::try_from(root.clone()).unwrap(),
            empty_knowledge(),
            Arc::new(noop_with("rejected")),
            empty_registry(),
            Settings::default(),
        )
        .unwrap();
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

    #[test]
    fn malformed_input_pattern_surfaces_as_input_pattern_mismatch() {
        use crate::workflow::schema::{InputSpec, Workflow};
        use crate::workflow::state::WorkflowState;

        let fs = mem_fs! { "root": {} };
        let root = fs.join("root").unwrap();

        let mut inputs = BTreeMap::new();
        inputs.insert(
            "topic".to_string(),
            InputSpec {
                description: "broken regex".to_string(),
                required: true,
                // Unclosed character class — fails to compile as a Regex.
                pattern: Some("[".to_string()),
            },
        );
        let workflow = Workflow {
            name: "broken-pattern".to_string(),
            start: "first".to_string(),
            inputs,
            tasks: vec![task("first", "noop", &[])],
        };
        let mut state = WorkflowState::initial(&workflow);
        state
            .inputs
            .insert("topic".to_string(), "anything".to_string());

        let err = Runtime::new(
            workflow,
            state,
            ConversationRoot::try_from(root).unwrap(),
            empty_knowledge(),
            Arc::new(Noop::default()),
            empty_registry(),
            Settings::default(),
        )
        .expect_err("malformed pattern must reject construction");
        match err {
            WorkflowError::InputPatternMismatch {
                name,
                value,
                pattern,
            } => {
                assert_eq!(name, "topic");
                assert_eq!(pattern, "[");
                assert!(
                    value.contains("pattern compile error"),
                    "value should signal compile failure: {value}"
                );
            }
            other => panic!("expected InputPatternMismatch, got {other:?}"),
        }
    }

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
            workflow.clone(),
            state,
            ConversationRoot::try_from(root.clone()).unwrap(),
            empty_knowledge(),
            Arc::new(Noop::default()),
            empty_registry(),
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
            ConversationRoot::try_from(root2).unwrap(),
            empty_knowledge(),
            Arc::new(Noop::default()),
            empty_registry(),
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
        bad_inputs
            .inputs
            .insert("topic".to_string(), "Bad Slug".to_string());
        let err = Runtime::new(
            workflow.clone(),
            bad_inputs,
            ConversationRoot::try_from(root.clone()).unwrap(),
            empty_knowledge(),
            Arc::new(Noop::default()),
            empty_registry(),
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
            ConversationRoot::try_from(root).unwrap(),
            empty_knowledge(),
            Arc::new(Noop::default()),
            empty_registry(),
            Settings::default(),
        )
        .expect_err("missing required input must reject construction");
        let msg2 = format!("{err2:#}");
        assert!(msg2.contains("topic"), "{msg2}");
    }

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
                    next: BTreeMap::from([("end_turn".to_string(), "bare".to_string())]),
                },
                Task {
                    name: "bare".to_string(),
                    skills: vec![],
                    task: TaskAction::Prompt {
                        text: "Run.".to_string(),
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
            ConversationRoot::try_from(root.clone()).unwrap(),
            empty_knowledge(),
            Arc::new(Noop::default()),
            empty_registry(),
            Settings::default(),
        )
        .unwrap();
        let _: Vec<WorkflowEvent> = runtime.run().collect().await;

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

        let convo = Conversation::single_turn(design_path).await.unwrap();
        let resolved: Vec<String> = convo.turn(0).declared_skills().to_vec();
        assert_eq!(
            resolved,
            vec![
                "developer:design".to_string(),
                "developer:thinking".to_string(),
            ]
        );

        let bare_name = root
            .read_dir()
            .unwrap()
            .map(|e| e.filename())
            .find(|n| n.ends_with("_bare.toml"))
            .expect("bare turn file written");
        let bare_text = root.join(&bare_name).unwrap().read_to_string().unwrap();
        assert!(
            !bare_text.contains("skills ="),
            "empty skills slice must omit the field: {bare_text}"
        );
    }

    fn fs_absent_args(path: &str, needle: &str) -> toml::Value {
        let mut t = toml::value::Table::new();
        t.insert("path".to_string(), toml::Value::String(path.to_string()));
        t.insert(
            "needle".to_string(),
            toml::Value::String(needle.to_string()),
        );
        toml::Value::Table(t)
    }

    fn registry_with_fs_absent(root: &VfsPath) -> Arc<dyn ToolRegistry> {
        use crate::engine::HashMapRegistry;
        use crate::knowledge::tools::FsAbsent;
        let mut registry = HashMapRegistry::default();
        registry.insert(
            FsAbsent::NAME,
            Arc::new(FsAbsent::new(
                &ConversationRoot::try_from(root.clone()).unwrap(),
            )),
        );
        Arc::new(registry)
    }

    #[tokio::test]
    async fn tool_call_eval_paused_yields_paused_stop_reason_and_requeues_task() {
        let fs = mem_fs! {
            "root": {
                "design.md": "# Design\n\n*Draft 2026-05-04*\n\nbody\n",
            }
        };
        let root = fs.join("root").unwrap();

        let mut first = task("first", "First task.", &[("cleared", "second")]);
        first.evaluation = Some(TaskAction::ToolCall {
            tool: "fs.absent".to_string(),
            args: fs_absent_args("design.md", "*Draft"),
        });
        let workflow = Workflow {
            name: "paused".to_string(),
            start: "first".to_string(),
            inputs: BTreeMap::new(),
            tasks: vec![first, task("second", "Second.", &[])],
        };
        let state = WorkflowState::initial(&workflow);

        let runtime = Runtime::new(
            workflow,
            state,
            ConversationRoot::try_from(root.clone()).unwrap(),
            empty_knowledge(),
            Arc::new(Noop::default()),
            registry_with_fs_absent(&root),
            Settings::default(),
        )
        .unwrap();
        let events: Vec<WorkflowEvent> = runtime.run().collect().await;

        match events.last().unwrap() {
            WorkflowEvent::WorkflowFinished {
                reason: WorkflowStopReason::Paused { task, result },
            } => {
                assert_eq!(task, "first");
                assert_eq!(result, "paused");
            }
            other => panic!("expected Paused, got {other:?}"),
        }

        let reloaded = WorkflowState::read(&root).unwrap().expect("state written");
        assert_eq!(reloaded.queue.front().map(String::as_str), Some("first"));
        assert!(
            reloaded
                .history
                .iter()
                .any(|h| h.task == "first" && h.result == "paused"),
            "history must record the pause: {:?}",
            reloaded.history
        );
    }

    #[tokio::test]
    async fn tool_call_eval_cleared_routes_via_next() {
        let fs = mem_fs! {
            "root": {
                "design.md": "# Design\n\nbody\n",
            }
        };
        let root = fs.join("root").unwrap();

        let mut first = task("first", "First.", &[("cleared", "second")]);
        first.evaluation = Some(TaskAction::ToolCall {
            tool: "fs.absent".to_string(),
            args: fs_absent_args("design.md", "*Draft"),
        });
        let workflow = Workflow {
            name: "cleared".to_string(),
            start: "first".to_string(),
            inputs: BTreeMap::new(),
            tasks: vec![first, task("second", "Second.", &[])],
        };
        let state = WorkflowState::initial(&workflow);

        let runtime = Runtime::new(
            workflow,
            state,
            ConversationRoot::try_from(root.clone()).unwrap(),
            empty_knowledge(),
            Arc::new(Noop::default()),
            registry_with_fs_absent(&root),
            Settings::default(),
        )
        .unwrap();
        let events: Vec<WorkflowEvent> = runtime.run().collect().await;

        assert!(matches!(
            events.last().unwrap(),
            WorkflowEvent::WorkflowFinished {
                reason: WorkflowStopReason::Completed,
            }
        ));
        let started: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                WorkflowEvent::TaskStarted { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(started, vec!["first".to_string(), "second".to_string()]);
    }

    #[tokio::test]
    async fn tool_call_eval_unknown_tool_yields_task_failed() {
        let fs = mem_fs! { "root": {} };
        let root = fs.join("root").unwrap();

        let mut first = task("first", "First.", &[]);
        first.evaluation = Some(TaskAction::ToolCall {
            tool: "tool.does_not_exist".to_string(),
            args: fs_absent_args("design.md", "*Draft"),
        });
        let workflow = Workflow {
            name: "unknown_tool".to_string(),
            start: "first".to_string(),
            inputs: BTreeMap::new(),
            tasks: vec![first],
        };
        let state = WorkflowState::initial(&workflow);

        let runtime = Runtime::new(
            workflow,
            state,
            ConversationRoot::try_from(root.clone()).unwrap(),
            empty_knowledge(),
            Arc::new(Noop::default()),
            empty_registry(),
            Settings::default(),
        )
        .unwrap();
        let events: Vec<WorkflowEvent> = runtime.run().collect().await;

        match events.last().unwrap() {
            WorkflowEvent::WorkflowFinished {
                reason: WorkflowStopReason::TaskFailed { task, error },
            } => {
                assert_eq!(task, "first");
                let msg = format!("{error:#}");
                assert!(
                    msg.contains("tool.does_not_exist"),
                    "error must name the missing tool: {msg}"
                );
            }
            other => panic!("expected TaskFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn next_paused_is_overridden_by_reserved_word() {
        let fs = mem_fs! {
            "root": {
                "design.md": "# Design\n\n*Draft 2026-05-04*\n\nbody\n",
            }
        };
        let root = fs.join("root").unwrap();

        let mut first = task("first", "First.", &[]);
        first.evaluation = Some(TaskAction::ToolCall {
            tool: "fs.absent".to_string(),
            args: fs_absent_args("design.md", "*Draft"),
        });
        first.next.insert("paused".to_string(), "skip".to_string());
        let workflow = Workflow {
            name: "reserved".to_string(),
            start: "first".to_string(),
            inputs: BTreeMap::new(),
            tasks: vec![first, task("skip", "Should never run.", &[])],
        };
        let state = WorkflowState::initial(&workflow);

        let runtime = Runtime::new(
            workflow,
            state,
            ConversationRoot::try_from(root.clone()).unwrap(),
            empty_knowledge(),
            Arc::new(Noop::default()),
            registry_with_fs_absent(&root),
            Settings::default(),
        )
        .unwrap();
        let events: Vec<WorkflowEvent> = runtime.run().collect().await;

        match events.last().unwrap() {
            WorkflowEvent::WorkflowFinished {
                reason: WorkflowStopReason::Paused { task, .. },
            } => assert_eq!(task, "first"),
            other => panic!("expected Paused, got {other:?}"),
        }

        let reloaded = WorkflowState::read(&root).unwrap().expect("state written");
        let queue: Vec<&str> = reloaded.queue.iter().map(String::as_str).collect();
        assert_eq!(
            queue,
            vec!["first"],
            "reserved-word halt must not push next.paused onto the queue: {:?}",
            reloaded.queue
        );
    }

    #[tokio::test]
    async fn tool_call_eval_args_substitute_via_template_context() {
        // Args carry {{ vars.path }}; the runtime renders it before
        // dispatching the tool. The fixture's file is only readable when
        // the placeholder resolves.
        let fs = mem_fs! {
            "root": {
                "design.md": "# Design\n\nbody\n",
            }
        };
        let root = fs.join("root").unwrap();

        use crate::workflow::schema::InputSpec;
        let mut inputs_spec = BTreeMap::new();
        inputs_spec.insert(
            "path".to_string(),
            InputSpec {
                description: "artifact path".to_string(),
                required: true,
                pattern: None,
            },
        );

        let mut args_table = toml::value::Table::new();
        args_table.insert(
            "path".to_string(),
            toml::Value::String("{{ vars.path }}".to_string()),
        );
        args_table.insert(
            "needle".to_string(),
            toml::Value::String("*Draft".to_string()),
        );

        let mut first = task("first", "First.", &[]);
        first.evaluation = Some(TaskAction::ToolCall {
            tool: "fs.absent".to_string(),
            args: toml::Value::Table(args_table),
        });
        let workflow = Workflow {
            name: "substituted".to_string(),
            start: "first".to_string(),
            inputs: inputs_spec,
            tasks: vec![first],
        };
        let mut state = WorkflowState::initial(&workflow);
        state
            .inputs
            .insert("path".to_string(), "design.md".to_string());

        let runtime = Runtime::new(
            workflow,
            state,
            ConversationRoot::try_from(root.clone()).unwrap(),
            empty_knowledge(),
            Arc::new(Noop::default()),
            registry_with_fs_absent(&root),
            Settings::default(),
        )
        .unwrap();
        let events: Vec<WorkflowEvent> = runtime.run().collect().await;

        assert!(matches!(
            events.last().unwrap(),
            WorkflowEvent::WorkflowFinished {
                reason: WorkflowStopReason::Completed,
            }
        ));
    }

    fn write_file(root: &VfsPath, filename: &str, body: &str) -> VfsPath {
        let path = root.join(filename).unwrap();
        let mut writer = path.create_file().unwrap();
        std::io::Write::write_all(&mut writer, body.as_bytes()).unwrap();
        path
    }

    fn seed_turn_with_response(root: &VfsPath, filename: &str, body: &str) -> VfsPath {
        write_file(root, filename, body)
    }

    #[tokio::test]
    async fn resume_skips_prompt_when_history_names_a_recorded_turn_file() {
        let fs = mem_fs! { "root": {} };
        let root = fs.join("root").unwrap();

        let pre_seeded = "prompt = \"original prompt\"\n\n[[response]]\nrole = \"assistant\"\ntext = \"recorded response\"\nstop_reason = \"end_turn\"\n";
        let turn_path = seed_turn_with_response(&root, "01_only.toml", pre_seeded);

        let workflow = Workflow {
            name: "resumer".to_string(),
            start: "only".to_string(),
            inputs: BTreeMap::new(),
            tasks: vec![task("only", "ignored on resume", &[])],
        };
        let mut state = WorkflowState::initial(&workflow);
        state.history.push(HistoryEntry {
            task: "only".to_string(),
            turn: "01_only.toml".to_string(),
            result: "paused".to_string(),
        });

        let runtime = Runtime::new(
            workflow,
            state,
            ConversationRoot::try_from(root.clone()).unwrap(),
            empty_knowledge(),
            Arc::new(noop_with("DIFFERENT_RESPONSE")),
            empty_registry(),
            Settings::default(),
        )
        .unwrap();
        let _: Vec<WorkflowEvent> = runtime.run().collect().await;

        let after = turn_path.read_to_string().unwrap();
        assert_eq!(
            after, pre_seeded,
            "resume must not rewrite the recorded turn file"
        );
    }

    #[tokio::test]
    async fn resume_proceeds_to_evaluation_using_recorded_stop_reason() {
        let fs = mem_fs! { "root": {} };
        let root = fs.join("root").unwrap();

        let pre_seeded = "prompt = \"resumed\"\n\n[[response]]\nrole = \"assistant\"\ntext = \"hi\"\nstop_reason = \"end_turn\"\n";
        seed_turn_with_response(&root, "01_first.toml", pre_seeded);

        let workflow = Workflow {
            name: "evald-resume".to_string(),
            start: "first".to_string(),
            inputs: BTreeMap::new(),
            tasks: vec![
                task("first", "ignored", &[("end_turn", "second")]),
                task("second", "Second.", &[]),
            ],
        };
        let mut state = WorkflowState::initial(&workflow);
        state.history.push(HistoryEntry {
            task: "first".to_string(),
            turn: "01_first.toml".to_string(),
            result: "paused".to_string(),
        });

        let runtime = Runtime::new(
            workflow,
            state,
            ConversationRoot::try_from(root.clone()).unwrap(),
            empty_knowledge(),
            Arc::new(Noop::default()),
            empty_registry(),
            Settings::default(),
        )
        .unwrap();
        let events: Vec<WorkflowEvent> = runtime.run().collect().await;

        let started: Vec<String> = events
            .iter()
            .filter_map(|e| match e {
                WorkflowEvent::TaskStarted { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(started, vec!["first".to_string(), "second".to_string()]);
        assert!(matches!(
            events.last().unwrap(),
            WorkflowEvent::WorkflowFinished {
                reason: WorkflowStopReason::Completed,
            }
        ));
    }

    #[tokio::test]
    async fn resume_falls_through_to_fresh_prompt_when_recorded_turn_lacks_response() {
        let fs = mem_fs! { "root": {} };
        let root = fs.join("root").unwrap();

        let pre_seeded = "prompt = \"stale\"\n";
        seed_turn_with_response(&root, "01_first.toml", pre_seeded);

        let workflow = Workflow {
            name: "fresh-fallthrough".to_string(),
            start: "first".to_string(),
            inputs: BTreeMap::new(),
            tasks: vec![task("first", "Fresh prompt.", &[])],
        };
        let mut state = WorkflowState::initial(&workflow);
        state.history.push(HistoryEntry {
            task: "first".to_string(),
            turn: "01_first.toml".to_string(),
            result: "paused".to_string(),
        });

        let runtime = Runtime::new(
            workflow,
            state,
            ConversationRoot::try_from(root.clone()).unwrap(),
            empty_knowledge(),
            Arc::new(Noop::default()),
            empty_registry(),
            Settings::default(),
        )
        .unwrap();
        let _: Vec<WorkflowEvent> = runtime.run().collect().await;

        let entries: Vec<String> = root.read_dir().unwrap().map(|e| e.filename()).collect();
        assert!(
            entries.iter().any(|n| n.ends_with("_first.toml")),
            "fresh first turn file expected: {entries:?}"
        );
        let synthesized_name = entries
            .iter()
            .find(|n| n.ends_with("_first.toml") && *n != "01_first.toml")
            .expect("a freshly synthesized turn file distinct from the pre-seeded stale one");
        let synthesized = root
            .join(synthesized_name)
            .unwrap()
            .read_to_string()
            .unwrap();
        assert!(
            synthesized.contains("Fresh prompt."),
            "fresh turn must carry the rendered prompt: {synthesized}"
        );
        assert!(
            synthesized.contains("[[response]]"),
            "fresh turn must carry a recorded response: {synthesized}"
        );
    }

    #[tokio::test]
    async fn workflow_pauses_and_resumes_across_design_feature_test_and_plan() {
        use crate::engine::{HashMapRegistry, ToolRegistry};
        use crate::knowledge::tools::FsAbsent;

        let fs = mem_fs! {
            "root": {
                "design.md": "# Design\n\n*Draft 2026-05-04*\n\nbody\n",
                "feature-test.md": "# FT\n\n*Draft 2026-05-04*\n\nbody\n",
                "plan.md": "# Plan\n\n*Draft 2026-05-04*\n\nbody\n",
            }
        };
        let root = fs.join("root").unwrap();

        fn fs_absent_args(path: &str) -> toml::Value {
            let mut t = toml::value::Table::new();
            t.insert("path".to_string(), toml::Value::String(path.to_string()));
            t.insert(
                "needle".to_string(),
                toml::Value::String("*Draft".to_string()),
            );
            toml::Value::Table(t)
        }

        fn gated(name: &str, skill: &str, artifact: &str, next: Option<&str>) -> Task {
            let mut next_map = BTreeMap::new();
            if let Some(n) = next {
                next_map.insert("cleared".to_string(), n.to_string());
            }
            Task {
                name: name.to_string(),
                skills: vec![skill.to_string()],
                task: TaskAction::Prompt {
                    text: format!("Produce {artifact}."),
                },
                evaluation: Some(TaskAction::ToolCall {
                    tool: "fs.absent".to_string(),
                    args: fs_absent_args(artifact),
                }),
                next: next_map,
            }
        }

        let workflow = Workflow {
            name: "ailly-dev-cycle".to_string(),
            start: "design".to_string(),
            inputs: BTreeMap::new(),
            tasks: vec![
                gated(
                    "design",
                    "developer:design",
                    "design.md",
                    Some("feature_test"),
                ),
                gated(
                    "feature_test",
                    "developer:feature-test",
                    "feature-test.md",
                    Some("plan"),
                ),
                gated("plan", "developer:plan", "plan.md", None),
            ],
        };

        let mut registry = HashMapRegistry::default();
        registry.insert(
            "fs.absent",
            Arc::new(FsAbsent::new(
                &ConversationRoot::try_from(root.clone()).unwrap(),
            )),
        );
        let registry: Arc<dyn ToolRegistry> = Arc::new(registry);

        async fn run_once(
            workflow: &Workflow,
            state: WorkflowState,
            root: &VfsPath,
            registry: Arc<dyn ToolRegistry>,
        ) -> Vec<WorkflowEvent> {
            let runtime = Runtime::new(
                workflow.clone(),
                state,
                ConversationRoot::try_from(root.clone()).unwrap(),
                empty_knowledge(),
                Arc::new(Noop::default()),
                registry,
                Settings::default(),
            )
            .expect("construction OK");
            runtime.run().collect().await
        }

        fn turn_file(root: &VfsPath, suffix: &str) -> VfsPath {
            let name = root
                .read_dir()
                .unwrap()
                .map(|e| e.filename())
                .find(|n| n.ends_with(suffix))
                .unwrap_or_else(|| panic!("turn file ending in {suffix} missing"));
            root.join(&name).unwrap()
        }

        fn assert_paused_at(events: &[WorkflowEvent], task_name: &str) {
            match events.last().unwrap() {
                WorkflowEvent::WorkflowFinished {
                    reason: WorkflowStopReason::Paused { task, result },
                } => {
                    assert_eq!(task, task_name, "expected pause at {task_name}");
                    assert_eq!(result, "paused");
                }
                other => panic!("expected Paused at {task_name}, got {other:?}"),
            }
        }

        fn assert_no_restart(events: &[WorkflowEvent], turn_path: &VfsPath) {
            let started = events.iter().any(|e| {
                matches!(
                    e,
                    WorkflowEvent::TaskTurn(TurnEvent::Started { path })
                        if path.as_str() == turn_path.as_str()
                )
            });
            assert!(
                !started,
                "resume must not re-emit Started for {}",
                turn_path.as_str()
            );
        }

        // Run 1: design pauses on its own draft marker.
        let events = run_once(
            &workflow,
            WorkflowState::initial(&workflow),
            &root,
            registry.clone(),
        )
        .await;
        assert_paused_at(&events, "design");

        let design_turn = turn_file(&root, "_design.toml");
        let design_before = design_turn.read_to_string().unwrap();
        assert!(
            design_before.contains("[[response]]"),
            "design turn must carry a recorded response: {design_before}"
        );

        let state = WorkflowState::read(&root)
            .unwrap()
            .expect("state persisted");
        assert_eq!(state.queue.front().map(String::as_str), Some("design"));
        assert!(
            state
                .history
                .iter()
                .any(|h| h.task == "design" && h.result == "paused"),
            "history must record the design pause: {:?}",
            state.history
        );

        write_file(&root, "design.md", "# Design\n\nbody\n");

        // Run 2: design resumes cleared, feature_test pauses.
        let events = run_once(&workflow, state, &root, registry.clone()).await;
        assert_no_restart(&events, &design_turn);
        assert_eq!(
            design_turn.read_to_string().unwrap(),
            design_before,
            "design turn file must be byte-identical across resume"
        );
        assert_paused_at(&events, "feature_test");

        let ft_turn = turn_file(&root, "_feature_test.toml");
        let ft_before = ft_turn.read_to_string().unwrap();

        let state = WorkflowState::read(&root)
            .unwrap()
            .expect("state persisted");
        assert_eq!(
            state.queue.front().map(String::as_str),
            Some("feature_test")
        );

        write_file(&root, "feature-test.md", "# FT\n\nbody\n");

        // Run 3: feature_test resumes cleared, plan pauses.
        let events = run_once(&workflow, state, &root, registry.clone()).await;
        assert_no_restart(&events, &ft_turn);
        assert_eq!(
            ft_turn.read_to_string().unwrap(),
            ft_before,
            "feature_test turn file must be byte-identical across resume"
        );
        assert_paused_at(&events, "plan");

        let plan_turn = turn_file(&root, "_plan.toml");
        let plan_before = plan_turn.read_to_string().unwrap();

        let state = WorkflowState::read(&root)
            .unwrap()
            .expect("state persisted");
        assert_eq!(state.queue.front().map(String::as_str), Some("plan"));

        write_file(&root, "plan.md", "# Plan\n\nbody\n");

        // Run 4: plan resumes cleared, workflow completes.
        let events = run_once(&workflow, state, &root, registry).await;
        assert_no_restart(&events, &plan_turn);
        assert_eq!(
            plan_turn.read_to_string().unwrap(),
            plan_before,
            "plan turn file must be byte-identical across resume"
        );
        assert!(
            matches!(
                events.last().unwrap(),
                WorkflowEvent::WorkflowFinished {
                    reason: WorkflowStopReason::Completed,
                }
            ),
            "expected Completed, got {:?}",
            events.last()
        );

        let final_state = WorkflowState::read(&root)
            .unwrap()
            .expect("state persisted");
        assert!(
            final_state.queue.is_empty(),
            "completed workflow has empty queue"
        );
        let stages_in_history: Vec<&str> = final_state
            .history
            .iter()
            .map(|h| h.task.as_str())
            .collect();
        for stage in ["design", "feature_test", "plan"] {
            assert!(
                stages_in_history.contains(&stage),
                "history must record {stage}; got {stages_in_history:?}"
            );
        }
    }

    #[tokio::test]
    async fn runtime_writes_workflow_state_under_conversation_root() {
        let fs = mem_fs! { "root": {} };
        let root = fs.join("root").unwrap();
        let workflow = Workflow {
            name: "lone".to_string(),
            start: "only".to_string(),
            inputs: BTreeMap::new(),
            tasks: vec![task("only", "Do it.", &[])],
        };
        let state = WorkflowState::initial(&workflow);

        let runtime = Runtime::new(
            workflow,
            state,
            ConversationRoot::try_from(root.clone()).unwrap(),
            empty_knowledge(),
            Arc::new(Noop::default()),
            empty_registry(),
            Settings::default(),
        )
        .unwrap();
        let _events: Vec<WorkflowEvent> = runtime.run().collect().await;

        let state_path = root.join("workflow.state.toml").unwrap();
        assert!(
            state_path.exists().unwrap(),
            "workflow.state.toml is written under the conversation root"
        );
    }

    #[tokio::test]
    async fn runtime_workflow_definition_loads_from_knowledge_base() {
        use crate::knowledge::base::{FsKnowledgeBase, KnowledgeBase, WorkflowName};
        use crate::project::KnowledgeRoot;

        let fs = mem_fs! {
            "project": {
                "workflows": {
                    "build.toml": "name = \"build\"\nstart = \"compile\"\n[[tasks]]\nname = \"compile\"\ntask = { kind = \"prompt\", text = \"build it\" }\n",
                },
            },
        };
        let project = KnowledgeRoot::try_from(fs.join("project").unwrap()).unwrap();
        let kb = FsKnowledgeBase::new(vec![project]);

        let workflow = kb.workflow(&WorkflowName::new("build")).unwrap();
        assert_eq!(workflow.name, "build");
        assert_eq!(workflow.start, "compile");

        let state = WorkflowState::initial(&workflow);
        let runtime = Runtime::new(
            workflow,
            state,
            ConversationRoot::try_from(fs.join("project").unwrap()).unwrap(),
            Arc::new(kb),
            Arc::new(Noop::default()),
            empty_registry(),
            Settings::default(),
        )
        .expect("runtime accepts knowledge-base-loaded workflow");
        let _events: Vec<WorkflowEvent> = runtime.run().collect().await;
    }
}
