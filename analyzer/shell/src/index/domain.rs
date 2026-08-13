//! Domain helpers shared by every index backend.

use crate::model::{Event, EventKind, Harness, ParsedSession, Relationship, Session, SourceValue};
use std::collections::HashMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileIdentity {
    pub path: String,
    pub mtime_secs: i64,
    pub mtime_nanos: u32,
    pub size: i64,
}

#[derive(Clone, Debug, Default)]
pub struct ParsedBatch {
    pub sessions: HashMap<String, Session>,
    pub events: Vec<Event>,
    pub relationships: Vec<Relationship>,
    pub diagnostics: Vec<crate::model::Diagnostic>,
    pub search: Vec<SearchRow>,
}

#[derive(Clone, Debug)]
pub struct SearchRow {
    pub event_id: String,
    pub session_id: String,
    pub text: String,
}

pub fn batch_from_parsed(
    harness: Harness,
    source_path: &str,
    parsed: ParsedSession,
) -> ParsedBatch {
    let mut batch = ParsedBatch::default();
    if let Some(session) = parsed.session {
        batch.sessions.insert(session.id.clone(), session);
    }

    for event in &parsed.events {
        batch
            .sessions
            .entry(event.session_id.clone())
            .or_insert_with(|| Session {
                id: event.session_id.clone(),
                harness,
                source: event.source.clone(),
                native_id: SourceValue::Absent,
                project: SourceValue::Absent,
                parent_session: SourceValue::Absent,
            });
    }

    for event in parsed.events {
        if let Some(text) = search_text_for_event(&event) {
            batch.search.push(SearchRow {
                event_id: event.id.clone(),
                session_id: event.session_id.clone(),
                text,
            });
        }
        batch.events.push(event);
    }

    batch.relationships = parsed.relationships;
    batch.diagnostics = parsed.diagnostics;
    let _ = source_path;
    batch
}

pub fn search_text_for_event(event: &Event) -> Option<String> {
    let mut parts = Vec::new();
    if let SourceValue::Recorded(turn) = &event.turn {
        if let SourceValue::Recorded(text) = &turn.text {
            parts.push(text.as_str());
        }
    }
    if let SourceValue::Recorded(tool) = &event.tool_call {
        parts.push(tool.name.as_str());
        if let SourceValue::Recorded(path) = &tool.path {
            parts.push(path.as_str());
        }
        if let SourceValue::Recorded(url) = &tool.url {
            parts.push(url.as_str());
        }
        if let SourceValue::Recorded(command) = &tool.command {
            parts.push(command.as_str());
        }
    }
    if let SourceValue::Recorded(files) = &event.files {
        for file in files {
            parts.push(file.path.as_str());
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

pub fn token_total_from_events(events: &[Event]) -> (SourceValue<u64>, usize) {
    let mut recorded_count = 0usize;
    let mut sum = 0u64;
    let mut any_total = false;
    for event in events {
        if let SourceValue::Recorded(usage) = &event.token_usage {
            recorded_count += 1;
            if let SourceValue::Recorded(total) = usage.total {
                sum = sum.saturating_add(total);
                any_total = true;
            }
        }
    }
    let token_total = if recorded_count == 0 {
        SourceValue::Absent
    } else if any_total {
        SourceValue::Recorded(sum)
    } else {
        SourceValue::Absent
    };
    (token_total, recorded_count)
}

/// The indexed session a spawn's recorded child id names, when one is indexed.
/// A spawn that named no child, and a named child nothing answered, both
/// resolve Absent: linkage is read from what the source wrote, never inferred
/// from timestamps or adjacency.
///
/// Both harnesses that name a child also write that child's transcript to a
/// path built from the same id, which is the linkage this reads. Pi names no
/// child at all, so it never resolves.
pub fn resolve_child_session_id(
    sessions: &[Session],
    harness: Harness,
    spawn_native_id: &SourceValue<String>,
) -> SourceValue<String> {
    let SourceValue::Recorded(child_id) = spawn_native_id else {
        return SourceValue::Absent;
    };
    let suffix = match harness {
        // ~/.claude/projects/<project>/<parent>/subagents/agent-<agentId>.jsonl
        Harness::ClaudeCode => format!("subagents/agent-{child_id}.jsonl"),
        // ~/.codex/sessions/YYYY/MM/DD/rollout-<timestamp>-<agent_id>.jsonl
        Harness::Codex => format!("-{child_id}.jsonl"),
        Harness::Pi => return SourceValue::Absent,
    };
    sessions
        .iter()
        .find(|session| session.harness == harness && session.source.path.ends_with(&suffix))
        .map(|session| SourceValue::Recorded(session.id.clone()))
        .unwrap_or(SourceValue::Absent)
}

pub fn harness_name(harness: Harness) -> &'static str {
    match harness {
        Harness::ClaudeCode => "claude_code",
        Harness::Codex => "codex",
        Harness::Pi => "pi",
    }
}

pub fn parse_harness(text: &str) -> Option<Harness> {
    match text {
        "claude_code" => Some(Harness::ClaudeCode),
        "codex" => Some(Harness::Codex),
        "pi" => Some(Harness::Pi),
        _ => None,
    }
}

pub fn event_kind_name(kind: EventKind) -> &'static str {
    match kind {
        EventKind::UserTurn => "user_turn",
        EventKind::AssistantTurn => "assistant_turn",
        EventKind::ToolCall => "tool_call",
        EventKind::ToolResult => "tool_result",
        EventKind::SubagentSpawn => "subagent_spawn",
        EventKind::SessionMetadata => "session_metadata",
        EventKind::ModelChange => "model_change",
        EventKind::ThinkingChange => "thinking_change",
        EventKind::Summary => "summary",
        EventKind::Unknown => "unknown",
    }
}

pub fn parse_event_kind(text: &str) -> EventKind {
    match text {
        "user_turn" => EventKind::UserTurn,
        "assistant_turn" => EventKind::AssistantTurn,
        "tool_call" => EventKind::ToolCall,
        "tool_result" => EventKind::ToolResult,
        "subagent_spawn" => EventKind::SubagentSpawn,
        "session_metadata" => EventKind::SessionMetadata,
        "model_change" => EventKind::ModelChange,
        "thinking_change" => EventKind::ThinkingChange,
        "summary" => EventKind::Summary,
        _ => EventKind::Unknown,
    }
}

pub fn relationship_kind_name(kind: crate::model::RelationshipKind) -> &'static str {
    match kind {
        crate::model::RelationshipKind::ConversationTreeParent => "conversation_tree_parent",
        crate::model::RelationshipKind::ToolCallResult => "tool_call_result",
        crate::model::RelationshipKind::SubagentSpawn => "subagent_spawn",
        crate::model::RelationshipKind::SessionForkLineage => "session_fork_lineage",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Provenance;

    fn session(id: &str, harness: Harness, path: &str) -> Session {
        Session {
            id: id.to_string(),
            harness,
            source: Provenance {
                harness,
                path: path.to_string(),
                line: 1,
                ordinal: 1,
            },
            native_id: SourceValue::Absent,
            project: SourceValue::Absent,
            parent_session: SourceValue::Absent,
        }
    }

    #[test]
    fn a_claude_spawn_resolves_to_the_child_transcript_named_after_its_agent_id() {
        let sessions = [
            session("parent", Harness::ClaudeCode, "/p/proj/parent.jsonl"),
            session(
                "child",
                Harness::ClaudeCode,
                "/p/proj/parent/subagents/agent-a3c304e0.jsonl",
            ),
        ];

        let resolved = resolve_child_session_id(
            &sessions,
            Harness::ClaudeCode,
            &SourceValue::Recorded("a3c304e0".to_string()),
        );

        assert_eq!(resolved, SourceValue::Recorded("child".to_string()));
    }

    #[test]
    fn a_codex_spawn_resolves_to_the_rollout_named_after_its_agent_id() {
        let sessions = [session(
            "child",
            Harness::Codex,
            "/p/.codex/sessions/2026/08/12/rollout-2026-08-12T10-00-00-agent-7.jsonl",
        )];

        let resolved = resolve_child_session_id(
            &sessions,
            Harness::Codex,
            &SourceValue::Recorded("agent-7".to_string()),
        );

        assert_eq!(resolved, SourceValue::Recorded("child".to_string()));
    }

    /// "The source named a child nothing answered" is a different fact from
    /// "no child was named", but the honest report of both is the same: there
    /// is no session to open.
    #[test]
    fn a_named_child_that_nothing_indexed_answers_resolves_absent() {
        let sessions = [session("parent", Harness::ClaudeCode, "/p/parent.jsonl")];

        let resolved = resolve_child_session_id(
            &sessions,
            Harness::ClaudeCode,
            &SourceValue::Recorded("a3c304e0".to_string()),
        );

        assert_eq!(resolved, SourceValue::Absent);
    }

    #[test]
    fn a_spawn_that_named_no_child_resolves_absent_without_searching() {
        let sessions = [session(
            "child",
            Harness::ClaudeCode,
            "/p/parent/subagents/agent-a3c304e0.jsonl",
        )];

        let resolved =
            resolve_child_session_id(&sessions, Harness::ClaudeCode, &SourceValue::Absent);

        assert_eq!(resolved, SourceValue::Absent);
    }

    /// Pi records no child id at all, so nothing about a Pi session may be
    /// offered as one — not even a path that happens to look right.
    #[test]
    fn a_pi_spawn_never_resolves_a_child() {
        let sessions = [session(
            "child",
            Harness::Pi,
            "/p/subagents/agent-a3c304e0.jsonl",
        )];

        let resolved = resolve_child_session_id(
            &sessions,
            Harness::Pi,
            &SourceValue::Recorded("a3c304e0".to_string()),
        );

        assert_eq!(resolved, SourceValue::Absent);
    }
}
