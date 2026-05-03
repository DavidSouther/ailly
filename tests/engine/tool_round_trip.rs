//! Feature test for the engine tool-call slice.
//!
//! User story (narrative):
//!
//! A developer registers a single `echo` tool and runs a `Generator` over a
//! one-turn `Conversation`. The `Noop` engine recognizes the `USE <tool> WITH
//! <args>` directive in the user prompt and emits one tool round-trip: text,
//! then a tool call against `echo`, then the tool result the engine
//! dispatched, then more text, then a final response. The developer expects
//! the round-trip to surface as `TurnEvent::ToolCall` and
//! `TurnEvent::ToolResult` in stream order between the two `Delta` runs, and
//! to find the round-trip persisted to the per-turn TOML file as
//! `[[response]]` entries with role `tool_call` and `tool_result` whose `id`
//! matches the call the engine issued.

use std::sync::Arc;

use futures::StreamExt;
use serde::Deserialize;
use serde_json::json;

use ailly::content::Conversation;
use ailly::engine::{
    Generator, HashMapRegistry, Noop, Settings, StopReason, ToolRegistry, TurnEvent,
};
use ailly::knowledge::skills::NullSkillRepository;
use ailly::mem_fs;

use rig::completion::request::ToolDefinition;
use rig::tool::{Tool, ToolDyn};

#[derive(Debug, thiserror::Error)]
#[error("echo tool error")]
struct EchoError;

#[derive(Deserialize)]
struct EchoArgs {
    text: String,
}

#[derive(Default)]
struct EchoTool;

impl Tool for EchoTool {
    const NAME: &'static str = "echo";
    type Error = EchoError;
    type Args = EchoArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: "echo".to_string(),
            description: "echoes the input text".to_string(),
            parameters: json!({
                "type": "object",
                "properties": { "text": { "type": "string" } },
                "required": ["text"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        Ok(args.text)
    }
}

#[tokio::test]
async fn engine_surfaces_one_tool_round_trip_through_generator_and_file() {
    let fs = mem_fs! {
        "root": {
            "01.toml": "prompt = 'USE echo WITH {\"text\":\"ok\"}'\ntools = [\"echo\"]\n",
        },
    };
    let conversation = Conversation::load(fs.join("root").unwrap(), &NullSkillRepository)
        .await
        .expect("load conversation");

    let engine = Arc::new(Noop::default());
    let echo: Arc<dyn ToolDyn> = Arc::new(EchoTool);
    let mut registry = HashMapRegistry::default();
    registry.insert("echo", echo);
    let registry: Arc<dyn ToolRegistry> = Arc::new(registry);
    let generator =
        Generator::new(conversation, engine, Settings::default()).with_registry(registry);

    let events: Vec<TurnEvent> = generator.run().collect().await;

    assert!(matches!(events[0], TurnEvent::Started { .. }));

    let tool_call_idx = events
        .iter()
        .position(|e| matches!(e, TurnEvent::ToolCall { .. }))
        .expect("expected a TurnEvent::ToolCall");
    let tool_result_idx = events
        .iter()
        .position(|e| matches!(e, TurnEvent::ToolResult { .. }))
        .expect("expected a TurnEvent::ToolResult");
    let finished_idx = events
        .iter()
        .position(|e| matches!(e, TurnEvent::Finished { .. }))
        .expect("expected a TurnEvent::Finished");

    assert!(
        tool_call_idx > 0,
        "tool call must arrive after Started and at least one Delta"
    );
    assert!(
        tool_call_idx < tool_result_idx,
        "ToolCall must arrive before ToolResult, got idx {tool_call_idx} >= {tool_result_idx}"
    );
    assert!(
        tool_result_idx < finished_idx,
        "tool round-trip must complete before Finished"
    );

    for ev in &events[1..tool_call_idx] {
        assert!(
            matches!(ev, TurnEvent::Delta { .. }),
            "events between Started and ToolCall must be Delta, got {ev:?}"
        );
    }
    for ev in &events[tool_result_idx + 1..finished_idx] {
        assert!(
            matches!(ev, TurnEvent::Delta { .. }),
            "events between ToolResult and Finished must be Delta, got {ev:?}"
        );
    }

    let TurnEvent::ToolCall { call, .. } = &events[tool_call_idx] else {
        unreachable!()
    };
    assert_eq!(call.function.name, "echo");
    assert_eq!(call.id, "call_1");

    let TurnEvent::ToolResult { result, .. } = &events[tool_result_idx] else {
        unreachable!()
    };
    assert_eq!(
        result.id, "call_1",
        "tool result id pairs with tool call id"
    );

    let TurnEvent::Finished { stop_reason, .. } = &events[finished_idx] else {
        unreachable!()
    };
    assert!(
        matches!(stop_reason, StopReason::EndTurn),
        "single-round-trip run inside the default max_tool_turns budget should finish normally"
    );

    let written = fs
        .join("root/01.toml")
        .unwrap()
        .read_to_string()
        .expect("read written turn file");

    assert!(
        written.contains(r#"USE echo WITH"#),
        "user prompt preserved at top of file: {written}"
    );

    let asst_first_pos = written
        .find(r#"role = "assistant""#)
        .expect("first assistant entry in file");
    let tool_call_pos = written
        .find(r#"role = "tool_call""#)
        .expect("tool_call entry in file");
    let tool_result_pos = written
        .find(r#"role = "tool_result""#)
        .expect("tool_result entry in file");
    let asst_last_pos = written
        .rfind(r#"role = "assistant""#)
        .expect("second assistant entry in file");

    assert!(
        asst_first_pos < tool_call_pos,
        "first assistant text precedes tool call: {written}"
    );
    assert!(
        tool_call_pos < tool_result_pos,
        "tool call precedes tool result: {written}"
    );
    assert!(
        tool_result_pos < asst_last_pos,
        "tool result precedes second assistant text: {written}"
    );
    assert_ne!(
        asst_first_pos, asst_last_pos,
        "two distinct assistant entries before and after the tool round-trip"
    );

    assert!(
        written.contains(r#"name = "echo""#),
        "tool_call entry should record the tool name; got: {written}"
    );
    assert!(
        written.contains(r#"id = "call_1""#),
        "tool_call/tool_result entries should record the call id; got: {written}"
    );
}
