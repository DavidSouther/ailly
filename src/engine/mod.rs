use futures::Stream;
use rig::completion::Usage as RigUsage;
use rig::message::Message;
use rig::tool::ToolDyn;
use std::{fmt, sync::Arc};
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

pub const DEFAULT_REQUEST_LIMIT: usize = 5;
pub const DEFAULT_MAX_TOOL_TURNS: usize = 5;

#[derive(Debug, Clone)]
pub struct Settings {
    pub model: Option<String>,
    pub request_limit: usize,
    pub max_tool_turns: usize,
    pub isolated: bool,
    pub overwrite: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model: None,
            request_limit: DEFAULT_REQUEST_LIMIT,
            max_tool_turns: DEFAULT_MAX_TOOL_TURNS,
            isolated: false,
            overwrite: false,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
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
    pub text: String,
    pub model: Option<String>,
    pub stop_reason: StopReason,
    pub usage: Option<Usage>,
}

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum EngineEvent {
    Text(String),
    ToolCall(rig::message::ToolCall),
    ToolResult(rig::message::ToolResult),
    Final(EngineResponse),
}

pub type EngineStream = Pin<Box<dyn Stream<Item = EngineEvent> + Send>>;

#[derive(Debug, Clone, Default)]
pub struct EngineInput {
    pub preamble: Preamble,
    pub history: Vec<Message>,
}

impl EngineInput {
    pub fn with_history(history: Vec<Message>)-> Self {
        Self { preamble: Preamble::default(), history }
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
