//! Tool dispatch for the `Conversation::run` agentic loop.
//!
//! `ToolExecutor` is the behavior half of the tool-call contract (the schema
//! half, [`crate::content::conversation::ToolDefinition`], lives in `content`).
//! It maps one `ContentBlock::ToolUse` to one `ContentBlock::ToolResult`.
//! `NoopToolExecutor` is the deterministic, scriptable adapter — the executor
//! analogue of `NoopEngine` — that serves harness tests and the structural CI
//! gate without a live tool. Its empty `default()` serves a no-tools
//! conversation: a conversation that emits no `tool_use` never calls it.
//!
//! Type-first stub. Signatures are the Feature 1 design contract; bodies are
//! `todo!()` until the run loop is implemented.

use std::collections::BTreeMap;
use std::collections::VecDeque;
use std::sync::Mutex;

use crate::content::conversation::ContentBlock;

/// Failure modes for tool execution, surfaced into `RunError::Tool`.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    /// A `NoopToolExecutor` script was exhausted (or never present) for the
    /// named tool.
    #[error("no scripted tool result for tool `{name}` (call #{call_index})")]
    NoopExhausted { name: String, call_index: usize },
    /// `execute` received a block that was not a `ContentBlock::ToolUse`.
    #[error("executor received a non-tool_use block")]
    NotAToolUse,
}

/// Execute one tool call and produce its result.
#[async_trait::async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Execute one tool call and produce its result. `call` is a
    /// `ContentBlock::ToolUse`; the return is a `ContentBlock::ToolResult`
    /// whose `tool_use_id` echoes the call's `id`.
    ///
    /// # Errors
    /// Returns [`ToolError`] when no result can be produced for `call`
    /// (e.g. a noop script exhausted for the named tool).
    async fn execute(&self, call: &ContentBlock) -> Result<ContentBlock, ToolError>;
}

/// Deterministic, scriptable executor. Scripted `tool_result` strings keyed by
/// tool name, popped front-to-back per name in call order — the executor-side
/// analogue of `NoopEngine`'s script queue. An empty executor errors with
/// [`ToolError::NoopExhausted`] on any call, which is correct for a no-tools
/// conversation that never emits a `tool_use`.
pub struct NoopToolExecutor {
    /// tool name -> queued result strings, popped front-to-back per call.
    #[expect(
        dead_code,
        reason = "type-first stub; consumed when the executor body is implemented"
    )]
    scripts: Mutex<BTreeMap<String, VecDeque<String>>>,
}

impl NoopToolExecutor {
    /// Empty executor: any tool call errors with [`ToolError::NoopExhausted`].
    /// The no-tools default — a conversation that emits no `tool_use` never
    /// calls it.
    #[must_use]
    pub fn new() -> Self {
        Self {
            scripts: Mutex::new(BTreeMap::new()),
        }
    }

    /// Build from `(tool_name, replies)` pairs; replies served in call order
    /// per tool name.
    #[must_use]
    pub fn from_scripts<I, S>(scripts: I) -> Self
    where
        I: IntoIterator<Item = (S, Vec<String>)>,
        S: Into<String>,
    {
        let map = scripts
            .into_iter()
            .map(|(name, replies)| (name.into(), replies.into_iter().collect()))
            .collect();
        Self {
            scripts: Mutex::new(map),
        }
    }
}

impl Default for NoopToolExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl ToolExecutor for NoopToolExecutor {
    async fn execute(&self, _call: &ContentBlock) -> Result<ContentBlock, ToolError> {
        todo!(
            "Feature 1 step 4/6: pop the named tool's scripted result and wrap it in a ToolResult"
        )
    }
}
