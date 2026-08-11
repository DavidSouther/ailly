//! Claude Code JSONL adapter.

use super::*;

pub(crate) fn record(value: &Value, parsed: &mut ParsedSession, path: &str, source: Provenance) {
    let record_type = recorded_string(value, &["type"]);
    let native = string(value, &["uuid"]);
    let session = session_id(
        Harness::ClaudeCode,
        path,
        recorded_string(value, &["sessionId"]),
    );
    if parsed.session.is_none() {
        parsed.session = Some(Session {
            id: session.clone(),
            harness: Harness::ClaudeCode,
            source: source.clone(),
            native_id: string(value, &["sessionId"]),
            project: string(value, &["cwd"]),
            parent_session: SourceValue::Absent,
        });
    }

    let message = value.get("message").unwrap_or(value);
    let role = recorded_string(message, &["role"]);
    let mut base = event(
        &session,
        match role.as_deref() {
            Some("user") => EventKind::UserTurn,
            Some("assistant") => EventKind::AssistantTurn,
            _ => EventKind::Unknown,
        },
        source.clone(),
        native.clone(),
    );
    base.turn = role
        .clone()
        .map(|role| {
            SourceValue::Recorded(Turn {
                role,
                text: content_text(message.get("content")),
            })
        })
        .unwrap_or(SourceValue::Absent);
    base.token_usage = usage(message.get("usage"), "message");
    let base_id = base.id.clone();

    if matches!(record_type.as_deref(), Some("user") | Some("assistant")) {
        parsed.events.push(base);
    }

    if let Some(blocks) = message.get("content").and_then(Value::as_array) {
        for block in blocks
            .iter()
            .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"))
        {
            let mut call = event(
                &session,
                EventKind::ToolCall,
                source.clone(),
                string(block, &["id"]),
            );
            call.tool_call = SourceValue::Recorded(tool_call(block, "name", "input"));
            parsed.events.push(call);
        }
    }

    push_relationship_if_present(
        parsed,
        RelationshipKind::ConversationTreeParent,
        base_id,
        recorded_string(value, &["parentUuid"]),
        source,
    );
}

#[cfg(test)]
mod tests {
    use super::conformance::{conformance_tests, parse_records, Conformance};
    use super::*;
    use serde_json::json;

    /// Minimal Claude transcript: the user record seeds the session header;
    /// the assistant record carries a tool_use block, usage, and a parentUuid
    /// pointing back at the user turn. Claude records no tool-result linkage
    /// (results ride the conversation tree instead), so
    /// `tool_result_call_id` is None.
    fn conformance() -> Conformance {
        Conformance {
            harness: Harness::ClaudeCode,
            record,
            transcript: vec![
                json!({
                    "type": "user",
                    "uuid": "turn-user",
                    "sessionId": "native-session",
                    "cwd": "/tmp/proj",
                    "message": {"role": "user", "content": "hello"}
                }),
                json!({
                    "type": "assistant",
                    "uuid": "turn-assistant",
                    "parentUuid": "turn-user",
                    "sessionId": "native-session",
                    "message": {
                        "role": "assistant",
                        "content": [
                            {"type": "text", "text": "reading a file"},
                            {"type": "tool_use", "id": "call-1", "name": "Read", "input": {"file_path": "README.md"}}
                        ],
                        "usage": {"input_tokens": 10, "output_tokens": 2}
                    }
                }),
            ],
            native_session_id: "native-session",
            project: "/tmp/proj",
            tool_name: "Read",
            tool_path: "README.md",
            tool_result_call_id: None,
            tree_parent_id: Some("turn-user"),
            assistant_usage: Some(TokenUsage {
                input: SourceValue::Recorded(10),
                output: SourceValue::Recorded(2),
                // Claude records no cache or total fields here; they must
                // stay Absent, never a fabricated zero.
                cache_read: SourceValue::Absent,
                cache_write: SourceValue::Absent,
                total: SourceValue::Absent,
                scope: "message".to_string(),
            }),
        }
    }

    conformance_tests!(conformance);

    // --- Claude-specific quirks ---

    #[test]
    fn assistant_record_with_no_tool_use_blocks_produces_no_tool_call_events() {
        let value = json!({
            "type": "assistant",
            "uuid": "a",
            "sessionId": "s",
            "message": {
                "role": "assistant",
                "content": [{"type": "text", "text": "just talking"}]
            }
        });
        let parsed = parse_records(record, Harness::ClaudeCode, &[value]);

        assert_eq!(parsed.events.len(), 1);
        assert!(parsed
            .events
            .iter()
            .all(|event| event.kind != EventKind::ToolCall));
    }

    #[test]
    fn a_record_missing_parent_uuid_produces_no_relationships() {
        let value = json!({
            "type": "user",
            "uuid": "a",
            "sessionId": "s",
            "message": {"role": "user", "content": "hello"}
        });
        let parsed = parse_records(record, Harness::ClaudeCode, &[value]);

        assert!(parsed.relationships.is_empty());
    }

    #[test]
    fn a_non_turn_record_type_produces_no_turn_event_but_still_surfaces_tool_use() {
        // e.g. "summary" or other record types: the base event is not pushed,
        // but any embedded tool_use evidence still surfaces.
        let value = json!({
            "type": "summary",
            "uuid": "a",
            "sessionId": "s",
            "message": {
                "role": "assistant",
                "content": [{"type": "tool_use", "id": "call", "name": "Read", "input": {"file_path": "README.md"}}]
            }
        });
        let parsed = parse_records(record, Harness::ClaudeCode, &[value]);

        assert_eq!(parsed.events.len(), 1);
        assert_eq!(parsed.events[0].kind, EventKind::ToolCall);
    }
}
