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

use crate::content::conversation::Content;
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
    async fn execute(&self, call: &ContentBlock) -> Result<ContentBlock, ToolError> {
        let ContentBlock::ToolUse { id, name, input: _ } = call else {
            return Err(ToolError::NotAToolUse);
        };
        let mut scripts = self
            .scripts
            .lock()
            .expect("NoopToolExecutor mutex poisoned");
        let queue = scripts.get_mut(name);
        let reply =
            queue
                .and_then(VecDeque::pop_front)
                .ok_or_else(|| ToolError::NoopExhausted {
                    name: name.clone(),
                    // The named queue is empty (or absent) at this call. With no
                    // per-name counter on the struct, the honest reportable value
                    // is the depth remaining at exhaustion, which is zero.
                    call_index: 0,
                })?;
        Ok(ContentBlock::ToolResult {
            tool_use_id: id.clone(),
            content: Content::from(reply),
            is_error: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ContentBlock;
    use super::NoopToolExecutor;
    use super::ToolError;
    use super::ToolExecutor;
    use crate::content::conversation::Content;
    use crate::content::conversation::ToolUseId;

    fn tool_use(id: &str, name: &str) -> ContentBlock {
        ContentBlock::ToolUse {
            id: ToolUseId::from(id),
            name: String::from(name),
            input: serde_yaml_ng::Value::Null,
        }
    }

    #[tokio::test]
    async fn execute_returns_scripted_result_echoing_call_id() {
        let executor = NoopToolExecutor::from_scripts([(
            "lookup_policy",
            vec![String::from("status: in_force")],
        )]);

        let result = executor
            .execute(&tool_use("toolu_001", "lookup_policy"))
            .await
            .expect("scripted result is served");

        assert_eq!(
            result,
            ContentBlock::ToolResult {
                tool_use_id: ToolUseId::from("toolu_001"),
                content: Content::Text(String::from("status: in_force")),
                is_error: None,
            }
        );
    }

    #[tokio::test]
    async fn execute_errors_when_script_exhausted() {
        let executor = NoopToolExecutor::from_scripts([(
            "lookup_policy",
            vec![String::from("status: in_force")],
        )]);

        executor
            .execute(&tool_use("toolu_001", "lookup_policy"))
            .await
            .expect("first call is served");

        let err = executor
            .execute(&tool_use("toolu_002", "lookup_policy"))
            .await
            .expect_err("second call exhausts the script");

        assert!(matches!(
            err,
            ToolError::NoopExhausted { ref name, .. } if name == "lookup_policy"
        ));
    }

    #[tokio::test]
    async fn execute_errors_on_non_tool_use_block() {
        let executor = NoopToolExecutor::default();

        let err = executor
            .execute(&ContentBlock::Text {
                text: String::from("not a tool call"),
            })
            .await
            .expect_err("a non-tool_use block is rejected");

        assert!(matches!(err, ToolError::NotAToolUse));
    }
}
