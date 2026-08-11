//! Shared conformance suite for the harness adapters.
//!
//! Every adapter maps its native JSONL records onto the same normalized event
//! model, so the shape that comes out — one session header, user/assistant
//! turns, tool calls as their own events, honest `SourceValue` provenance —
//! is a cross-adapter contract, not a per-adapter accident. Each adapter's
//! test module supplies a minimal native transcript plus the normalized facts
//! it expects back through [`Conformance`], then instantiates the identically
//! named tests with [`conformance_tests!`]. Genuine per-harness quirks stay
//! as plain tests next to each adapter.

use super::*;

/// One adapter's minimal native transcript plus the normalized facts the
/// shared tests expect back out of it. The transcript must contain: a
/// session-seeding record, one user turn, one assistant turn, one tool call,
/// and — when the harness records them — a tool result linked to that call
/// and a conversation-tree parent pointing at the user turn.
pub(crate) struct Conformance {
    pub harness: Harness,
    pub record: RecordParser,
    pub transcript: Vec<Value>,
    pub native_session_id: &'static str,
    pub project: &'static str,
    pub tool_name: &'static str,
    pub tool_path: &'static str,
    /// `None` when the harness records no tool-result linkage (Claude ties
    /// results into its conversation tree instead); the shared test then
    /// asserts the adapter manufactures none.
    pub tool_result_call_id: Option<&'static str>,
    /// `None` when the harness records no conversation-tree parent (Codex is
    /// a flat response stream); the shared test then asserts none appear.
    pub tree_parent_id: Option<&'static str>,
    /// Expected usage on the assistant turn; `None` when the harness's
    /// transcript supplies none, in which case usage must resolve `Absent`,
    /// never a fabricated zeroed `TokenUsage`.
    pub assistant_usage: Option<TokenUsage>,
}

impl Conformance {
    fn parse(&self) -> ParsedSession {
        parse_records(self.record, self.harness, &self.transcript)
    }
}

/// Runs `record` over a slice of native values as if they were consecutive
/// lines of one fixture file. Also handy for adapters' quirk tests.
pub(crate) fn parse_records(
    record: RecordParser,
    harness: Harness,
    values: &[Value],
) -> ParsedSession {
    let mut parsed = ParsedSession::default();
    for (index, value) in values.iter().enumerate() {
        let source = provenance(harness, "fixture.jsonl", index + 1);
        record(value, &mut parsed, "fixture.jsonl", source);
    }
    parsed
}

fn events_of_kind(parsed: &ParsedSession, kind: EventKind) -> Vec<&Event> {
    parsed
        .events
        .iter()
        .filter(|event| event.kind == kind)
        .collect()
}

fn only_event(parsed: &ParsedSession, kind: EventKind) -> &Event {
    let events = events_of_kind(parsed, kind);
    assert_eq!(events.len(), 1, "expected exactly one {kind:?} event");
    events[0]
}

fn recorded_turn(event: &Event) -> &Turn {
    let SourceValue::Recorded(turn) = &event.turn else {
        panic!("expected a recorded turn, got {:?}", event.turn);
    };
    turn
}

fn edges_of_kind(parsed: &ParsedSession, kind: RelationshipKind) -> Vec<&Relationship> {
    parsed
        .relationships
        .iter()
        .filter(|relationship| relationship.kind == kind)
        .collect()
}

pub(crate) fn assert_seeds_the_session(conformance: &Conformance) {
    let parsed = conformance.parse();
    let session = parsed.session.expect("session header seeded");
    assert_eq!(session.harness, conformance.harness);
    assert_eq!(
        session.native_id,
        SourceValue::Recorded(conformance.native_session_id.to_string())
    );
    assert_eq!(
        session.project,
        SourceValue::Recorded(conformance.project.to_string())
    );
}

pub(crate) fn assert_imports_a_user_turn(conformance: &Conformance) {
    let parsed = conformance.parse();
    let turn = recorded_turn(only_event(&parsed, EventKind::UserTurn));
    assert_eq!(turn.role, "user");
    assert!(
        matches!(turn.text, SourceValue::Recorded(_)),
        "user text should be recorded, got {:?}",
        turn.text
    );
}

pub(crate) fn assert_imports_an_assistant_turn(conformance: &Conformance) {
    let parsed = conformance.parse();
    let turn = recorded_turn(only_event(&parsed, EventKind::AssistantTurn));
    assert_eq!(turn.role, "assistant");
    assert!(
        matches!(turn.text, SourceValue::Recorded(_)),
        "assistant text should be recorded, got {:?}",
        turn.text
    );
}

pub(crate) fn assert_tool_call_is_its_own_event(conformance: &Conformance) {
    let parsed = conformance.parse();
    // The assistant turn stays a turn; the tool call is a separate event.
    only_event(&parsed, EventKind::AssistantTurn);
    let call_event = only_event(&parsed, EventKind::ToolCall);
    let SourceValue::Recorded(call) = &call_event.tool_call else {
        panic!(
            "expected a recorded tool call, got {:?}",
            call_event.tool_call
        );
    };
    assert_eq!(call.name, conformance.tool_name);
    assert!(
        matches!(call.input, SourceValue::Recorded(_)),
        "tool input should be recorded, got {:?}",
        call.input
    );
    assert_eq!(
        call.path,
        SourceValue::Recorded(conformance.tool_path.to_string())
    );
}

pub(crate) fn assert_token_usage_is_honest(conformance: &Conformance) {
    let parsed = conformance.parse();
    let assistant = only_event(&parsed, EventKind::AssistantTurn);
    match &conformance.assistant_usage {
        Some(expected) => assert_eq!(
            assistant.token_usage,
            SourceValue::Recorded(expected.clone())
        ),
        None => assert_eq!(assistant.token_usage, SourceValue::Absent),
    }
    // No transcript supplies user-turn usage; it must stay Absent, never a
    // fabricated zeroed TokenUsage.
    let user = only_event(&parsed, EventKind::UserTurn);
    assert_eq!(user.token_usage, SourceValue::Absent);
}

pub(crate) fn assert_tool_result_linkage(conformance: &Conformance) {
    let parsed = conformance.parse();
    let edges = edges_of_kind(&parsed, RelationshipKind::ToolCallResult);
    match conformance.tool_result_call_id {
        Some(call_id) => {
            let result = only_event(&parsed, EventKind::ToolResult);
            assert_eq!(edges.len(), 1);
            assert_eq!(edges[0].from_event_id, result.id);
            assert_eq!(edges[0].to_native_id, call_id);
        }
        None => {
            assert!(
                edges.is_empty(),
                "this harness records no tool-result linkage; none may be manufactured"
            );
            assert!(events_of_kind(&parsed, EventKind::ToolResult).is_empty());
        }
    }
}

pub(crate) fn assert_tree_parent_linkage(conformance: &Conformance) {
    let parsed = conformance.parse();
    let edges = edges_of_kind(&parsed, RelationshipKind::ConversationTreeParent);
    match conformance.tree_parent_id {
        Some(parent_id) => {
            let assistant = only_event(&parsed, EventKind::AssistantTurn);
            assert_eq!(edges.len(), 1);
            assert_eq!(edges[0].from_event_id, assistant.id);
            assert_eq!(edges[0].to_native_id, parent_id);
        }
        None => assert!(
            edges.is_empty(),
            "this harness records no conversation-tree parent; none may be manufactured"
        ),
    }
}

pub(crate) fn assert_no_subagent_spawn(conformance: &Conformance) {
    let parsed = conformance.parse();
    // SubagentSpawn is reserved for explicit delegation evidence, which no
    // adapter's transcript records; none may manufacture the variant.
    assert!(edges_of_kind(&parsed, RelationshipKind::SubagentSpawn).is_empty());
}

pub(crate) fn assert_event_provenance(conformance: &Conformance) {
    let parsed = conformance.parse();
    let session_id = parsed.session.as_ref().expect("session seeded").id.clone();
    assert!(!parsed.events.is_empty());
    for event in &parsed.events {
        assert_eq!(event.session_id, session_id);
        assert_eq!(event.source.harness, conformance.harness);
        assert!(
            (1..=conformance.transcript.len()).contains(&event.source.line),
            "event line {} must point at a transcript line",
            event.source.line
        );
    }
}

/// Instantiates the shared, identically named conformance tests for one
/// adapter. `$conformance` is a zero-argument function returning that
/// adapter's [`Conformance`] fixture.
macro_rules! conformance_tests {
    ($conformance:path) => {
        #[test]
        fn seeds_the_session_with_recorded_identity_and_project() {
            crate::loader::conformance::assert_seeds_the_session(&$conformance());
        }

        #[test]
        fn imports_a_user_turn_with_recorded_role_and_text() {
            crate::loader::conformance::assert_imports_a_user_turn(&$conformance());
        }

        #[test]
        fn imports_an_assistant_turn_with_recorded_role_and_text() {
            crate::loader::conformance::assert_imports_an_assistant_turn(&$conformance());
        }

        #[test]
        fn a_tool_call_surfaces_as_its_own_event_with_recorded_name_input_and_path() {
            crate::loader::conformance::assert_tool_call_is_its_own_event(&$conformance());
        }

        #[test]
        fn token_usage_reflects_only_what_the_harness_recorded_never_zeroed() {
            crate::loader::conformance::assert_token_usage_is_honest(&$conformance());
        }

        #[test]
        fn a_tool_result_links_to_its_recorded_call_id_or_is_never_manufactured() {
            crate::loader::conformance::assert_tool_result_linkage(&$conformance());
        }

        #[test]
        fn conversation_tree_parent_edges_come_only_from_recorded_parent_ids() {
            crate::loader::conformance::assert_tree_parent_linkage(&$conformance());
        }

        #[test]
        fn no_adapter_manufactures_a_subagent_spawn_edge() {
            crate::loader::conformance::assert_no_subagent_spawn(&$conformance());
        }

        #[test]
        fn every_event_carries_the_session_id_and_harness_provenance() {
            crate::loader::conformance::assert_event_provenance(&$conformance());
        }
    };
}
pub(crate) use conformance_tests;
