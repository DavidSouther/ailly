use futures::Stream;
use rig::completion::Usage as RigUsage;
use rig::message::Message;
use rig::tool::ToolDyn;
use std::collections::HashMap;
use std::fmt;
use std::pin::Pin;
use std::sync::Arc;

use crate::content::Preamble;

pub mod generator;
pub mod noop;
pub mod rig_engine;

pub use generator::{Generator, SkipReason, TurnEvent};
pub use noop::Noop;
#[cfg(feature = "bedrock")]
pub use rig_engine::bedrock_from_env;
pub use rig_engine::{RigEngine, anthropic_from_env, gemini_from_env, openai_from_env};

use crate::content::{AssistantResponse, ResponseUsage};

pub const DEFAULT_REQUEST_LIMIT: usize = 5;
pub const DEFAULT_MAX_TOOL_TURNS: usize = 5;

#[derive(Debug, Clone)]
pub struct ModelId(String);
impl From<&'static str> for ModelId {
    fn from(value: &'static str) -> Self {
        Self(value.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct EngineName(String);

impl From<&'static str> for EngineName {
    fn from(value: &'static str) -> Self {
        Self(value.to_string())
    }
}

#[derive(Debug, Clone)]
pub struct Settings {
    model_id: Option<ModelId>,
    request_limit: usize,
    max_tool_turns: usize,
    isolated: bool,
    overwrite: bool,
    /// When `true`, an unknown tool name on a turn's chain produces a
    /// `TurnEvent::Failed` and the engine is never invoked for that turn.
    /// When `false`, unknown names are dropped with a `log::warn!` and the
    /// resolved subset is passed to the engine.
    strict_tools: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model_id: None,
            request_limit: DEFAULT_REQUEST_LIMIT,
            max_tool_turns: DEFAULT_MAX_TOOL_TURNS,
            isolated: false,
            overwrite: false,
            strict_tools: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Usage {
    input_tokens: u32,
    output_tokens: u32,
}

impl From<RigUsage> for Usage {
    fn from(u: RigUsage) -> Self {
        Self {
            input_tokens: u.input_tokens.try_into().unwrap_or(u32::MAX),
            output_tokens: u.output_tokens.try_into().unwrap_or(u32::MAX),
        }
    }
}

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum StopReason {
    EndTurn,
    StopSequence,
    MaxTokens,
    Refusal,
    ToolLimit,
    Error(String),
}

impl fmt::Display for StopReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            StopReason::EndTurn => "end_turn",
            StopReason::StopSequence => "stop_sequence",
            StopReason::MaxTokens => "max_tokens",
            StopReason::Refusal => "refusal",
            StopReason::ToolLimit => "tool_limit",
            StopReason::Error(_) => "error",
        };
        f.write_str(s)
    }
}

#[derive(Debug, Clone)]
pub struct EngineResponse {
    pub(crate) text: String,
    pub(crate) engine_name: EngineName,
    pub(crate) model_id: ModelId,
    pub(crate) stop_reason: StopReason,
    pub(crate) usage: Option<Usage>,
}

impl EngineResponse {
    /// Build a final engine response. The four metadata fields
    /// (`engine`, `model`, `stop_reason`, `usage`) are required because
    /// downstream consumers (recorder, layering guard) rely on them being
    /// populated for any non-error final.
    pub fn new(
        text: String,
        engine: EngineName,
        model: ModelId,
        stop_reason: StopReason,
        usage: Option<Usage>,
    ) -> Self {
        Self {
            text,
            engine_name: engine,
            model_id: model,
            stop_reason,
            usage,
        }
    }
}

/// Map an engine's `EngineResponse` onto a content-layer `AssistantResponse`,
/// preserving the engine name, model, stop reason, and usage so each turn
/// file carries the same provenance regardless of which call site recorded
/// the response (`Generator::run` for normal turns, the workflow runtime for
/// evaluator turns).
impl From<&EngineResponse> for AssistantResponse {
    fn from(value: &EngineResponse) -> Self {
        Self {
            text: value.text.clone(),
            model: Some(value.model_id.0.clone()),
            engine: Some(value.engine_name.0.clone()),
            stop_reason: Some(value.stop_reason.to_string()),
            usage: value.usage.as_ref().map(|u| ResponseUsage {
                input_tokens: u.input_tokens,
                output_tokens: u.output_tokens,
            }),
        }
    }
}

/// Outbound events from an `Engine::stream` impl.
///
/// `Reasoning` and `ReasoningDelta` carry the model's thinking trace.
/// They MUST be emitted in arrival order so the recorder's accumulator
/// can interleave them with `Text` and `ToolCall` correctly. `id` on
/// `ReasoningDelta` is the provider-supplied reasoning slot key; deltas
/// with a matching `Some(id)` are merged by the recorder. `id: None`
/// deltas are kept in distinct slots, mirroring rig's
/// `merge_reasoning_blocks_keeps_none_ids_separate_items`.
#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum EngineEvent {
    Text(String),
    ToolCall(rig::message::ToolCall),
    ToolResult(rig::message::ToolResult),
    Reasoning(rig::message::Reasoning),
    ReasoningDelta { id: Option<String>, text: String },
    Final(EngineResponse),
}

pub type EngineStream = Pin<Box<dyn Stream<Item = EngineEvent> + Send>>;

#[derive(Debug, Clone, Default)]
pub struct EngineInput {
    pub preamble: Preamble,
    pub history: Vec<Message>,
}

impl EngineInput {
    pub fn with_history(history: Vec<Message>) -> Self {
        Self {
            preamble: Preamble::default(),
            history,
        }
    }
}

pub trait Engine: Send + Sync {
    fn name(&self) -> &'static str;

    fn stream(
        &self,
        input: EngineInput,
        settings: &Settings,
        tools: &[Arc<dyn ToolDyn>],
        request_label: &str,
    ) -> anyhow::Result<EngineStream>;
}

/// Resolves a tool name declared on disk to a concrete `ToolDyn` implementation.
pub trait ToolRegistry: Send + Sync {
    fn resolve(&self, name: &str) -> Option<Arc<dyn rig::tool::ToolDyn>>;
}

/// Registry that knows about no tools. Used as the default when the CLI does
/// not register implementations.
#[derive(Debug, Default)]
pub struct EmptyRegistry;

impl ToolRegistry for EmptyRegistry {
    fn resolve(&self, _name: &str) -> Option<Arc<dyn rig::tool::ToolDyn>> {
        None
    }
}

/// In-memory registry mapping names to `ToolDyn` implementations.
#[derive(Default, Clone)]
pub struct HashMapRegistry {
    tools: HashMap<String, Arc<dyn rig::tool::ToolDyn>>,
}

impl HashMapRegistry {
    pub fn insert(&mut self, name: impl Into<String>, tool: Arc<dyn rig::tool::ToolDyn>) {
        self.tools.insert(name.into(), tool);
    }
}

impl ToolRegistry for HashMapRegistry {
    fn resolve(&self, name: &str) -> Option<Arc<dyn rig::tool::ToolDyn>> {
        self.tools.get(name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_reason_display_uses_snake_case() {
        assert_eq!(StopReason::EndTurn.to_string(), "end_turn");
        assert_eq!(StopReason::StopSequence.to_string(), "stop_sequence");
        assert_eq!(StopReason::MaxTokens.to_string(), "max_tokens");
        assert_eq!(StopReason::Refusal.to_string(), "refusal");
        assert_eq!(StopReason::ToolLimit.to_string(), "tool_limit");
        assert_eq!(StopReason::Error(String::new()).to_string(), "error");
        assert_eq!(
            StopReason::Error("anything at all".to_string()).to_string(),
            "error"
        );
    }

    #[test]
    fn usage_from_rig_usage_passes_through_within_u32_range() {
        let mut rig = RigUsage::new();
        rig.input_tokens = 1234;
        rig.output_tokens = u32::MAX as u64;

        let usage = Usage::from(rig);

        assert_eq!(usage.input_tokens, 1234);
        assert_eq!(usage.output_tokens, u32::MAX);
    }

    /// Layering guard. The `engine/` tree must not reach into the content
    /// vocabulary. The single allowed exception is the conversation type,
    /// which appears as the typed `Generator::conversation` field and its
    /// loader sites in tests. Forbidden tokens are constructed from fragments
    /// so this test's own source does not match the assertion.
    #[test]
    fn engine_module_does_not_leak_content_metadata_vocabulary() {
        use std::path::Path;

        fn collect_rs_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let entry = entry.unwrap();
                let path = entry.path();
                if path.is_dir() {
                    collect_rs_files(&path, out);
                } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                    out.push(path);
                }
            }
        }

        let mut rs_files = Vec::new();
        collect_rs_files(Path::new("src/engine"), &mut rs_files);
        assert!(
            !rs_files.is_empty(),
            "engine module must contain at least one .rs file (cwd: {:?})",
            std::env::current_dir().ok()
        );

        let conversation = format!("{}{}", "Conver", "sation");
        let forbidden = [
            format!("{}{}", "Content", "Meta"),
            format!("{}{}{}", "Conver", "sation", "Turn"),
            format!("{}{}", "ailly", "rc"),
        ];
        for path in &rs_files {
            let body = std::fs::read_to_string(path).unwrap();
            for token in &forbidden {
                assert!(
                    !body.contains(token.as_str()),
                    "{}: engine layer must not reference forbidden token {token:?}",
                    path.display()
                );
            }
        }

        let allow = Path::new("src/engine/generator.rs");
        for path in &rs_files {
            if path == allow {
                continue;
            }
            let body = std::fs::read_to_string(path).unwrap();
            assert!(
                !body.contains(conversation.as_str()),
                "{}: only {} may reference the conversation type; this file must not",
                path.display(),
                allow.display()
            );
        }
    }

    #[test]
    fn usage_from_rig_usage_saturates_oversize_to_u32_max() {
        let mut rig = RigUsage::new();
        rig.input_tokens = u64::MAX;
        rig.output_tokens = (u32::MAX as u64) + 1;

        let usage = Usage::from(rig);

        assert_eq!(usage.input_tokens, u32::MAX);
        assert_eq!(usage.output_tokens, u32::MAX);
    }
}
