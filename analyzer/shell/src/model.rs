//! Harness-neutral, read-only evidence imported from session transcripts.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Harness {
    ClaudeCode,
    Codex,
    Pi,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Provenance {
    pub harness: Harness,
    pub path: String,
    /// One-based JSONL line number when the source is line-oriented.
    pub line: usize,
    /// Native record order. This is deliberately independent of timestamps.
    pub ordinal: usize,
}

/// Whether a value was found in the source document. Consumers must preserve
/// `Absent`, `Unsupported`, and `Malformed` rather than treating them as an
/// observed value.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum SourceValue<T> {
    Recorded(T),
    Absent,
    Unsupported,
    Malformed,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Session {
    pub id: String,
    pub harness: Harness,
    pub source: Provenance,
    pub native_id: SourceValue<String>,
    pub project: SourceValue<String>,
    pub parent_session: SourceValue<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    UserTurn,
    AssistantTurn,
    ToolCall,
    ToolResult,
    SubagentSpawn,
    SessionMetadata,
    ModelChange,
    ThinkingChange,
    Summary,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Turn {
    pub role: String,
    pub text: SourceValue<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ToolCall {
    pub name: String,
    pub call_id: SourceValue<String>,
    pub input: SourceValue<String>,
    pub command: SourceValue<String>,
    pub path: SourceValue<String>,
    pub url: SourceValue<String>,
    /// The directory the call ran in, when the harness recorded one — either on
    /// the call's own payload or on the record that encloses it.
    pub cwd: SourceValue<String>,
}

/// What a tool call returned, as the harness recorded it. `output` is the text
/// the agent saw, not a harness-specific breakdown of it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ToolResult {
    /// The call this result answers, when the harness recorded that linkage.
    pub call_id: SourceValue<String>,
    pub output: SourceValue<String>,
    /// Only Claude records a failure flag. It is never inferred from output
    /// text, so the other harnesses leave it Absent.
    pub is_error: SourceValue<bool>,
}

/// An explicitly recorded delegation. Adapters must not manufacture this from
/// conversation-tree parents or adjacent records. Every dimension is
/// independently recorded-or-not: a harness that wrote no duration leaves
/// `duration_ms` Absent rather than zero.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Subagent {
    pub native_id: SourceValue<String>,
    pub agent_type: SourceValue<String>,
    pub prompt: SourceValue<String>,
    pub outcome: SourceValue<String>,
    /// A human label for the child, where a harness records one (Codex).
    pub nickname: SourceValue<String>,
    pub duration_ms: SourceValue<u64>,
    /// The child's own token figures. These never move onto the spawn event's
    /// `token_usage`, which `token_total_from_events` sums for the parent.
    pub token_usage: SourceValue<TokenUsage>,
    /// Populated by the query layer at read time, never by an adapter —
    /// adapters only ever emit Absent here.
    pub child_session_id: SourceValue<String>,
}

impl Subagent {
    /// A delegation with nothing recorded yet, for adapters to fill in only the
    /// dimensions their harness actually wrote.
    pub fn unrecorded() -> Self {
        Self {
            native_id: SourceValue::Absent,
            agent_type: SourceValue::Absent,
            prompt: SourceValue::Absent,
            outcome: SourceValue::Absent,
            nickname: SourceValue::Absent,
            duration_ms: SourceValue::Absent,
            token_usage: SourceValue::Absent,
            child_session_id: SourceValue::Absent,
        }
    }
}

/// An attempted filesystem access. `provenance` identifies recorded versus
/// derived evidence; `ambiguity` marks unresolved command fragments; `cwd` is
/// recorded context and is never joined to `path`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FileReference {
    pub path: String,
    pub target: SourceValue<String>,
    pub operation: SourceValue<String>,
    pub provenance: SourceValue<String>,
    pub ambiguity: SourceValue<String>,
    pub cwd: SourceValue<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TokenUsage {
    pub input: SourceValue<u64>,
    pub output: SourceValue<u64>,
    pub cache_read: SourceValue<u64>,
    pub cache_write: SourceValue<u64>,
    pub total: SourceValue<u64>,
    /// Harness-reported cost in millionths of a US dollar. Integer storage
    /// preserves equality and sub-cent precision; `Absent` means the harness
    /// reported no cost.
    pub cost_total_micros: SourceValue<u64>,
    /// The record that reported these measurements; never an inferred aggregate.
    pub scope: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RelationshipKind {
    ConversationTreeParent,
    ToolCallResult,
    SubagentSpawn,
    SessionForkLineage,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Relationship {
    pub kind: RelationshipKind,
    pub from_event_id: String,
    pub to_native_id: String,
    pub source: Provenance,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Event {
    pub id: String,
    pub session_id: String,
    pub kind: EventKind,
    pub source: Provenance,
    pub native_id: SourceValue<String>,
    /// The API response this event's usage record belongs to, when the harness
    /// wrote one. Claude repeats one response's `usage` object across several
    /// records, so this is the key that collapses them back to a single
    /// response. Codex and Pi record no response identity, leaving this Absent,
    /// which downstream reads as "count every record once".
    pub response_id: SourceValue<String>,
    /// The model that produced this event, when a record of the same transcript
    /// named one. Claude and Pi write it beside the usage it priced; Codex
    /// names it on a `turn_context` record instead, so its usage events carry
    /// the last model named before them in file order.
    ///
    /// Per-event rather than per-session because a model really does change
    /// mid-session — measured on all three harnesses — and a session-level
    /// field would have to discard one of the values a transcript recorded.
    pub model: SourceValue<String>,
    pub timestamp: SourceValue<String>,
    pub turn: SourceValue<Turn>,
    pub tool_call: SourceValue<ToolCall>,
    pub tool_result: SourceValue<ToolResult>,
    pub token_usage: SourceValue<TokenUsage>,
    pub files: SourceValue<Vec<FileReference>>,
    /// A compact reason for unknown/custom records; raw payloads are not required.
    pub detail: SourceValue<String>,
    /// The delegation facts a `SubagentSpawn` event carries. Absent on every
    /// other kind of event.
    pub subagent: SourceValue<Subagent>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ParsedSession {
    pub session: Option<Session>,
    pub events: Vec<Event>,
    pub relationships: Vec<Relationship>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Diagnostic {
    pub source: Provenance,
    pub message: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_value_states_stay_pairwise_distinct() {
        let recorded = SourceValue::Recorded("x".to_string());
        let absent: SourceValue<String> = SourceValue::Absent;
        let unsupported: SourceValue<String> = SourceValue::Unsupported;
        let malformed: SourceValue<String> = SourceValue::Malformed;

        assert_ne!(recorded, absent);
        assert_ne!(absent, unsupported);
        assert_ne!(unsupported, malformed);
        assert_ne!(malformed, recorded);
    }

    #[test]
    fn recorded_values_compare_by_inner_value_not_discriminant() {
        let a = SourceValue::Recorded("x".to_string());
        let b = SourceValue::Recorded("y".to_string());

        assert_ne!(a, b);
        assert_eq!(a, SourceValue::Recorded("x".to_string()));
    }

    #[test]
    fn source_value_composed_in_a_struct_compares_field_wise() {
        let with_text = Turn {
            role: "user".to_string(),
            text: SourceValue::Recorded("hello".to_string()),
        };
        let with_absent_text = Turn {
            role: "user".to_string(),
            text: SourceValue::Absent,
        };
        let same_as_first = Turn {
            role: "user".to_string(),
            text: SourceValue::Recorded("hello".to_string()),
        };

        assert_ne!(with_text, with_absent_text);
        assert_eq!(with_text, same_as_first);
    }
}
