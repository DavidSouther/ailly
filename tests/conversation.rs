//! Feature test for the Conversation domain object.
//!
//! End-to-end user story: an operator runs `ailly run` against a conversation
//! file that `ailly assemble` produced. The file already contains a meta
//! header, a system message, a user message, and a blank assistant slot.
//! The operator parses the file, locates the blank assistant slot, fills it
//! with the model's response and inline trace, and serializes the result
//! back to YAML. The other messages survive the round-trip unchanged.

use ailly_two::content::conversation::Content;
use ailly_two::content::conversation::ContentBlock;
use ailly_two::content::conversation::Conversation;
use ailly_two::content::conversation::Message;
use ailly_two::content::conversation::ModelId;
use ailly_two::content::conversation::Role;
use ailly_two::content::conversation::SpanId;
use ailly_two::content::conversation::TokenCounts;
use ailly_two::content::conversation::Trace;

const ASSEMBLED_YAML: &str = "\
---
model: claude-opus-4-7
assembly: claim-handler
binding:
  domain: prose-bio
---
role: system
content: You classify insurance claims.
cache: true
---
role: user
content:
  - type: text
    text: \"Claim 42: a tree fell on my fence.\"
---
role: assistant
";

#[test]
fn run_fills_blank_assistant_slot_and_round_trips() {
    let mut conversation = Conversation::from_yaml_str(ASSEMBLED_YAML)
        .expect("assembled YAML parses into a Conversation");

    assert_eq!(conversation.meta.model.as_ref(), "claude-opus-4-7");
    assert_eq!(conversation.meta.assembly.as_deref(), Some("claim-handler"));
    assert_eq!(conversation.session.len(), 3);
    assert!(matches!(conversation.session[0].role, Role::System));
    assert!(matches!(conversation.session[1].role, Role::User));
    assert!(matches!(conversation.session[2].role, Role::Assistant));
    assert!(conversation.session[2].body.is_none());

    let blank_idx = conversation
        .next_blank_assistant()
        .expect("assembled file has a blank assistant slot");
    assert_eq!(blank_idx, 2);

    let prefix = conversation.messages_up_to(blank_idx);
    assert_eq!(prefix.len(), 2);
    assert!(matches!(prefix[0].role, Role::System));
    assert!(matches!(prefix[1].role, Role::User));

    let response = Content::from(String::from("auto-approve"));
    let trace = Trace {
        span_id: SpanId::from("span-001"),
        model: ModelId::from("claude-opus-4-7"),
        tokens: TokenCounts {
            input: 128,
            output: 4,
            cache_hit: Some(96),
            cache_write: None,
        },
        latency_ms: 412,
        events: Vec::new(),
    };

    conversation
        .fill_blank_assistant(blank_idx, response, trace)
        .expect("blank assistant slot accepts content and trace");

    assert!(conversation.next_blank_assistant().is_none());

    let filled = &conversation.session[blank_idx];
    match filled.body.as_ref().expect("assistant now has content") {
        Content::Text(text) => assert_eq!(text, "auto-approve"),
        Content::Blocks(_) => panic!("expected text content, got structured blocks"),
    }
    let filled_trace = filled.trace.as_ref().expect("assistant now has trace");
    assert_eq!(filled_trace.span_id.as_ref(), "span-001");
    assert_eq!(filled_trace.tokens.input, 128);
    assert_eq!(filled_trace.tokens.output, 4);
    assert_eq!(filled_trace.tokens.cache_hit, Some(96));
    assert_eq!(filled_trace.latency_ms, 412);

    let system = &conversation.session[0];
    assert!(system.cache);
    match system.body.as_ref().expect("system content preserved") {
        Content::Text(text) => assert_eq!(text, "You classify insurance claims."),
        Content::Blocks(_) => panic!("system content was a string"),
    }
    let user = &conversation.session[1];
    match user.body.as_ref().expect("user content preserved") {
        Content::Blocks(blocks) => {
            assert_eq!(blocks.len(), 1);
            match &blocks[0] {
                ContentBlock::Text { text } => {
                    assert_eq!(text, "Claim 42: a tree fell on my fence.");
                }
                other => panic!("expected text block, got {other:?}"),
            }
        }
        Content::Text(_) => panic!("user content was a block list"),
    }

    let emitted = conversation
        .to_yaml_string()
        .expect("filled conversation serializes back to YAML");

    let reparsed =
        Conversation::from_yaml_str(&emitted).expect("serialized output re-parses cleanly");

    assert_eq!(reparsed.meta.model, conversation.meta.model);
    assert_eq!(reparsed.meta.assembly, conversation.meta.assembly);
    assert_eq!(reparsed.session.len(), conversation.session.len());
    assert!(reparsed.next_blank_assistant().is_none());

    let reparsed_assistant = &reparsed.session[blank_idx];
    match reparsed_assistant
        .body
        .as_ref()
        .expect("round-trip preserves filled content")
    {
        Content::Text(text) => assert_eq!(text, "auto-approve"),
        Content::Blocks(_) => panic!("filled content was Text before serialize"),
    }
    let reparsed_trace = reparsed_assistant
        .trace
        .as_ref()
        .expect("round-trip preserves trace");
    assert_eq!(reparsed_trace.span_id.as_ref(), "span-001");
    assert_eq!(reparsed_trace.tokens.input, 128);
    assert_eq!(reparsed_trace.tokens.output, 4);
    assert_eq!(reparsed_trace.tokens.cache_hit, Some(96));
    assert_eq!(reparsed_trace.latency_ms, 412);

    assert_message_eq(&reparsed.session[0], &conversation.session[0]);
    assert_message_eq(&reparsed.session[1], &conversation.session[1]);
}

fn assert_message_eq(left: &Message, right: &Message) {
    assert_eq!(
        std::mem::discriminant(&left.role),
        std::mem::discriminant(&right.role),
    );
    assert_eq!(left.cache, right.cache);
    match (left.body.as_ref(), right.body.as_ref()) {
        (None, None) => {}
        (Some(Content::Text(a)), Some(Content::Text(b))) => assert_eq!(a, b),
        (Some(Content::Blocks(a)), Some(Content::Blocks(b))) => {
            assert_eq!(a.len(), b.len());
            for (l, r) in a.iter().zip(b.iter()) {
                assert_eq!(
                    std::mem::discriminant(l),
                    std::mem::discriminant(r),
                    "content block kind diverged on round-trip",
                );
            }
        }
        _ => panic!("content kind changed across round-trip"),
    }
}
