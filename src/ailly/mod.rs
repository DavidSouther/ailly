mod args;
mod logging;

pub use args::{Cli, LogFormat};
pub use logging::init as init_logging;

use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use futures::StreamExt;
use serde::Serialize;
use vfs::{MemoryFS, PhysicalFS, VfsPath};

use crate::content::Conversation;
use crate::engine::{
    Engine, Generator, Noop, Settings, StopReason, TurnEvent, anthropic_from_env, openai_from_env,
};
#[cfg(feature = "bedrock")]
use crate::engine::bedrock_from_env;

const DEFAULT_ANTHROPIC_MODEL: &str = "claude-sonnet-4-5";
const DEFAULT_OPENAI_MODEL: &str = "gpt-4o-mini";
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
    let engine_kind = resolve_engine_kind(cli.engine.as_deref())?;
    let model = cli.model.clone();

    log::info!(
        "ailly starting; root={}, engine={:?}, model={:?}",
        cli.root().display(),
        engine_kind,
        model
    );

    let engine: Arc<dyn Engine> = build_engine(engine_kind, model.as_deref())?;
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
    #[cfg(feature = "bedrock")]
    Bedrock,
}

fn resolve_engine_kind(raw: Option<&str>) -> Result<EngineKind> {
    match raw.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        None | Some("") | Some("noop") => Ok(EngineKind::Noop),
        Some("anthropic") | Some("claude") => Ok(EngineKind::Anthropic),
        Some("openai") | Some("gpt") => Ok(EngineKind::Openai),
        #[cfg(feature = "bedrock")]
        Some("bedrock") => Ok(EngineKind::Bedrock),
        Some(other) => Err(anyhow!(
            "unknown engine {other:?}; expected one of: noop, anthropic, openai{}",
            if cfg!(feature = "bedrock") { ", bedrock" } else { "" }
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
        #[cfg(feature = "bedrock")]
        EngineKind::Bedrock => {
            let model = model.unwrap_or(DEFAULT_BEDROCK_MODEL);
            Ok(Arc::new(bedrock_from_env(model)?))
        }
    }
}

async fn load_conversation(cli: &Cli) -> Result<Conversation> {
    if let Some(prompt) = cli.prompt.as_deref() {
        load_synthetic_prompt(prompt).await
    } else {
        load_from_root(&cli.root()).await
    }
}

async fn load_synthetic_prompt(prompt: &str) -> Result<Conversation> {
    #[derive(Serialize)]
    struct Turn<'a> {
        prompt: &'a str,
    }

    let fs = VfsPath::new(MemoryFS::new());
    let root = fs.join("prompt").context("joining synthetic root")?;
    root.create_dir().context("creating synthetic root")?;

    let turn_path = root.join("01_prompt.toml").context("joining turn file")?;
    let body = toml::to_string(&Turn { prompt }).context("serializing synthetic turn")?;
    let mut writer = turn_path
        .create_file()
        .context("opening synthetic turn for write")?;
    writer
        .write_all(body.as_bytes())
        .context("writing synthetic turn")?;
    drop(writer);

    Conversation::load(root)
        .await
        .context("loading synthetic conversation")
}

async fn load_from_root(root: &Path) -> Result<Conversation> {
    if !root.exists() {
        return Err(anyhow!("root path does not exist: {}", root.display()));
    }
    let vfs_root = VfsPath::new(PhysicalFS::new(root.to_path_buf()));
    Conversation::load(vfs_root)
        .await
        .with_context(|| format!("loading conversation at {}", root.display()))
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
        let err = resolve_engine_kind(Some("nonsense"))
            .expect_err("nonsense should always be unknown");
        let msg = err.to_string();
        if cfg!(feature = "bedrock") {
            assert!(msg.contains("bedrock"), "expected bedrock listed in: {msg}");
        } else {
            assert!(!msg.contains("bedrock"), "did not expect bedrock in: {msg}");
        }
    }
}

