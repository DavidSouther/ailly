mod args;
mod logging;

pub use args::{Cli, LogFormat, parse_workflow_arg};
pub use logging::init as init_logging;

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use futures::StreamExt;
use vfs::{PhysicalFS, VfsPath};

use crate::content::Conversation;
#[cfg(feature = "bedrock")]
use crate::engine::bedrock_from_env;
use crate::engine::{
    Engine, Generator, Noop, Settings, StopReason, TurnEvent, anthropic_from_env, gemini_from_env,
    openai_from_env,
};
use crate::knowledge::skills::FsSkillRepository;
use crate::workflow::{Runtime, Workflow, WorkflowEvent, WorkflowState, WorkflowStopReason};

use std::collections::VecDeque;

const DEFAULT_ANTHROPIC_MODEL: &str = "claude-sonnet-4-5";
const DEFAULT_OPENAI_MODEL: &str = "gpt-4o-mini";
const DEFAULT_GEMINI_MODEL: &str = "gemini-2.5-flash";
#[cfg(feature = "bedrock")]
const DEFAULT_BEDROCK_MODEL: &str = "us.anthropic.claude-sonnet-4-5-20250929-v1:0";

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

async fn load_conversation(cli: &Cli) -> Result<Conversation> {
    let root = cli.root();
    let mut conversation = if root.exists() {
        load_from_root(&root).await?
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

    let mut conversation = load_from_root(&root).await?;
    conversation.clean().await?;
    for idx in 0..conversation.turn_count() {
        log::info!("cleaned: {}", conversation.turn(idx).path().as_str());
    }
    Ok(())
}

/// Read and parse `workflow.toml` from the conversation root, validating
/// that its declared name matches what `-w` requested.
fn load_workflow_definition(
    vfs_root: &VfsPath,
    expected_name: &str,
    raw_arg: &str,
) -> Result<Workflow, RunError> {
    let workflow_path = vfs_root
        .join("workflow.toml")
        .with_context(|| format!("resolving workflow.toml under {}", vfs_root.as_str()))?;
    if !workflow_path.exists().unwrap_or(false) {
        return Err(RunError::Setup(anyhow!(
            "no workflow.toml at conversation root {}",
            vfs_root.as_str()
        )));
    }
    let workflow_text = workflow_path
        .read_to_string()
        .with_context(|| format!("reading {}", workflow_path.as_str()))?;
    let workflow: Workflow = toml::from_str(&workflow_text)
        .with_context(|| format!("parsing {}", workflow_path.as_str()))?;

    if workflow.name != expected_name {
        return Err(RunError::Setup(anyhow!(
            "workflow.toml declares name {:?} but `-w {raw_arg}` requested {expected_name:?}",
            workflow.name
        )));
    }

    Ok(workflow)
}

async fn load_from_root(root: &Path) -> Result<Conversation> {
    let vfs_root = VfsPath::new(PhysicalFS::new(root));
    let skills = FsSkillRepository::new(&vfs_root);
    Conversation::load(vfs_root, &skills)
        .await
        .with_context(|| format!("loading conversation at {}", root.display()))
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
    let vfs_root = VfsPath::new(PhysicalFS::new(&root));

    let workflow = load_workflow_definition(&vfs_root, &workflow_name, raw)?;

    let mut state =
        WorkflowState::read(&vfs_root)?.unwrap_or_else(|| WorkflowState::initial(&workflow));

    if let Some(task_name) = task_override {
        state.queue = VecDeque::from(vec![task_name]);
    } else if state.queue.is_empty() {
        state.queue.push_back(workflow.start.clone());
    }

    let runtime = Runtime::new(workflow, state, vfs_root, engine, Settings::default());
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
            },
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{format_engine_error, resolve_engine_kind};

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
