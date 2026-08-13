//! Codex JSONL adapter.

use super::*;

/// The delegation Codex recorded, built from a `spawn_agent` function call's
/// arguments and, when the caller already has it, the matching `wait_agent`
/// status entry. Streaming through a file the status arrives many records
/// later, so `record` builds with `None` and folds the outcome in on arrival.
///
/// Codex records neither a duration nor per-subagent tokens, so both stay
/// Absent; a duration derived from the spawn/wait timestamps would measure the
/// parent's wait, not the child's run.
pub(crate) fn subagent_from_spawn_agent(payload: &Value, wait_status: Option<&Value>) -> Subagent {
    Subagent {
        agent_type: string_field(find_input_field(payload, "arguments", &["agent_type"])),
        prompt: string_field(find_input_field(
            payload,
            "arguments",
            &["message", "prompt"],
        )),
        outcome: wait_status
            .map(spawn_outcome)
            .unwrap_or(SourceValue::Absent),
        ..Subagent::unrecorded()
    }
}

/// A `wait_agent` status entry is a one-key object naming how the delegation
/// ended, e.g. `{"completed": "<final report>"}`. The key is the outcome; the
/// report text is the child's own output, not a fact about the spawn.
fn spawn_outcome(status: &Value) -> SourceValue<String> {
    match status.as_object().and_then(|entry| entry.keys().next()) {
        Some(outcome) => SourceValue::Recorded(outcome.clone()),
        None => SourceValue::Malformed,
    }
}

/// Codex writes a call's output as a JSON-encoded string; this reads it back
/// without treating the encoding as a fact about the call.
fn decoded_output(output: Option<&Value>) -> Option<Value> {
    match output? {
        Value::String(text) => serde_json::from_str(text).ok(),
        other => Some(other.clone()),
    }
}

/// The tool a recorded call named, found by the call id — the only linkage
/// Codex writes between a call and the output that answers it.
fn tool_name_for_call(parsed: &ParsedSession, call_id: &SourceValue<String>) -> Option<String> {
    parsed
        .events
        .iter()
        .find(|event| event.kind == EventKind::ToolCall && &event.native_id == call_id)
        .and_then(|event| match &event.tool_call {
            SourceValue::Recorded(call) => Some(call.name.clone()),
            _ => None,
        })
}

/// Folds a `function_call_output` back onto the spawn it concerns: a
/// `spawn_agent` call's own output names the child, and a `wait_agent` call's
/// output reports how a named child ended. Both are matched by recorded id.
fn link_spawn_output(
    parsed: &mut ParsedSession,
    call_id: &SourceValue<String>,
    output: Option<&Value>,
    source: &Provenance,
) {
    let Some(output) = decoded_output(output) else {
        return;
    };
    if let Some((spawn_id, child)) = apply_spawn_identity(parsed, call_id, &output) {
        push_relationship_if_present(
            parsed,
            RelationshipKind::SubagentSpawn,
            spawn_id,
            child,
            source.clone(),
        );
        return;
    }
    if tool_name_for_call(parsed, call_id).as_deref() == Some("wait_agent") {
        apply_wait_agent_status(parsed, &output);
    }
}

/// Names the child a `spawn_agent` output reported. Returns the spawn event and
/// the child id to link, or `None` when this output answered some other call.
fn apply_spawn_identity(
    parsed: &mut ParsedSession,
    call_id: &SourceValue<String>,
    output: &Value,
) -> Option<(String, Option<String>)> {
    let spawn = parsed
        .events
        .iter_mut()
        .find(|event| event.kind == EventKind::SubagentSpawn && &event.native_id == call_id)?;
    let SourceValue::Recorded(subagent) = &mut spawn.subagent else {
        return None;
    };
    subagent.native_id = string(output, &["agent_id"]);
    subagent.nickname = string(output, &["nickname"]);
    let child = recorded(&subagent.native_id);
    Some((spawn.id.clone(), child))
}

fn apply_wait_agent_status(parsed: &mut ParsedSession, output: &Value) {
    let Some(statuses) = output.get("status").and_then(Value::as_object) else {
        return;
    };
    for (agent_id, status) in statuses {
        let named = SourceValue::Recorded(agent_id.clone());
        for event in parsed.events.iter_mut() {
            if event.kind != EventKind::SubagentSpawn {
                continue;
            }
            if let SourceValue::Recorded(subagent) = &mut event.subagent {
                if subagent.native_id == named {
                    subagent.outcome = spawn_outcome(status);
                }
            }
        }
    }
}

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
        // A delegation, not an undifferentiated tool call. The child id and the
        // outcome arrive in later records, so only what this call recorded is
        // read here.
        Some("function_call")
            if recorded_string(payload, &["name"]).as_deref() == Some("spawn_agent") =>
        {
            let mut spawn = event(&session, EventKind::SubagentSpawn, source, native);
            spawn.subagent = SourceValue::Recorded(subagent_from_spawn_agent(payload, None));
            parsed.events.push(spawn);
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
            push_relationship_if_present(
                parsed,
                RelationshipKind::ToolCallResult,
                id,
                recorded(&native),
                source.clone(),
            );
            link_spawn_output(parsed, &native, payload.get("output"), &source);
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::conformance::{conformance_tests, parse_records, Conformance, ConformanceSpawn};
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
                // A delegation: the call gives type and prompt, its output
                // names the child, and a later wait_agent reports the outcome.
                json!({"type":"response_item","payload":{"type":"function_call","call_id":"call-3","name":"spawn_agent","arguments":"{\"agent_type\":\"explore\",\"message\":\"Map the parser module\"}"}}),
                json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"call-3","output":"{\"agent_id\":\"agent-7\",\"nickname\":\"Avicenna\"}"}}),
                json!({"type":"response_item","payload":{"type":"function_call","call_id":"call-4","name":"wait_agent","arguments":"{\"agent_id\":\"agent-7\"}"}}),
                json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"call-4","output":"{\"status\":{\"agent-7\":{\"completed\":\"report\"}},\"timed_out\":false}"}}),
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
            subagent_spawn: Some(ConformanceSpawn {
                agent_type: "explore",
                prompt: "Map the parser module",
                outcome: Some("completed"),
                // Codex times nothing and counts no per-subagent tokens.
                duration_ms: None,
                child_native_id: Some("agent-7"),
                child_token_total: None,
            }),
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

    /// 433 of 519 local Codex spawns have no output naming a child. The spawn
    /// is still evidence; the missing linkage is not filled in by adjacency.
    #[test]
    fn a_spawn_whose_output_never_named_a_child_emits_the_event_but_no_edge() {
        let value = json!({"type":"response_item","payload":{"type":"function_call","call_id":"call-1","name":"spawn_agent","arguments":"{\"agent_type\":\"explore\",\"message\":\"Map the parser\"}"}});
        let parsed = parse_records(record, Harness::Codex, &[value]);

        assert_eq!(parsed.events.len(), 1);
        assert_eq!(parsed.events[0].kind, EventKind::SubagentSpawn);
        let SourceValue::Recorded(subagent) = &parsed.events[0].subagent else {
            panic!("expected a recorded subagent");
        };
        assert_eq!(
            subagent.prompt,
            SourceValue::Recorded("Map the parser".to_string())
        );
        assert_eq!(subagent.native_id, SourceValue::Absent);
        assert_eq!(subagent.outcome, SourceValue::Absent);
        assert!(parsed.relationships.is_empty());
    }

    /// A named child with no `wait_agent` reporting on it links, but the
    /// outcome stays unrecorded — the two facts are independent.
    #[test]
    fn a_named_child_with_no_wait_agent_links_but_records_no_outcome() {
        let values = [
            json!({"type":"response_item","payload":{"type":"function_call","call_id":"call-1","name":"spawn_agent","arguments":"{\"agent_type\":\"explore\",\"message\":\"Map the parser\"}"}}),
            json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"call-1","output":"{\"agent_id\":\"agent-7\",\"nickname\":\"Avicenna\"}"}}),
        ];
        let parsed = parse_records(record, Harness::Codex, &values);

        let spawn = parsed
            .events
            .iter()
            .find(|event| event.kind == EventKind::SubagentSpawn)
            .expect("a spawn event");
        let SourceValue::Recorded(subagent) = &spawn.subagent else {
            panic!("expected a recorded subagent");
        };
        assert_eq!(
            subagent.native_id,
            SourceValue::Recorded("agent-7".to_string())
        );
        assert_eq!(
            subagent.nickname,
            SourceValue::Recorded("Avicenna".to_string())
        );
        assert_eq!(subagent.outcome, SourceValue::Absent);
        let spawn_edges: Vec<_> = parsed
            .relationships
            .iter()
            .filter(|edge| edge.kind == RelationshipKind::SubagentSpawn)
            .collect();
        assert_eq!(spawn_edges.len(), 1);
        assert_eq!(spawn_edges[0].to_native_id, "agent-7");
    }

    #[test]
    fn a_wait_agent_status_entry_reports_its_key_as_the_outcome() {
        let payload =
            json!({"type":"function_call","call_id":"c","name":"spawn_agent","arguments":"{}"});

        let subagent =
            subagent_from_spawn_agent(&payload, Some(&json!({"failed": "ran out of context"})));

        assert_eq!(
            subagent.outcome,
            SourceValue::Recorded("failed".to_string())
        );
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
