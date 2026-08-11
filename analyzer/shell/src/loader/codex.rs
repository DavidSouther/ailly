//! Codex JSONL adapter.

use super::*;

pub(crate) fn record(value: &Value, parsed: &mut ParsedSession, path: &str, source: Provenance) {
    let record_type = recorded_string(value, &["type"]);
    let payload = value.get("payload").unwrap_or(value);

    if parsed.session.is_none() {
        parsed.session = Some(Session {
            id: session_id(
                Harness::Codex,
                path,
                recorded_string(payload, &["id", "session_id"]),
            ),
            harness: Harness::Codex,
            source: source.clone(),
            native_id: string(payload, &["id", "session_id"]),
            project: string(payload, &["cwd"]),
            parent_session: SourceValue::Absent,
        });
    }

    if record_type.as_deref() != Some("response_item") {
        return;
    }

    let session = parsed
        .session
        .as_ref()
        .expect("session seeded above")
        .id
        .clone();
    let payload_type = recorded_string(payload, &["type"]);
    let native = string(payload, &["id", "call_id"]);

    match payload_type.as_deref() {
        Some("message") => {
            let role = recorded_string(payload, &["role"]);
            let mut turn_event = event(
                &session,
                match role.as_deref() {
                    Some("user") => EventKind::UserTurn,
                    _ => EventKind::AssistantTurn,
                },
                source,
                native,
            );
            turn_event.turn = role
                .map(|role| {
                    SourceValue::Recorded(Turn {
                        role,
                        text: content_text(payload.get("content")),
                    })
                })
                .unwrap_or(SourceValue::Absent);
            turn_event.token_usage = usage(payload.get("usage"), "message");
            parsed.events.push(turn_event);
        }
        Some("function_call") => {
            let mut call = event(&session, EventKind::ToolCall, source, native);
            call.tool_call = SourceValue::Recorded(tool_call(payload, "name", "arguments"));
            parsed.events.push(call);
        }
        Some("function_call_output") => {
            let result = event(
                &session,
                EventKind::ToolResult,
                source.clone(),
                native.clone(),
            );
            let id = result.id.clone();
            parsed.events.push(result);
            let call_id = match native {
                SourceValue::Recorded(call_id) => Some(call_id),
                _ => None,
            };
            push_relationship_if_present(
                parsed,
                RelationshipKind::ToolCallResult,
                id,
                call_id,
                source,
            );
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::conformance::{conformance_tests, parse_records, Conformance};
    use super::*;
    use serde_json::json;

    /// Minimal Codex transcript: a session_meta header, then response_item
    /// records for the turns, a function_call, and its linked output. Codex
    /// is a flat response stream (no conversation-tree parents), and its
    /// response items carry no usage, so those expectations are None.
    fn conformance() -> Conformance {
        Conformance {
            harness: Harness::Codex,
            record,
            transcript: vec![
                json!({"type":"session_meta","payload":{"id":"native-session","cwd":"/tmp/proj"}}),
                json!({"type":"response_item","payload":{"type":"message","id":"turn-user","role":"user","content":"hello"}}),
                json!({"type":"response_item","payload":{"type":"message","id":"turn-assistant","role":"assistant","content":"reading a file"}}),
                json!({"type":"response_item","payload":{"type":"function_call","call_id":"call-1","name":"read_file","arguments":"{\"path\":\"README.md\"}"}}),
                json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"call-1","output":"ok"}}),
            ],
            native_session_id: "native-session",
            project: "/tmp/proj",
            tool_name: "read_file",
            tool_path: "README.md",
            tool_result_call_id: Some("call-1"),
            tree_parent_id: None,
            // No Codex response_item supplies usage; it must resolve Absent,
            // never a fabricated zeroed TokenUsage.
            assistant_usage: None,
        }
    }

    conformance_tests!(conformance);

    // --- Codex-specific quirks ---

    #[test]
    fn seeds_the_session_from_a_session_meta_record_without_producing_events() {
        let value = json!({"type":"session_meta","payload":{"id":"codex-1","cwd":"/work"}});
        let parsed = parse_records(record, Harness::Codex, &[value]);

        let session = parsed.session.expect("session header seeded");
        assert_eq!(
            session.native_id,
            SourceValue::Recorded("codex-1".to_string())
        );
        // session_meta is not a response_item; it seeds the session but produces no events.
        assert!(parsed.events.is_empty());
    }

    #[test]
    fn a_non_response_item_record_produces_no_events() {
        let value =
            json!({"type":"event_msg","payload":{"type":"agent_reasoning","text":"thinking"}});
        let parsed = parse_records(record, Harness::Codex, &[value]);

        assert!(parsed.events.is_empty());
    }

    #[test]
    fn string_encoded_arguments_still_yield_a_recorded_input_and_path() {
        let value = json!({"type":"response_item","payload":{"type":"function_call","call_id":"c1","name":"exec_command","arguments":"{\"path\":\"Cargo.toml\"}"}});
        let parsed = parse_records(record, Harness::Codex, &[value]);

        let SourceValue::Recorded(tool_call) = &parsed.events[0].tool_call else {
            panic!(
                "expected a recorded tool call, got {:?}",
                parsed.events[0].tool_call
            );
        };
        // arguments arrive as a JSON-encoded string, not an object; input must still
        // resolve to Recorded, never Malformed, and path is pulled out of the string.
        assert!(matches!(tool_call.input, SourceValue::Recorded(_)));
        assert_eq!(
            tool_call.path,
            SourceValue::Recorded("Cargo.toml".to_string())
        );
    }
}
