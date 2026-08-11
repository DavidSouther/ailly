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

/// Tracks whether a value was found in the source document. Consumers must preserve these
/// distinctions rather than treating missing, unsupported, or malformed data as
/// an observed value.
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
}

/// An explicitly recorded delegation. Adapters must not manufacture this from
/// conversation-tree parents or adjacent records.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Subagent {
    pub native_id: SourceValue<String>,
    pub agent_type: SourceValue<String>,
    pub prompt: SourceValue<String>,
    pub outcome: SourceValue<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct FileReference {
    pub path: String,
    pub operation: SourceValue<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct TokenUsage {
    pub input: SourceValue<u64>,
    pub output: SourceValue<u64>,
    pub cache_read: SourceValue<u64>,
    pub cache_write: SourceValue<u64>,
    pub total: SourceValue<u64>,
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
    pub timestamp: SourceValue<String>,
    pub turn: SourceValue<Turn>,
    pub tool_call: SourceValue<ToolCall>,
    pub token_usage: SourceValue<TokenUsage>,
    pub files: SourceValue<Vec<FileReference>>,
    /// A compact reason for unknown/custom records; raw payloads are not required.
    pub detail: SourceValue<String>,
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
