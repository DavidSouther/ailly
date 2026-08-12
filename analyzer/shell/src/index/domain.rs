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
