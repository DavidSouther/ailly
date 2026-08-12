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
            call.tool_call =
                SourceValue::Recorded(tool_call(payload, "name", "arguments", SourceValue::Absent));
            parsed.events.push(call);
        }
        // A custom tool call carries its payload as a JavaScript snippet under
        // `input` rather than JSON under `arguments`. It still records the tool
        // that ran, so it surfaces as a call; whatever the snippet says stays
        // uninterpreted in `input`.
        Some("custom_tool_call") => {
            let mut call = event(&session, EventKind::ToolCall, source, native);
            call.tool_call =
                SourceValue::Recorded(tool_call(payload, "name", "input", SourceValue::Absent));
            parsed.events.push(call);
        }
        Some("function_call_output") => {
            let mut result = event(
                &session,
                EventKind::ToolResult,
                source.clone(),
                native.clone(),
            );
            result.tool_result =
                SourceValue::Recorded(tool_result(payload, &["call_id"], "output"));
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
                json!({"type":"response_item","payload":{"type":"function_call","call_id":"call-2","name":"exec_command","arguments":"{\"cmd\":\"cargo test\",\"workdir\":\"/tmp/proj\"}"}}),
                json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"call-1","output":"ok"}}),
            ],
            native_session_id: "native-session",
            project: "/tmp/proj",
            tool_name: "read_file",
            tool_path: "README.md",
            shell_tool_name: "exec_command",
            tool_command: "cargo test",
            // Codex records the directory on the call itself, not on the record.
            tool_cwd: Some("/tmp/proj"),
            tool_result_call_id: Some("call-1"),
            tool_result_output: Some("ok"),
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
    fn custom_tool_call_records_surface_as_tool_call_events_with_input_preserved_verbatim() {
        let value = json!({"type":"response_item","payload":{
            "type":"custom_tool_call","id":"ctc_1","call_id":"call_1","name":"exec",
            "input":"const r = await tools.exec_command({\"cmd\":\"ls\"});\ntext(r.output);"
        }});
        let parsed = parse_records(record, Harness::Codex, &[value]);

        assert_eq!(parsed.events.len(), 1);
        assert_eq!(parsed.events[0].kind, EventKind::ToolCall);
        let SourceValue::Recorded(call) = &parsed.events[0].tool_call else {
            panic!("expected a recorded tool call");
        };
        assert_eq!(call.name, "exec");
        assert!(matches!(call.input, SourceValue::Recorded(_)));
        // The input is a JavaScript snippet, not JSON. Reading a command out
        // of it would be inference, so these stay honestly unrecorded.
        assert_eq!(call.command, SourceValue::Absent);
        assert_eq!(call.cwd, SourceValue::Absent);
        assert_eq!(call.path, SourceValue::Absent);
    }

    #[test]
    fn an_apply_patch_custom_tool_call_gets_no_special_casing_by_name() {
        let value = json!({"type":"response_item","payload":{
            "type":"custom_tool_call","id":"ctc_2","call_id":"call_2","name":"apply_patch",
            "input":"*** Begin Patch\n*** End Patch"
        }});
        let parsed = parse_records(record, Harness::Codex, &[value]);

        let SourceValue::Recorded(call) = &parsed.events[0].tool_call else {
            panic!("expected a recorded tool call");
        };
        assert_eq!(call.name, "apply_patch");
        assert_eq!(call.command, SourceValue::Absent);
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
