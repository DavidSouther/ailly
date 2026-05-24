//! Feature test for the `EngineProvider` port, the `NoopEngine` adapter, and
//! the `Conversation::run` aggregate operation.
//!
//! End-to-end user story: a caller has an assembled, multi-turn conversation
//! with two blank assistant slots — a clarifying turn and a final decision —
//! and a deterministic `NoopEngine` scripted with the two assistant replies
//! the run is meant to produce. The caller hands the engine to the
//! `Conversation` aggregate root with a single `run(&engine).await` call, and
//! the aggregate atomically transitions the conversation from "partially
//! filled" to "fully filled": every blank assistant slot now carries the
//! scripted reply and an inline trace, no blank assistant slot remains, and
//! the filled conversation round-trips through YAML unchanged.
//!
//! The walk over blank slots is the aggregate's concern, not the caller's.
//! The engine sees one `CompletionRequest` per blank, in order, against the
//! conversation's cumulative prefix.

use ailly_two::content::conversation::Content;
use ailly_two::content::conversation::Conversation;
use ailly_two::content::conversation::ModelId;
use ailly_two::engine::engine::NoopEngine;

const ASSEMBLED_YAML: &str = "\
---
model: claude-opus-4-7
assembly: claim-handler
---
role: system
content: |
  You are a claims handler. For each claim, decide whether to auto-approve,
  escalate to human review, or reject. If the claim is missing either the
  date of loss or the policy number, ask one clarifying question that
  requests exactly those fields before deciding.
cache: true
---
role: user
content: \"A tree fell on my fence during yesterday's storm. Estimated damage: $1,200.\"
---
role: assistant
---
role: user
content: \"Date of loss: 2026-05-23. Policy number: ABC-123.\"
---
role: assistant
";

const CLARIFY_REPLY: &str = "Could you share the date of loss and the policy number on file?";
const DECISION_REPLY: &str =
    "auto-approve: claim is within policy threshold and required fields are present.";

#[tokio::test]
async fn conversation_run_fills_every_blank_assistant_via_scripted_engine() {
    // Arrange
    let mut conversation = Conversation::from_yaml_str(ASSEMBLED_YAML)
        .expect("assembled YAML parses into a Conversation");
    assert_eq!(conversation.session.len(), 5);
    assert!(conversation.next_blank_assistant().is_some());
    let engine = NoopEngine::from_replies([CLARIFY_REPLY, DECISION_REPLY]);

    // Act
    conversation
        .run(&engine)
        .await
        .expect("aggregate fills every blank assistant slot");

    // Assert
    assert!(conversation.next_blank_assistant().is_none());

    let clarify = &conversation.session[2];
    match clarify.body.as_ref().expect("first blank now filled") {
        Content::Text(text) => assert_eq!(text, CLARIFY_REPLY),
        Content::Blocks(_) => panic!("from_replies should produce Content::Text"),
    }
    let clarify_trace = clarify.trace.as_ref().expect("first assistant has trace");
    assert_eq!(clarify_trace.model, ModelId::from("noop"));
    assert_eq!(clarify_trace.tokens.input, 0);
    assert_eq!(clarify_trace.tokens.output, 0);
    assert_eq!(clarify_trace.latency_ms, 0);
    assert!(clarify_trace.events.is_empty());

    let decision = &conversation.session[4];
    match decision.body.as_ref().expect("second blank now filled") {
        Content::Text(text) => assert_eq!(text, DECISION_REPLY),
        Content::Blocks(_) => panic!("from_replies should produce Content::Text"),
    }
    let decision_trace = decision.trace.as_ref().expect("second assistant has trace");
    assert_eq!(decision_trace.model, ModelId::from("noop"));
    assert_ne!(
        decision_trace.span_id, clarify_trace.span_id,
        "each completion gets its own span id"
    );

    let emitted = conversation
        .to_yaml_string()
        .expect("filled conversation serializes back to YAML");
    let reparsed =
        Conversation::from_yaml_str(&emitted).expect("serialized output re-parses cleanly");
    assert!(reparsed.next_blank_assistant().is_none());
    assert_eq!(reparsed.session.len(), conversation.session.len());
}
