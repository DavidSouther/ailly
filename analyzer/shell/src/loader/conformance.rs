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
/// session-seeding record, one user turn, one assistant turn, one file-shaped
/// tool call, one shell-shaped tool call, and — when the harness records them
/// — a tool result linked to the first call and a conversation-tree parent
/// pointing at the user turn.
pub(crate) struct Conformance {
    pub harness: Harness,
    pub record: RecordParser,
    pub transcript: Vec<Value>,
    pub native_session_id: &'static str,
    pub project: &'static str,
    pub tool_name: &'static str,
    pub tool_path: &'static str,
    /// The name of the transcript's shell-shaped call, whose command and
    /// working directory the two assertions below are about. Kept separate
    /// from `tool_name` because no harness's file-reading tool records a
    /// command, so one call cannot honestly carry both facts.
    pub shell_tool_name: &'static str,
    pub tool_command: &'static str,
    /// `Some` when the harness records a working directory for that call —
    /// on the call itself (Codex's `workdir`) or on the record enclosing it
    /// (Claude's `cwd`). `None` when it genuinely records none, so the shared
    /// test asserts `Absent` rather than excusing the adapter.
    pub tool_cwd: Option<&'static str>,
    /// `None` when the harness records no tool-result linkage; the shared test
    /// then asserts the adapter manufactures none.
    pub tool_result_call_id: Option<&'static str>,
    /// What the transcript's tool result returned. `None` when the fixture
    /// carries no result record at all, in which case the shared test asserts
    /// the adapter invents no result event.
    pub tool_result_output: Option<&'static str>,
    /// `None` when the harness records no conversation-tree parent (Codex is
    /// a flat response stream); the shared test then asserts none appear.
    pub tree_parent_id: Option<&'static str>,
    /// Expected usage on the assistant turn; `None` when the harness's
    /// transcript supplies none, in which case usage must resolve `Absent`,
    /// never a fabricated zeroed `TokenUsage`.
    pub assistant_usage: Option<TokenUsage>,
    /// The delegation the transcript records. `None` when it records none, in
    /// which case the shared test asserts the adapter manufactures neither a
    /// spawn event nor a spawn edge.
    pub subagent_spawn: Option<ConformanceSpawn>,
}

/// One fixture's recorded delegation, in normalized terms. Every `Option` here
/// is a fact about the *transcript*, not an allowance for the adapter: `None`
/// means the harness recorded nothing, and the shared test then holds the
/// adapter to `Absent`.
pub(crate) struct ConformanceSpawn {
    pub agent_type: &'static str,
    pub prompt: &'static str,
    pub outcome: Option<&'static str>,
    pub duration_ms: Option<u64>,
    /// The child the source named. The spawn edge is emitted if and only if
    /// this is `Some`.
    pub child_native_id: Option<&'static str>,
    /// The child's own recorded token total, which must live on the subagent
    /// payload and never on the spawn event's own `token_usage`.
    pub child_token_total: Option<u64>,
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

/// The one recorded tool call the fixture named `name`. Fixtures carry more
/// than one call, so the shared assertions address each by its harness name
/// rather than by position.
fn tool_call_named<'a>(parsed: &'a ParsedSession, name: &str) -> &'a ToolCall {
    let calls: Vec<&ToolCall> = events_of_kind(parsed, EventKind::ToolCall)
        .into_iter()
        .filter_map(|event| match &event.tool_call {
            SourceValue::Recorded(call) if call.name == name => Some(call),
            _ => None,
        })
        .collect();
    assert_eq!(
        calls.len(),
        1,
        "expected exactly one recorded {name} tool call"
    );
    calls[0]
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

/// Addresses the conversational user turn by its recorded text, because a
/// harness that delivers tool results on user-typed records (Claude) produces
/// further user turns that carry a result rather than anything the user said.
pub(crate) fn assert_imports_a_user_turn(conformance: &Conformance) {
    let parsed = conformance.parse();
    let spoken: Vec<&Turn> = events_of_kind(&parsed, EventKind::UserTurn)
        .into_iter()
        .map(recorded_turn)
        .filter(|turn| matches!(turn.text, SourceValue::Recorded(_)))
        .collect();
    assert_eq!(
        spoken.len(),
        1,
        "expected exactly one user turn with recorded text"
    );
    assert_eq!(spoken[0].role, "user");
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
    // The assistant turn stays a turn; each tool call is a separate event.
    only_event(&parsed, EventKind::AssistantTurn);
    let call = tool_call_named(&parsed, conformance.tool_name);
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

/// Every harness nests the command inside the call's input payload, so a
/// top-level lookup reads `Absent` for all three at once — which is how this
/// field stayed unrecorded across every adapter without a test failing.
pub(crate) fn assert_tool_call_command_is_recorded(conformance: &Conformance) {
    let parsed = conformance.parse();
    let call = tool_call_named(&parsed, conformance.shell_tool_name);
    assert_eq!(
        call.command,
        SourceValue::Recorded(conformance.tool_command.to_string())
    );
}

pub(crate) fn assert_tool_call_cwd_matches_expectation(conformance: &Conformance) {
    let parsed = conformance.parse();
    let call = tool_call_named(&parsed, conformance.shell_tool_name);
    match conformance.tool_cwd {
        Some(expected) => assert_eq!(call.cwd, SourceValue::Recorded(expected.to_string())),
        None => assert_eq!(
            call.cwd,
            SourceValue::Absent,
            "this harness records no per-call working directory; none may be borrowed from the session"
        ),
    }
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
    for user in events_of_kind(&parsed, EventKind::UserTurn) {
        assert_eq!(user.token_usage, SourceValue::Absent);
    }
}

/// A result event exists to carry what the call returned; an adapter that
/// emits the event but drops the payload passes every other assertion here.
pub(crate) fn assert_tool_result_output_is_recorded(conformance: &Conformance) {
    let parsed = conformance.parse();
    let Some(expected) = conformance.tool_result_output else {
        assert!(
            events_of_kind(&parsed, EventKind::ToolResult).is_empty(),
            "this fixture records no result; none may be manufactured"
        );
        return;
    };
    let result = match conformance.tool_result_call_id {
        Some(call_id) => tool_result_answering(&parsed, call_id),
        None => only_event(&parsed, EventKind::ToolResult),
    };
    let SourceValue::Recorded(recorded) = &result.tool_result else {
        panic!(
            "expected a recorded tool result, got {:?}",
            result.tool_result
        );
    };
    assert_eq!(
        recorded.output,
        SourceValue::Recorded(expected.to_string()),
        "the result event must carry the output its transcript recorded"
    );
    match conformance.tool_result_call_id {
        Some(call_id) => assert_eq!(recorded.call_id, SourceValue::Recorded(call_id.to_string())),
        None => assert_eq!(recorded.call_id, SourceValue::Absent),
    }
}

/// The one result event answering `call_id`. Fixtures that record a delegation
/// carry more than one result, so the shared assertions address each by the
/// call it answers rather than by being the only one.
fn tool_result_answering<'a>(parsed: &'a ParsedSession, call_id: &str) -> &'a Event {
    let answers: Vec<&Event> = events_of_kind(parsed, EventKind::ToolResult)
        .into_iter()
        .filter(|event| match &event.tool_result {
            SourceValue::Recorded(result) => {
                result.call_id == SourceValue::Recorded(call_id.to_string())
            }
            _ => false,
        })
        .collect();
    assert_eq!(
        answers.len(),
        1,
        "expected exactly one result answering {call_id}"
    );
    answers[0]
}

pub(crate) fn assert_tool_result_linkage(conformance: &Conformance) {
    let parsed = conformance.parse();
    let edges = edges_of_kind(&parsed, RelationshipKind::ToolCallResult);
    match conformance.tool_result_call_id {
        Some(call_id) => {
            let result = tool_result_answering(&parsed, call_id);
            let linking: Vec<&Relationship> = edges
                .iter()
                .copied()
                .filter(|edge| edge.to_native_id == call_id)
                .collect();
            assert_eq!(linking.len(), 1);
            assert_eq!(linking[0].from_event_id, result.id);
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

/// A spawn carries only what its harness recorded, and its child edge exists
/// if and only if the source named a child. Linkage is never inferred from
/// adjacency, and the child's tokens never land on the parent's event.
pub(crate) fn assert_subagent_spawn_matches_recorded_delegation(conformance: &Conformance) {
    let parsed = conformance.parse();
    let edges = edges_of_kind(&parsed, RelationshipKind::SubagentSpawn);
    let Some(expected) = &conformance.subagent_spawn else {
        assert!(
            events_of_kind(&parsed, EventKind::SubagentSpawn).is_empty(),
            "this transcript records no delegation; none may be manufactured"
        );
        assert!(edges.is_empty());
        return;
    };

    let spawn = only_event(&parsed, EventKind::SubagentSpawn);
    let SourceValue::Recorded(subagent) = &spawn.subagent else {
        panic!("expected a recorded subagent, got {:?}", spawn.subagent);
    };
    assert_eq!(
        subagent.agent_type,
        SourceValue::Recorded(expected.agent_type.to_string())
    );
    assert_eq!(
        subagent.prompt,
        SourceValue::Recorded(expected.prompt.to_string())
    );
    assert_expected(&subagent.outcome, expected.outcome.map(str::to_string));
    assert_expected(&subagent.duration_ms, expected.duration_ms);
    // Resolving a spawn to an indexed child session is the query layer's job;
    // an adapter can only ever see one file.
    assert_eq!(subagent.child_session_id, SourceValue::Absent);
    // A child's tokens on the parent's own event would silently inflate the
    // parent session's total.
    assert_eq!(spawn.token_usage, SourceValue::Absent);
    match expected.child_token_total {
        Some(total) => {
            let SourceValue::Recorded(usage) = &subagent.token_usage else {
                panic!(
                    "expected recorded subagent tokens, got {:?}",
                    subagent.token_usage
                );
            };
            assert_eq!(usage.total, SourceValue::Recorded(total));
        }
        None => assert_eq!(subagent.token_usage, SourceValue::Absent),
    }

    assert_expected(
        &subagent.native_id,
        expected.child_native_id.map(str::to_string),
    );
    match expected.child_native_id {
        Some(child) => {
            assert_eq!(edges.len(), 1, "expected exactly one spawn edge");
            assert_eq!(edges[0].from_event_id, spawn.id);
            assert_eq!(edges[0].to_native_id, child);
        }
        None => assert!(
            edges.is_empty(),
            "this transcript names no child; no spawn edge may be manufactured"
        ),
    }
}

/// Asserts a `SourceValue` matches what the transcript recorded, holding an
/// adapter to `Absent` wherever the harness wrote nothing.
fn assert_expected<T: std::fmt::Debug + PartialEq>(actual: &SourceValue<T>, expected: Option<T>) {
    match expected {
        Some(value) => assert_eq!(*actual, SourceValue::Recorded(value)),
        None => assert_eq!(*actual, SourceValue::Absent),
    }
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
        fn a_tool_call_surfaces_its_recorded_command() {
            crate::loader::conformance::assert_tool_call_command_is_recorded(&$conformance());
        }

        #[test]
        fn a_tool_calls_working_directory_matches_what_the_harness_actually_recorded() {
            crate::loader::conformance::assert_tool_call_cwd_matches_expectation(&$conformance());
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
        fn a_tool_result_carries_the_output_its_harness_recorded() {
            crate::loader::conformance::assert_tool_result_output_is_recorded(&$conformance());
        }

        #[test]
        fn conversation_tree_parent_edges_come_only_from_recorded_parent_ids() {
            crate::loader::conformance::assert_tree_parent_linkage(&$conformance());
        }

        #[test]
        fn a_subagent_spawn_carries_only_what_its_harness_recorded() {
            crate::loader::conformance::assert_subagent_spawn_matches_recorded_delegation(
                &$conformance(),
            );
        }

        #[test]
        fn every_event_carries_the_session_id_and_harness_provenance() {
            crate::loader::conformance::assert_event_provenance(&$conformance());
        }
    };
}
pub(crate) use conformance_tests;
