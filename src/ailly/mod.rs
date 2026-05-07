mod args;
pub(crate) mod knowledge_base;
pub(crate) mod listing;
mod logging;

pub use args::{Cli, LogFormat, parse_workflow_arg};
pub use logging::init as init_logging;

use std::io::Write;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use futures::StreamExt;

use crate::content::Conversation;
#[cfg(feature = "bedrock")]
use crate::engine::bedrock_from_env;
use crate::engine::{
    Engine, Generator, Noop, Settings, StopReason, TurnEvent, anthropic_from_env, gemini_from_env,
    openai_from_env,
};
use crate::workflow::{Runtime, WorkflowEvent, WorkflowState, WorkflowStopReason};

use std::collections::{BTreeMap, VecDeque};

/// Merge `--input KEY=VALUE` entries into the workflow's persisted inputs.
///
/// Persisted state takes precedence: an entry is inserted only when its
/// key is not already present in `state_inputs`.
fn merge_input_flags(
    state_inputs: &mut BTreeMap<String, String>,
    flags: &[String],
) -> Result<(), RunError> {
    for entry in flags {
        let (key, value) = entry
            .split_once('=')
            .ok_or_else(|| RunError::Setup(anyhow!("--input {entry:?}: expected KEY=VALUE")))?;
        state_inputs
            .entry(key.to_string())
            .or_insert_with(|| value.to_string());
    }
    Ok(())
}

const DEFAULT_ANTHROPIC_MODEL: &str = "claude-sonnet-4-5";
const DEFAULT_OPENAI_MODEL: &str = "gpt-4o-mini";
const DEFAULT_GEMINI_MODEL: &str = "gemini-2.5-flash";
#[cfg(feature = "bedrock")]
const DEFAULT_BEDROCK_MODEL: &str = "us.anthropic.claude-sonnet-4-5-20250929-v1:0";

#[derive(Debug)]
enum RunError {
    /// The error has already been reported to stderr; the CLI just needs to
    /// exit non-zero.
    Reported,
    /// A setup or infrastructure error that the CLI should print itself.
    Setup(anyhow::Error),
}

impl<E: Into<anyhow::Error>> From<E> for RunError {
    fn from(err: E) -> Self {
        Self::Setup(err.into())
    }
}

pub fn run() -> ExitCode {
    let cli = Cli::parse();
    logging::init(cli.verbose, cli.log_level.as_deref(), cli.log_format);

    let _ = dotenvy::dotenv();

    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("building tokio runtime")
    {
        Ok(rt) => rt,
        Err(err) => {
            eprintln!("ailly: {err:#}");
            return ExitCode::FAILURE;
        }
    };

    match runtime.block_on(run_async(cli)) {
        Ok(()) => ExitCode::SUCCESS,
        Err(RunError::Reported) => ExitCode::FAILURE,
        Err(RunError::Setup(err)) => {
            eprintln!("ailly: {err:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run_async(cli: Cli) -> Result<(), RunError> {
    if cli.clean {
        return run_clean(&cli).await;
    }

    if cli.list_workflows || matches!(cli.workflow.as_deref(), Some("")) {
        return run_list_workflows(&cli, None).await;
    }
    if cli.list_skills {
        return run_list_skills(&cli).await;
    }
    if cli.list_tools {
        return run_list_tools(&cli).await;
    }

    let engine_kind = resolve_engine_kind(cli.engine.as_deref())?;
    let model = cli.model.clone();

    log::info!(
        "ailly starting; root={}, engine={:?}, model={:?}",
        cli.root().display(),
        engine_kind,
        model
    );

    let engine: Arc<dyn Engine> = build_engine(engine_kind, model.as_deref())?;

    if let Some(raw) = cli.workflow.as_deref() {
        let (name, _) = parse_workflow_arg(raw)?;
        let project = cli.project().map_err(RunError::Setup)?;
        let entries = project
            .list_workflows()
            .map_err(|e| RunError::Setup(anyhow!("failed to list workflows: {e}")))?;
        if !entries.iter().any(|e| e.name() == &name) {
            return run_list_workflows(&cli, Some(name.as_str())).await;
        }
        return run_workflow(&cli, engine, raw).await;
    }

    let conversation = load_conversation(&cli).await?;

    if conversation.turn_count() == 0 {
        return Err(RunError::Setup(anyhow!(
            "no conversational turns found at {}; pass --prompt or add a `<name>.toml` turn file",
            cli.root().display()
        )));
    }

    let generator = Generator::new(conversation, engine, Settings::default());
    let mut events = generator.run();

    let mut stdout = std::io::stdout().lock();
    let mut had_turn_error = false;
    while let Some(event) = events.next().await {
        match event {
            TurnEvent::Started { path } => {
                log::info!("turn started: {}", path.as_str());
            }
            TurnEvent::Delta { text, .. } => {
                stdout.write_all(text.as_bytes())?;
                stdout.flush()?;
            }
            TurnEvent::Finished {
                path, stop_reason, ..
            } => {
                stdout.write_all(b"\n")?;
                stdout.flush()?;
                if let StopReason::Error(msg) = &stop_reason {
                    eprintln!(
                        "ailly: turn {} failed: {}",
                        path.as_str(),
                        format_engine_error(msg)
                    );
                    had_turn_error = true;
                }
                log::info!("turn finished: {} ({:?})", path.as_str(), stop_reason);
            }
            TurnEvent::ToolCall { path, call } => {
                log::info!(
                    "turn tool_call: {} {} id={}",
                    path.as_str(),
                    call.function.name,
                    call.id
                );
            }
            TurnEvent::ToolResult { path, result } => {
                log::info!("turn tool_result: {} id={}", path.as_str(), result.id);
            }
            TurnEvent::Skipped { path, reason } => {
                log::info!("turn skipped: {} ({:?})", path.as_str(), reason);
            }
            TurnEvent::Failed { path, error } => {
                eprintln!(
                    "ailly: turn {} failed: {}",
                    path.as_str(),
                    format_engine_error(&format!("{error:#}"))
                );
                return Err(RunError::Reported);
            }
        }
    }

    if had_turn_error {
        Err(RunError::Reported)
    } else {
        Ok(())
    }
}

/// Extract a human-readable error from a provider error string.
///
/// Provider errors arrive wrapped in transport prefixes like
/// `CompletionError: ProviderError: SSE Error: ... with message: {json}`.
/// When a JSON body is embedded, pull `error.type` and `error.message` out
/// so the CLI shows the API's own description instead of the wire-level
/// chain. Falls back to the raw string when no JSON is present.
fn format_engine_error(raw: &str) -> String {
    let Some(start) = raw.find('{') else {
        return raw.to_string();
    };
    let candidate = &raw[start..];
    let mut iter = serde_json::Deserializer::from_str(candidate).into_iter::<serde_json::Value>();
    let Some(Ok(value)) = iter.next() else {
        return raw.to_string();
    };
    let Some(error) = value.get("error") else {
        return raw.to_string();
    };
    let kind = error.get("type").and_then(serde_json::Value::as_str);
    let message = error.get("message").and_then(serde_json::Value::as_str);
    match (kind, message) {
        (Some(k), Some(m)) => format!("{k}: {m}"),
        (None, Some(m)) => m.to_string(),
        _ => raw.to_string(),
    }
}

#[derive(Debug, Clone, Copy)]
enum EngineKind {
    Noop,
    Anthropic,
    Openai,
    Gemini,
    #[cfg(feature = "bedrock")]
    Bedrock,
}

fn resolve_engine_kind(raw: Option<&str>) -> Result<EngineKind> {
    match raw.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        None | Some("") | Some("noop") => Ok(EngineKind::Noop),
        Some("anthropic") | Some("claude") => Ok(EngineKind::Anthropic),
        Some("openai") | Some("gpt") => Ok(EngineKind::Openai),
        Some("gemini") | Some("google") => Ok(EngineKind::Gemini),
        #[cfg(feature = "bedrock")]
        Some("bedrock") => Ok(EngineKind::Bedrock),
        Some(other) => Err(anyhow!(
            "unknown engine {other:?}; expected one of: noop, anthropic, openai, gemini{}",
            if cfg!(feature = "bedrock") {
                ", bedrock"
            } else {
                ""
            }
        )),
    }
}

fn build_engine(kind: EngineKind, model: Option<&str>) -> Result<Arc<dyn Engine>> {
    match kind {
        EngineKind::Noop => Ok(Arc::new(Noop::default())),
        EngineKind::Anthropic => {
            let model = model.unwrap_or(DEFAULT_ANTHROPIC_MODEL);
            Ok(Arc::new(anthropic_from_env(model)?))
        }
        EngineKind::Openai => {
            let model = model.unwrap_or(DEFAULT_OPENAI_MODEL);
            Ok(Arc::new(openai_from_env(model)?))
        }
        EngineKind::Gemini => {
            let model = model.unwrap_or(DEFAULT_GEMINI_MODEL);
            Ok(Arc::new(gemini_from_env(model)?))
        }
        #[cfg(feature = "bedrock")]
        EngineKind::Bedrock => {
            let model = model.unwrap_or(DEFAULT_BEDROCK_MODEL);
            Ok(Arc::new(bedrock_from_env(model)?))
        }
    }
}

async fn run_list_workflows(cli: &Cli, unknown: Option<&str>) -> Result<(), RunError> {
    let project = cli.project().map_err(RunError::Setup)?;
    let entries = project
        .list_workflows()
        .map_err(|e| RunError::Setup(anyhow!("failed to list workflows: {e}")))?;
    if let Some(name) = unknown {
        let preface =
            listing::render_unknown_workflow_preface(name, &project.workflow_search_paths());
        eprint!("{preface}");
    }
    print!("{}", listing::render_workflow_listing(&entries));
    if unknown.is_some() {
        Err(RunError::Reported)
    } else {
        Ok(())
    }
}

async fn run_list_skills(cli: &Cli) -> Result<(), RunError> {
    let project = cli.project().map_err(RunError::Setup)?;
    let entries = project
        .list_skills()
        .map_err(|e| RunError::Setup(anyhow!("failed to list skills: {e}")))?;
    print!("{}", listing::render_skill_listing(&entries));
    Ok(())
}

async fn run_list_tools(cli: &Cli) -> Result<(), RunError> {
    let project = cli.project().map_err(RunError::Setup)?;
    let entries = project.list_tools().await;
    print!("{}", listing::render_tool_listing(&entries));
    Ok(())
}

async fn load_conversation(cli: &Cli) -> Result<Conversation> {
    let root = cli.root();
    let mut conversation = if root.exists() {
        load_from_root(cli).await?
    } else if cli.prompt.is_some() {
        Conversation::empty()
    } else {
        return Err(anyhow!("root path does not exist: {}", root.display()));
    };

    if let Some(prompt) = cli.prompt.as_deref() {
        conversation.push_synthetic_prompt(prompt)?;
    }

    Ok(conversation)
}

async fn run_clean(cli: &Cli) -> Result<(), RunError> {
    let root = cli.root();
    if !root.exists() {
        return Err(RunError::Setup(anyhow!(
            "root path does not exist: {}",
            root.display()
        )));
    }

    let mut conversation = load_from_root(cli).await?;
    conversation.clean().await?;
    for idx in 0..conversation.turn_count() {
        log::info!("cleaned: {}", conversation.turn(idx).path().as_str());
    }
    Ok(())
}

async fn load_from_root(cli: &Cli) -> Result<Conversation> {
    let project = cli.project()?;
    let knowledge = crate::knowledge::base::FsKnowledgeBase::build(project.knowledge.clone())?;
    Conversation::load(&project.conversations, &knowledge)
        .await
        .with_context(|| format!("loading conversation at {}", cli.root().display()))
}

async fn run_workflow(cli: &Cli, engine: Arc<dyn Engine>, raw: &str) -> Result<(), RunError> {
    let (workflow_name, task_override) = parse_workflow_arg(raw)?;

    let root = cli.root();
    if !root.exists() {
        return Err(RunError::Setup(anyhow!(
            "root path does not exist: {}",
            root.display()
        )));
    }
    let project = cli
        .project()
        .with_context(|| format!("assembling project from {}", root.display()))?;
    let conversation = crate::project::ConversationRoot::workflow_subdir(&project.root)?;
    let knowledge: std::sync::Arc<dyn crate::knowledge::base::KnowledgeBase> = std::sync::Arc::new(
        crate::knowledge::base::FsKnowledgeBase::build(project.knowledge.clone())?,
    );

    let workflow = knowledge.workflow(&workflow_name)?;

    let mut state = WorkflowState::read(conversation.as_path())?
        .unwrap_or_else(|| WorkflowState::initial(&workflow));

    if let Some(task_name) = task_override {
        state.queue = VecDeque::from(vec![task_name]);
    } else if state.queue.is_empty() {
        state.queue.push_back(workflow.start.clone());
    }

    merge_input_flags(&mut state.inputs, &cli.input)?;

    let tool_registry: Arc<dyn crate::engine::ToolRegistry> = Arc::new(project.tool_registry());
    log::debug!("Starting runtime");
    log::debug!("state: {state:?}");
    log::debug!("engine: {}", engine.name());
    let runtime = Runtime::new(
        workflow,
        state,
        conversation,
        knowledge,
        engine,
        tool_registry,
        Settings::default(),
    )?;
    let mut events = runtime.run();

    let mut stdout = std::io::stdout().lock();
    while let Some(event) = events.next().await {
        match event {
            WorkflowEvent::TaskStarted { name, turn } => {
                log::info!("task started: {} ({})", name, turn.as_str());
            }
            WorkflowEvent::TaskTurn(TurnEvent::Started { path }) => {
                log::info!("turn started: {}", path.as_str());
            }
            WorkflowEvent::TaskTurn(TurnEvent::Delta { text, .. }) => {
                stdout.write_all(text.as_bytes())?;
                stdout.flush()?;
            }
            WorkflowEvent::TaskTurn(TurnEvent::ToolCall { path, call }) => {
                log::info!(
                    "turn tool_call: {} {} id={}",
                    path.as_str(),
                    call.function.name,
                    call.id
                );
            }
            WorkflowEvent::TaskTurn(TurnEvent::ToolResult { path, result }) => {
                log::info!("turn tool_result: {} id={}", path.as_str(), result.id);
            }
            WorkflowEvent::TaskTurn(TurnEvent::Finished {
                path, stop_reason, ..
            }) => {
                stdout.write_all(b"\n")?;
                stdout.flush()?;
                log::info!("turn finished: {} ({:?})", path.as_str(), stop_reason);
            }
            WorkflowEvent::TaskTurn(TurnEvent::Skipped { path, reason }) => {
                log::info!("turn skipped: {} ({:?})", path.as_str(), reason);
            }
            WorkflowEvent::TaskTurn(TurnEvent::Failed { path, error }) => {
                eprintln!(
                    "ailly: turn {} failed: {}",
                    path.as_str(),
                    format_engine_error(&format!("{error:#}"))
                );
                return Err(RunError::Reported);
            }
            WorkflowEvent::TaskFinished { name, result, next } => {
                log::info!("task finished: {name} (result={result}, next={next:?})");
            }
            WorkflowEvent::WorkflowFinished { reason } => match reason {
                WorkflowStopReason::Completed => {
                    log::info!("workflow completed");
                    return Ok(());
                }
                WorkflowStopReason::UnknownNext { task, result } => {
                    eprintln!(
                        "ailly: workflow halted at task {task:?}: no `next` entry for result {result:?}"
                    );
                    return Err(RunError::Reported);
                }
                WorkflowStopReason::TaskFailed { task, error } => {
                    eprintln!(
                        "ailly: task {task:?} failed: {}",
                        format_engine_error(&format!("{error:#}"))
                    );
                    return Err(RunError::Reported);
                }
                WorkflowStopReason::StatePersistFailed { error } => {
                    eprintln!(
                        "ailly: failed to persist workflow state: {}",
                        format_engine_error(&format!("{error:#}"))
                    );
                    return Err(RunError::Reported);
                }
                WorkflowStopReason::Cancelled => {
                    log::info!("workflow cancelled");
                    return Err(RunError::Reported);
                }
                WorkflowStopReason::Paused { task, result } => {
                    eprintln!("ailly: workflow paused at task {task:?} (result={result:?}).");
                    eprintln!(
                        "       Clear the `*Draft` marker in the gated artifact and re-run `ailly -w {workflow_name}` to continue."
                    );
                    return Ok(());
                }
            },
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{RunError, format_engine_error, merge_input_flags, resolve_engine_kind};
    use std::collections::BTreeMap;

    #[test]
    fn input_flag_parses_key_equals_value() {
        let mut inputs = BTreeMap::new();
        let flags = vec!["topic=widgets".to_string()];

        merge_input_flags(&mut inputs, &flags).expect("parses KEY=VALUE");

        assert_eq!(inputs.get("topic").map(String::as_str), Some("widgets"));
    }

    #[test]
    fn input_flag_rejects_entry_without_equals() {
        let mut inputs = BTreeMap::new();
        let flags = vec!["bad-entry".to_string()];

        let err = merge_input_flags(&mut inputs, &flags).expect_err("rejects without =");

        match err {
            RunError::Setup(error) => {
                let msg = format!("{error:#}");
                assert!(
                    msg.contains("KEY=VALUE") && msg.contains("bad-entry"),
                    "expected setup error to name the bad entry and the format; got: {msg}"
                );
            }
            RunError::Reported => panic!("expected RunError::Setup, got Reported"),
        }
        assert!(inputs.is_empty(), "no entries should be inserted on error");
    }

    #[test]
    fn persisted_state_takes_precedence_over_input_flag() {
        let mut inputs = BTreeMap::new();
        inputs.insert("topic".to_string(), "from-state".to_string());
        let flags = vec!["topic=from-flag".to_string()];

        merge_input_flags(&mut inputs, &flags).expect("merge OK");

        assert_eq!(
            inputs.get("topic").map(String::as_str),
            Some("from-state"),
            "persisted value must win"
        );
    }

    #[test]
    fn input_flag_populates_state_inputs_when_unset() {
        let mut inputs = BTreeMap::new();
        let flags = vec!["topic=fresh".to_string(), "owner=ailly".to_string()];

        merge_input_flags(&mut inputs, &flags).expect("merge OK");

        assert_eq!(inputs.get("topic").map(String::as_str), Some("fresh"));
        assert_eq!(inputs.get("owner").map(String::as_str), Some("ailly"));
    }

    #[test]
    fn input_flag_value_with_embedded_equals_keeps_full_remainder() {
        let mut inputs = BTreeMap::new();
        let flags = vec!["expr=a=b=c".to_string()];

        merge_input_flags(&mut inputs, &flags).expect("merge OK");

        assert_eq!(inputs.get("expr").map(String::as_str), Some("a=b=c"));
    }

    #[test]
    fn extracts_anthropic_error_body() {
        let raw = r#"CompletionError: ProviderError: SSE Error: Invalid status code 400 Bad Request with message: {"type":"error","error":{"type":"invalid_request_error","message":"Your credit balance is too low to access the Anthropic API. Please go to Plans & Billing to upgrade or purchase credits."},"request_id":"req_011CaeBoJpuo3ZqyjjwXU2aR"}"#;
        assert_eq!(
            format_engine_error(raw),
            "invalid_request_error: Your credit balance is too low to access the Anthropic API. Please go to Plans & Billing to upgrade or purchase credits."
        );
    }

    #[test]
    fn returns_raw_when_no_json() {
        let raw = "some opaque transport error";
        assert_eq!(format_engine_error(raw), raw);
    }

    #[test]
    fn returns_raw_when_json_lacks_error_field() {
        let raw = r#"prefix: {"unrelated": 42}"#;
        assert_eq!(format_engine_error(raw), raw);
    }

    #[cfg(feature = "bedrock")]
    #[test]
    fn resolve_engine_kind_accepts_bedrock_when_feature_on() {
        use super::EngineKind;
        let kind = resolve_engine_kind(Some("bedrock")).expect("bedrock should resolve");
        assert!(matches!(kind, EngineKind::Bedrock));
    }

    #[cfg(not(feature = "bedrock"))]
    #[test]
    fn resolve_engine_kind_rejects_bedrock_when_feature_off() {
        let err = resolve_engine_kind(Some("bedrock")).expect_err("bedrock should be unknown");
        assert!(err.to_string().contains("unknown engine"));
    }

    #[test]
    fn unknown_engine_error_lists_bedrock_only_when_feature_on() {
        let err =
            resolve_engine_kind(Some("nonsense")).expect_err("nonsense should always be unknown");
        let msg = err.to_string();
        assert!(msg.contains("gemini"), "expected gemini listed in: {msg}");
        if cfg!(feature = "bedrock") {
            assert!(msg.contains("bedrock"), "expected bedrock listed in: {msg}");
        } else {
            assert!(!msg.contains("bedrock"), "did not expect bedrock in: {msg}");
        }
    }
}
