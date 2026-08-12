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

    // Claude records the working directory on the record, not on the tool_use
    // block inside it, so this must be read before descending into `message`.
    let record_cwd = string(value, &["cwd"]);
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
        for block in blocks {
            match block.get("type").and_then(Value::as_str) {
                Some("tool_use") => {
                    let mut call = event(
                        &session,
                        EventKind::ToolCall,
                        source.clone(),
                        string(block, &["id"]),
                    );
                    call.tool_call = SourceValue::Recorded(tool_call(
                        block,
                        "name",
                        "input",
                        record_cwd.clone(),
                    ));
                    parsed.events.push(call);
                }
                // Claude delivers results as blocks on a user record. The block
                // is its own event, keyed by the call it answers so two results
                // sharing a record stay distinct.
                Some("tool_result") => {
                    let mut result = event(
                        &session,
                        EventKind::ToolResult,
                        source.clone(),
                        string(block, &["tool_use_id"]),
                    );
                    result.tool_result =
                        SourceValue::Recorded(tool_result(block, &["tool_use_id"], "content"));
                    let result_id = result.id.clone();
                    parsed.events.push(result);
                    push_relationship_if_present(
                        parsed,
                        RelationshipKind::ToolCallResult,
                        result_id,
                        recorded_string(block, &["tool_use_id"]),
                        source.clone(),
                    );
                }
                _ => {}
            }
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
    /// the assistant record carries two tool_use blocks, usage, and a
    /// parentUuid pointing back at the user turn; a third record delivers the
    /// first call's result. Claude stamps `cwd` on every record, which is where
    /// its calls' working directory comes from. Its results arrive as
    /// user-typed records carrying `tool_result` blocks, so that third record
    /// is both a user turn (with no text of its own) and a tool result.
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
                    "cwd": "/tmp/proj",
                    "message": {
                        "role": "assistant",
                        "content": [
                            {"type": "text", "text": "reading a file"},
                            {"type": "tool_use", "id": "call-1", "name": "Read", "input": {"file_path": "README.md"}},
                            {"type": "tool_use", "id": "call-2", "name": "Bash", "input": {"command": "cargo test"}}
                        ],
                        "usage": {"input_tokens": 10, "output_tokens": 2}
                    }
                }),
                json!({
                    "type": "user",
                    "uuid": "result-1",
                    "sessionId": "native-session",
                    "cwd": "/tmp/proj",
                    "message": {
                        "role": "user",
                        "content": [
                            {"type": "tool_result", "tool_use_id": "call-1", "is_error": false, "content": "ok"}
                        ]
                    }
                }),
            ],
            native_session_id: "native-session",
            project: "/tmp/proj",
            tool_name: "Read",
            tool_path: "README.md",
            shell_tool_name: "Bash",
            tool_command: "cargo test",
            tool_cwd: Some("/tmp/proj"),
            tool_result_call_id: Some("call-1"),
            tool_result_output: Some("ok"),
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
    fn a_tool_calls_directory_comes_from_the_enclosing_record_not_the_tool_use_block() {
        // Claude stamps `cwd` on the JSONL record; the tool_use block carries none.
        let value = json!({
            "type": "assistant",
            "uuid": "a",
            "sessionId": "s",
            "cwd": "/Users/dev/repo",
            "message": {
                "role": "assistant",
                "content": [{"type": "tool_use", "id": "call", "name": "Bash", "input": {"command": "ls"}}]
            }
        });
        let parsed = parse_records(record, Harness::ClaudeCode, &[value]);

        let SourceValue::Recorded(call) = &parsed.events[1].tool_call else {
            panic!("expected a recorded tool call");
        };
        assert_eq!(
            call.cwd,
            SourceValue::Recorded("/Users/dev/repo".to_string())
        );
    }

    #[test]
    fn a_record_without_a_cwd_leaves_its_tool_calls_directory_absent() {
        let value = json!({
            "type": "assistant",
            "uuid": "a",
            "sessionId": "s",
            "message": {
                "role": "assistant",
                "content": [{"type": "tool_use", "id": "call", "name": "Bash", "input": {"command": "ls"}}]
            }
        });
        let parsed = parse_records(record, Harness::ClaudeCode, &[value]);

        let SourceValue::Recorded(call) = &parsed.events[1].tool_call else {
            panic!("expected a recorded tool call");
        };
        assert_eq!(call.cwd, SourceValue::Absent);
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
