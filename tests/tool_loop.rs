//! Feature test for Feature 1: the tool-call harness loop.
//!
//! User story:
//!
//! **Given** a conversation whose blank assistant slot is about to be filled,
//! and a model (noop-scripted) whose first reply is an assistant `tool_use`
//! block and whose second reply is assistant text, and a tool executor
//! (noop-scripted) that returns the matching `tool_result`,
//! **When** the operator drives `Conversation::run` with both the engine and
//! the executor,
//! **Then** the resulting session has the agentic shape
//! `user → assistant(tool_use) → tool(tool_result) → assistant(text)`:
//! the assistant's `tool_use` was dispatched through the executor, its
//! `tool_result` was appended as a `Role::Tool` message, and a fresh blank
//! assistant slot was filled with the model's follow-up text.
//!
//! This drives `Conversation::run` directly (rather than the `ailly run` CLI
//! handler) so the engine and the executor can both be scripted with
//! `NoopEngine` and `NoopToolExecutor` — no live model, no live tool. The CLI
//! wiring is exercised by `tests/run.rs`; this test pins the loop's session
//! shape, which is Feature 1's defining behavior.

use std::marker::PhantomData;

use ailly_two::content::conversation::Content;
use ailly_two::content::conversation::ContentBlock;
use ailly_two::content::conversation::Conversation;
use ailly_two::content::conversation::Message;
use ailly_two::content::conversation::Meta;
use ailly_two::content::conversation::ModelId;
use ailly_two::content::conversation::Role;
use ailly_two::content::conversation::SpanId;
use ailly_two::content::conversation::TokenCounts;
use ailly_two::content::conversation::ToolUseId;
use ailly_two::content::conversation::Trace;
use ailly_two::engine::engine::CompletionResponse;
use ailly_two::engine::engine::NoopEngine;
use ailly_two::knowledge::tools::NoopToolExecutor;

/// The id linking the assistant's `tool_use` to its `tool_result`.
const CALL_ID: &str = "toolu_001";
/// The tool the assistant calls.
const TOOL_NAME: &str = "lookup_policy";

fn trace(span: &str) -> Trace {
    Trace {
        span_id: SpanId::from(span),
        model: ModelId::from("noop"),
        tokens: TokenCounts {
            input: 0,
            output: 0,
            cache_hit: None,
            cache_write: None,
        },
        latency_ms: 0,
        events: Vec::new(),
    }
}

#[tokio::test]
async fn run_drives_tool_use_then_tool_result_then_text() {
    // Arrange: a conversation with a user turn and one blank assistant slot,
    // exactly the shape `ailly assemble` produces before a tool-calling run.
    let mut conversation = Conversation {
        meta: Meta {
            model: ModelId::from("noop"),
            debug: false,
            assembly: None,
            binding: Default::default(),
        },
        session: vec![
            Message {
                role: Role::User,
                body: Some(Content::Text(String::from(
                    "Is policy P-42 in force for claim 42?",
                ))),
                cache: false,
                trace: None,
                _phase: PhantomData,
            },
            Message {
                role: Role::Assistant,
                body: None,
                cache: false,
                trace: None,
                _phase: PhantomData,
            },
        ],
    };

    // The engine's first scripted reply is an assistant `tool_use` block; its
    // second is the assistant's text once the tool result is in context.
    let engine = NoopEngine::from_scripts(vec![
        CompletionResponse {
            content: Content::Blocks(vec![ContentBlock::ToolUse {
                id: ToolUseId::from(CALL_ID),
                name: String::from(TOOL_NAME),
                input: serde_yaml_ng::from_str("policy_id: P-42").expect("tool input parses"),
            }]),
            trace: trace("noop-tooluse"),
        },
        CompletionResponse {
            content: Content::Text(String::from("Policy P-42 is in force; auto-approve.")),
            trace: trace("noop-text"),
        },
    ]);

    // The executor returns the scripted `tool_result` for the named tool.
    let executor =
        NoopToolExecutor::from_scripts([(TOOL_NAME, vec![String::from("status: in_force")])]);

    // Act: drive the loop with both the engine and the executor.
    conversation
        .run(&engine, &executor)
        .await
        .expect("the tool loop runs to completion");

    // Assert: the session is `user → assistant(tool_use) → tool(tool_result) →
    // assistant(text)`.
    let roles: Vec<Role> = conversation.session.iter().map(|m| m.role).collect();
    assert_eq!(
        roles,
        vec![Role::User, Role::Assistant, Role::Tool, Role::Assistant],
        "session shape is user, assistant(tool_use), tool(tool_result), assistant(text)",
    );

    // session[1]: assistant turn carrying exactly one `tool_use` block.
    assert_tool_use(&conversation.session[1], CALL_ID, TOOL_NAME);

    // session[2]: tool turn carrying exactly one `tool_result` echoing the call id.
    assert_tool_result(&conversation.session[2], CALL_ID);

    // session[3]: assistant turn carrying the model's follow-up text.
    let final_assistant = &conversation.session[3];
    match final_assistant
        .body
        .as_ref()
        .expect("final assistant turn is filled")
    {
        Content::Text(text) => {
            assert_eq!(text, "Policy P-42 is in force; auto-approve.");
        }
        Content::Blocks(_) => panic!("final assistant turn carries text, not blocks"),
    }

    // The loop terminated cleanly: no blank slot remains.
    assert!(
        conversation.next_blank_assistant().is_none(),
        "no blank assistant slot remains after the loop",
    );
}

fn assert_tool_use(message: &Message, expected_id: &str, expected_name: &str) {
    assert_eq!(message.role, Role::Assistant, "tool_use turn is assistant");
    match message
        .body
        .as_ref()
        .expect("assistant tool_use turn is filled")
    {
        Content::Blocks(blocks) => {
            assert_eq!(blocks.len(), 1, "exactly one tool_use block");
            match &blocks[0] {
                ContentBlock::ToolUse { id, name, .. } => {
                    assert_eq!(id.as_ref(), expected_id, "tool_use id");
                    assert_eq!(name, expected_name, "tool_use name");
                }
                other => panic!("expected a ToolUse block, got {other:?}"),
            }
        }
        Content::Text(_) => panic!("tool_use turn carries blocks, not text"),
    }
}

fn assert_tool_result(message: &Message, expected_id: &str) {
    assert_eq!(message.role, Role::Tool, "tool_result turn is Role::Tool");
    match message.body.as_ref().expect("tool turn is filled") {
        Content::Blocks(blocks) => {
            assert_eq!(blocks.len(), 1, "exactly one tool_result block");
            match &blocks[0] {
                ContentBlock::ToolResult { tool_use_id, .. } => {
                    assert_eq!(
                        tool_use_id.as_ref(),
                        expected_id,
                        "tool_result echoes the tool_use id",
                    );
                }
                other => panic!("expected a ToolResult block, got {other:?}"),
            }
        }
        Content::Text(_) => panic!("tool turn carries blocks, not text"),
    }
}
