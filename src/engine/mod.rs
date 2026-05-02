use futures::Stream;
use rig::message::Message;
use std::pin::Pin;

pub mod generator;
pub mod noop;
pub mod rig_engine;

pub use generator::{Generator, SkipReason, TurnEvent};
pub use noop::Noop;
pub use rig_engine::{RigEngine, anthropic_from_env, openai_from_env};

pub const DEFAULT_REQUEST_LIMIT: usize = 5;

#[derive(Debug, Clone)]
pub struct Settings {
    pub model: Option<String>,
    pub request_limit: usize,
    pub isolated: bool,
    pub overwrite: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            model: None,
            request_limit: DEFAULT_REQUEST_LIMIT,
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

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum StopReason {
    EndTurn,
    StopSequence,
    MaxTokens,
    Refusal,
    Error(String),
}

#[derive(Debug, Clone)]
pub struct EngineResponse {
    pub text: String,
    pub stop_reason: StopReason,
    pub usage: Option<Usage>,
}

#[non_exhaustive]
#[derive(Debug, Clone)]
pub enum EngineEvent {
    Text(String),
    Final(EngineResponse),
}

pub type EngineStream = Pin<Box<dyn Stream<Item = EngineEvent> + Send>>;

pub trait Engine: Send + Sync {
    fn stream(
        &self,
        history: Vec<Message>,
        settings: &Settings,
        request_label: &str,
    ) -> anyhow::Result<EngineStream>;
}
