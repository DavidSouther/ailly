//! Feature test for the `user.clarify` tool surface slice.
//!
//! User story (narrative):
//!
//! A workflow harness author constructs a `MapKnowledgeBase` populated with a
//! known question and recorded answer, wraps it in a `ClarifyTool`, erases the
//! tool to `Arc<dyn ToolDyn>`, and inserts it into a `HashMapRegistry` under
//! the constant tool name `ClarifyTool::NAME` (`"user.clarify"`). A workflow
//! turn whose user prompt encodes `USE user.clarify WITH {"question":"<known>"}`
//! is then driven through a `Generator` over a one-turn `Conversation`. The
//! `Noop` engine recognizes the `USE` directive, asks the registry for
//! `user.clarify`, dispatches the JSON args through `ToolDyn::call`, and
//! emits the round-trip as `TurnEvent::ToolCall` followed by
//! `TurnEvent::ToolResult` between the two `Delta` runs. The harness author
//! expects the `ToolResult` payload to carry the recorded answer string
//! verbatim and the per-turn TOML file to record both `tool_call` and
//! `tool_result` entries naming `user.clarify` with a paired call id, so the
//! next turn's history shows the model the answer it requested.

use std::sync::Arc;

use futures::StreamExt;

use ailly::content::Conversation;
use ailly::engine::{
    Generator, HashMapRegistry, Noop, Settings, StopReason, ToolRegistry, TurnEvent,
};
use ailly::knowledge::clarify::{ClarifyTool, KnowledgeBase, MapKnowledgeBase};
use ailly::knowledge::skills::NullSkillRepository;
use ailly::mem_fs;

use rig::tool::ToolDyn;

#[tokio::test]
async fn clarify_tool_returns_recorded_answer_through_generator_and_file() {
    let question = "What database backs the inbox queue?";
    let answer = "Postgres";

    let fs = mem_fs! {
        "root": {
            "01.toml": "prompt = 'USE user.clarify WITH {\"question\":\"What database backs the inbox queue?\"}'\ntools = [\"user.clarify\"]\n",
        },
    };
    let conversation = Conversation::load(fs.join("root").unwrap(), &NullSkillRepository)
        .await
        .expect("load conversation");

    let kb: Arc<dyn KnowledgeBase> = Arc::new(MapKnowledgeBase::new([(question, answer)]));
    let clarify: Arc<dyn ToolDyn> = Arc::new(ClarifyTool::new(kb));
    let mut registry = HashMapRegistry::default();
    registry.insert(ClarifyTool::NAME, clarify);
    let registry: Arc<dyn ToolRegistry> = Arc::new(registry);

    let engine = Arc::new(Noop::default());
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
        tool_call_idx < tool_result_idx,
        "ToolCall must arrive before ToolResult, got idx {tool_call_idx} >= {tool_result_idx}"
    );
    assert!(
        tool_result_idx < finished_idx,
        "tool round-trip must complete before Finished"
    );

    let TurnEvent::ToolCall { call, .. } = &events[tool_call_idx] else {
        unreachable!()
    };
    assert_eq!(
        call.function.name,
        ClarifyTool::NAME,
        "engine dispatches against the user.clarify tool name"
    );

    let TurnEvent::ToolResult { result, .. } = &events[tool_result_idx] else {
        unreachable!()
    };
    assert_eq!(
        result.id, call.id,
        "tool result id pairs with the originating tool call id"
    );

    let result_text = serde_json::to_string(&result.content).expect("serialize tool result");
    assert!(
        result_text.contains(answer),
        "ToolResult should carry the recorded answer string verbatim; got: {result_text}"
    );

    let TurnEvent::Finished { stop_reason, .. } = &events[finished_idx] else {
        unreachable!()
    };
    assert!(
        matches!(stop_reason, StopReason::EndTurn),
        "single clarify round-trip should finish normally inside the default tool-turn budget"
    );

    let written = fs
        .join("root/01.toml")
        .unwrap()
        .read_to_string()
        .expect("read written turn file");

    let tool_call_pos = written
        .find(r#"role = "tool_call""#)
        .expect("tool_call entry written to turn file");
    let tool_result_pos = written
        .find(r#"role = "tool_result""#)
        .expect("tool_result entry written to turn file");

    assert!(
        tool_call_pos < tool_result_pos,
        "tool call precedes tool result in the persisted turn file: {written}"
    );
    assert!(
        written.contains(&format!(r#"name = "{}""#, ClarifyTool::NAME)),
        "tool_call entry should record the user.clarify tool name; got: {written}"
    );
    assert!(
        written.contains(answer),
        "persisted tool_result entry should carry the recorded answer string; got: {written}"
    );
}
