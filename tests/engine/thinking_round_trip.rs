//! Feature test for the engine reasoning slice.
//!
//! User story (narrative):
//!
//! A developer runs an extended-thinking model on a turn. The engine emits
//! reasoning blocks (text with provider signature, encrypted payload,
//! redacted payload, summary) and reasoning deltas during streaming. The
//! developer expects:
//!
//! 1. The recorded turn file persists every reasoning entry verbatim,
//!    including the provider signature and opaque payloads, so the next
//!    request body can replay them and pass signature verification.
//! 2. A subsequent turn loading the conversation history sees those
//!    reasoning blocks replayed inside the predecessor's assistant
//!    `Message.content`, in insertion order, ahead of the assistant text.
//!    Anthropic's content-array signature check and OpenAI's "reasoning
//!    before tool_call" ordering both require this shape.
//!
//! This test drives the full pipeline end to end: a custom `Engine` impl
//! emits the reasoning event stream, the `Generator` records it through
//! the in-memory `Conversation`, the recorder writes the turn file, and a
//! reload from disk reconstructs the rig `Message` shape for turn 02.

use std::sync::Arc;

use ailly::knowledge::base::EmptyKnowledgeBase;
use ailly::project::ConversationRoot;
use async_stream::stream;
use futures::StreamExt;
use rig::message::{AssistantContent, Message, Reasoning, ReasoningContent, UserContent};
use rig::tool::ToolDyn;

use ailly::content::Conversation;
use ailly::engine::{
    Engine, EngineEvent, EngineInput, EngineName, EngineResponse, EngineStream, Generator, ModelId,
    Settings, StopReason, TurnEvent,
};
use ailly::mem_fs;

/// Test engine that scripts a reasoning-bearing first turn and a trivial
/// second turn. The Generator selects the script by the request label,
/// which is the turn's path. Only the engine surface is exercised here;
/// no rig provider is involved.
struct ThinkingEngine;

impl Engine for ThinkingEngine {
    fn name(&self) -> &'static str {
        "thinking_test"
    }

    fn stream(
        &self,
        _input: EngineInput,
        _settings: &Settings,
        _tools: &[Arc<dyn ToolDyn>],
        request_label: &str,
    ) -> anyhow::Result<EngineStream> {
        let is_first = request_label.ends_with("01.toml");
        let s = stream! {
            if is_first {
                yield EngineEvent::ReasoningDelta {
                    id: Some("r1".to_string()),
                    text: "let me ".to_string(),
                };
                yield EngineEvent::ReasoningDelta {
                    id: Some("r1".to_string()),
                    text: "think".to_string(),
                };
                let mut r2 = Reasoning::new_with_signature(
                    "deeper",
                    Some("sig-xyz".to_string()),
                )
                .with_id("r2".to_string());
                r2.content
                    .push(ReasoningContent::Encrypted("enc".to_string()));
                r2.content.push(ReasoningContent::Redacted {
                    data: "red".to_string(),
                });
                r2.content
                    .push(ReasoningContent::Summary("sum".to_string()));
                yield EngineEvent::Reasoning(r2);
                yield EngineEvent::Text("the answer is 42.".to_string());
                yield EngineEvent::Final(EngineResponse::new(
                    "the answer is 42.".to_string(),
                    EngineName::from("thinking_test"),
                    ModelId::from("thinking-test"),
                    StopReason::EndTurn,
                    None,
                ));
            } else {
                yield EngineEvent::Text("ack.".to_string());
                yield EngineEvent::Final(EngineResponse::new(
                    "ack.".to_string(),
                    EngineName::from("thinking_test"),
                    ModelId::from("thinking-test"),
                    StopReason::EndTurn,
                    None,
                ));
            }
        };
        Ok(Box::pin(s))
    }
}

#[tokio::test]
async fn engine_records_reasoning_to_disk_and_replays_in_next_turn_history() {
    let fs = mem_fs! {
        "root": {
            ".ailly.toml": r#"system = "you are deliberate""#,
            "01.toml": r#"prompt = "first turn""#,
            "02.toml": r#"prompt = "second turn""#,
        },
    };
    let root = fs.join("root").unwrap();
    let knowledge_base = EmptyKnowledgeBase {};
    let conversation = Conversation::load(
        &ConversationRoot::try_from(root.clone()).expect("conversation root"),
        &knowledge_base,
    )
    .await
    .expect("load conversation");

    let engine = Arc::new(ThinkingEngine);
    let generator = Generator::new(conversation, engine, Settings::default());
    let events: Vec<TurnEvent> = generator.run().collect().await;

    assert!(
        events.iter().any(|e| matches!(
            e,
            TurnEvent::Finished { path, .. } if path.as_str().ends_with("01.toml")
        )),
        "expected first turn to finish, got: {events:?}"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            TurnEvent::Finished { path, .. } if path.as_str().ends_with("02.toml")
        )),
        "expected second turn to finish"
    );
    assert!(
        !events.iter().any(|e| matches!(e, TurnEvent::Failed { .. })),
        "no Failed events in happy path: {events:?}"
    );

    let first_text: String = events
        .iter()
        .filter_map(|e| match e {
            TurnEvent::Delta { path, text } if path.as_str().ends_with("01.toml") => {
                Some(text.clone())
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        first_text, "the answer is 42.",
        "TurnEvent::Delta is text-only in this slice; reasoning is recorded but not surfaced through the live event stream"
    );

    // Outcome 1: turn 01's TOML file persists every reasoning entry
    // verbatim, including provider signature and opaque payloads.
    let written = fs
        .join("root/01.toml")
        .unwrap()
        .read_to_string()
        .expect("read written turn file");

    let reasoning_count = written.matches(r#"role = "reasoning""#).count();
    assert_eq!(
        reasoning_count, 2,
        "expected exactly two reasoning entries in turn file (one merged from r1 deltas, one from the r2 whole block); got {reasoning_count} in:\n{written}"
    );
    for needle in [
        "r1",
        "r2",
        "let me think",
        "deeper",
        "sig-xyz",
        "enc",
        "red",
        "sum",
    ] {
        assert!(
            written.contains(needle),
            "expected turn file to preserve {needle:?}; got:\n{written}"
        );
    }

    let last_reasoning_pos = written
        .rfind(r#"role = "reasoning""#)
        .expect("reasoning entry present");
    let assistant_pos = written
        .find(r#"role = "assistant""#)
        .expect("assistant entry present");
    assert!(
        last_reasoning_pos < assistant_pos,
        "every reasoning entry must precede the assistant text so providers see reasoning before the answer; got reasoning at {last_reasoning_pos}, assistant at {assistant_pos}"
    );

    // Outcome 2: re-loading the conversation from disk and asking for
    // turn 02's history surfaces the reasoning blocks inside the
    // predecessor's assistant Message content, in insertion order, with
    // signatures and opaque payloads intact.
    let knowlege = EmptyKnowledgeBase {};
    let reloaded = Conversation::load(
        &ConversationRoot::try_from(root).expect("conversation root"),
        &knowlege,
    )
    .await
    .expect("reload conversation after first run");

    let history = reloaded.history_for(reloaded.turn(1));

    let assistant_msg = history
        .iter()
        .find(|m| matches!(m, Message::Assistant { .. }))
        .expect("predecessor assistant message in turn-02 history");
    let Message::Assistant { content, .. } = assistant_msg else {
        unreachable!()
    };

    let content_vec: Vec<&AssistantContent> = content.iter().collect();
    let reasonings: Vec<&Reasoning> = content_vec
        .iter()
        .filter_map(|c| match c {
            AssistantContent::Reasoning(r) => Some(r),
            _ => None,
        })
        .collect();
    assert_eq!(
        reasonings.len(),
        2,
        "predecessor assistant content carries two reasoning blocks; got: {content_vec:?}"
    );

    let r1 = reasonings[0];
    assert_eq!(
        r1.id.as_deref(),
        Some("r1"),
        "first reasoning block carries the delta id"
    );
    assert_eq!(
        r1.content.len(),
        1,
        "consecutive deltas with matching id merge into a single Text block"
    );
    let ReasoningContent::Text { text, signature } = &r1.content[0] else {
        panic!(
            "r1 block must be Text after delta merge, got {:?}",
            r1.content[0]
        );
    };
    assert_eq!(text, "let me think");
    assert_eq!(
        *signature, None,
        "merged delta-derived block carries no signature"
    );

    let r2 = reasonings[1];
    assert_eq!(
        r2.id.as_deref(),
        Some("r2"),
        "second reasoning block carries the whole-block id"
    );
    assert_eq!(
        r2.content.len(),
        4,
        "r2 carries four block variants verbatim"
    );
    assert!(
        matches!(
            &r2.content[0],
            ReasoningContent::Text { text, signature: Some(s) } if text == "deeper" && s == "sig-xyz"
        ),
        "r2 block 0 must be signed text: {:?}",
        r2.content[0]
    );
    assert!(
        matches!(&r2.content[1], ReasoningContent::Encrypted(s) if s == "enc"),
        "r2 block 1 must be encrypted: {:?}",
        r2.content[1]
    );
    assert!(
        matches!(&r2.content[2], ReasoningContent::Redacted { data } if data == "red"),
        "r2 block 2 must be redacted: {:?}",
        r2.content[2]
    );
    assert!(
        matches!(&r2.content[3], ReasoningContent::Summary(s) if s == "sum"),
        "r2 block 3 must be summary: {:?}",
        r2.content[3]
    );

    let reasoning_positions: Vec<usize> = content_vec
        .iter()
        .enumerate()
        .filter_map(|(i, c)| match c {
            AssistantContent::Reasoning(_) => Some(i),
            _ => None,
        })
        .collect();
    let text_pos = content_vec
        .iter()
        .position(|c| matches!(c, AssistantContent::Text(_)))
        .expect("assistant text block in content");
    for r_pos in reasoning_positions {
        assert!(
            r_pos < text_pos,
            "reasoning block at index {r_pos} must precede assistant text at index {text_pos}"
        );
    }

    assert!(
        history.iter().any(|m| matches!(
            m,
            Message::User { content }
                if matches!(content.first(), UserContent::Text(t) if t.text == "first turn")
        )),
        "first-turn user prompt remains in turn-02 history"
    );
}
