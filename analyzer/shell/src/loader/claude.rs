//! Claude Code JSONL adapter.

use super::*;

/// Claude delegates through an ordinary `tool_use` block; only its name says so.
fn is_agent_call(block: &Value) -> bool {
    matches!(
        block.get("name").and_then(Value::as_str),
        Some("Agent") | Some("Task")
    )
}

/// The delegation Claude recorded, built from an `Agent`/`Task` `tool_use`
/// block plus, when the enclosing record carried one, its `toolUseResult`.
pub(crate) fn subagent_from_agent_call(block: &Value, tool_use_result: Option<&Value>) -> Subagent {
    let mut subagent = Subagent {
        agent_type: string_field(find_input_field(block, "input", &["subagent_type"])),
        prompt: string_field(find_input_field(block, "input", &["prompt", "description"])),
        ..Subagent::unrecorded()
    };
    if let Some(result) = tool_use_result {
        apply_agent_result(&mut subagent, result);
    }
    subagent
}

/// Folds what a delegation's `toolUseResult` reported into the spawn its call
/// recorded. The call-side type and prompt win where both sides carry one,
/// because that is what the orchestrator actually asked for.
fn apply_agent_result(subagent: &mut Subagent, result: &Value) {
    subagent.outcome = string(result, &["status"]);
    subagent.native_id = string(result, &["agentId"]);
    subagent.duration_ms = number(result.get("totalDurationMs"));
    subagent.token_usage = subagent_usage(result);
    if matches!(subagent.agent_type, SourceValue::Absent) {
        subagent.agent_type = string(result, &["agentType"]);
    }
    if matches!(subagent.prompt, SourceValue::Absent) {
        subagent.prompt = string(result, &["prompt"]);
    }
}

/// The child's own token figures. Claude splits them: the per-kind counts sit
/// in `usage`, while the rolled-up total sits beside it as `totalTokens`.
fn subagent_usage(result: &Value) -> SourceValue<TokenUsage> {
    let total = number(result.get("totalTokens"));
    match usage(result.get("usage"), "subagent") {
        SourceValue::Recorded(mut recorded_usage) => {
            if matches!(recorded_usage.total, SourceValue::Absent) {
                recorded_usage.total = total;
            }
            SourceValue::Recorded(recorded_usage)
        }
        // No per-kind breakdown, but a total is still a recorded fact.
        _ => match total {
            SourceValue::Recorded(total) => SourceValue::Recorded(TokenUsage {
                input: SourceValue::Absent,
                output: SourceValue::Absent,
                cache_read: SourceValue::Absent,
                cache_write: SourceValue::Absent,
                total: SourceValue::Recorded(total),
                scope: "subagent".to_string(),
            }),
            _ => SourceValue::Absent,
        },
    }
}

/// Folds a delegation's reported result back onto the spawn event the same
/// transcript recorded, matched by the `tool_use` id the result names — never
/// by adjacency. Claude writes `toolUseResult` on the later user record that
/// delivers the result block, not on the assistant record that made the call.
fn link_agent_result(
    parsed: &mut ParsedSession,
    tool_use_id: Option<String>,
    result: &Value,
    source: &Provenance,
) {
    let Some(tool_use_id) = tool_use_id else {
        return;
    };
    let named = SourceValue::Recorded(tool_use_id);
    let linked = {
        let Some(spawn) = parsed
            .events
            .iter_mut()
            .find(|event| event.kind == EventKind::SubagentSpawn && event.native_id == named)
        else {
            return;
        };
        let SourceValue::Recorded(subagent) = &mut spawn.subagent else {
            return;
        };
        // The edge is emitted where the child id is first named, so a record
        // that repeats an already-known id does not duplicate it.
        let already_named = matches!(subagent.native_id, SourceValue::Recorded(_));
        apply_agent_result(subagent, result);
        let child = if already_named {
            None
        } else {
            recorded(&subagent.native_id)
        };
        (spawn.id.clone(), child)
    };
    push_relationship_if_present(
        parsed,
        RelationshipKind::SubagentSpawn,
        linked.0,
        linked.1,
        source.clone(),
    );
}

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
    let timestamp = string(value, &["timestamp"]);
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
        timestamp.clone(),
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
                // An explicitly recorded delegation, not an undifferentiated
                // tool call. Its own tokens stay on the payload so they never
                // inflate the parent session's total.
                Some("tool_use") if is_agent_call(block) => {
                    let mut spawn = event(
                        &session,
                        EventKind::SubagentSpawn,
                        source.clone(),
                        string(block, &["id"]),
                        timestamp.clone(),
                    );
                    let subagent = subagent_from_agent_call(block, value.get("toolUseResult"));
                    let child = recorded(&subagent.native_id);
                    spawn.subagent = SourceValue::Recorded(subagent);
                    let spawn_id = spawn.id.clone();
                    parsed.events.push(spawn);
                    push_relationship_if_present(
                        parsed,
                        RelationshipKind::SubagentSpawn,
                        spawn_id,
                        child,
                        source.clone(),
                    );
                }
                Some("tool_use") => {
                    let mut call = event(
                        &session,
                        EventKind::ToolCall,
                        source.clone(),
                        string(block, &["id"]),
                        timestamp.clone(),
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
                        timestamp.clone(),
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
                    if let Some(result) = value.get("toolUseResult") {
                        link_agent_result(
                            parsed,
                            recorded_string(block, &["tool_use_id"]),
                            result,
                            &source,
                        );
                    }
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
    use super::conformance::{conformance_tests, parse_records, Conformance, ConformanceSpawn};
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
                            {"type": "tool_use", "id": "call-2", "name": "Bash", "input": {"command": "cargo test"}},
                            {"type": "tool_use", "id": "call-3", "name": "Agent", "input": {"subagent_type": "explore", "description": "map the parser", "prompt": "Map the parser module"}}
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
                // A delegation's outcome arrives on the later user record that
                // delivers its result block, as a sibling `toolUseResult`.
                json!({
                    "type": "user",
                    "uuid": "result-3",
                    "sessionId": "native-session",
                    "cwd": "/tmp/proj",
                    "message": {
                        "role": "user",
                        "content": [
                            {"type": "tool_result", "tool_use_id": "call-3", "content": "mapped"}
                        ]
                    },
                    "toolUseResult": {
                        "status": "completed",
                        "agentId": "a3c304e0",
                        "agentType": "explore",
                        "totalDurationMs": 467407,
                        "totalTokens": 128450,
                        "usage": {"input_tokens": 4210, "output_tokens": 9330}
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
            // Claude is the one harness that records every dimension, and it
            // names the child, so this fixture is the positive edge case.
            subagent_spawn: Some(ConformanceSpawn {
                agent_type: "explore",
                prompt: "Map the parser module",
                outcome: Some("completed"),
                duration_ms: Some(467407),
                child_native_id: Some("a3c304e0"),
                child_token_total: Some(128450),
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

    /// 149 of 223 local Claude spawns never had a result written back. The
    /// delegation still happened, so the spawn is recorded; everything the
    /// result would have carried stays unrecorded rather than zeroed.
    #[test]
    fn an_agent_call_whose_result_was_never_written_back_records_the_spawn_and_nothing_more() {
        let value = json!({
            "type": "assistant",
            "uuid": "a",
            "sessionId": "s",
            "message": {
                "role": "assistant",
                "content": [{"type": "tool_use", "id": "call-1", "name": "Agent", "input": {"subagent_type": "explore", "prompt": "Map the parser"}}]
            }
        });
        let parsed = parse_records(record, Harness::ClaudeCode, &[value]);

        let spawn = parsed
            .events
            .iter()
            .find(|event| event.kind == EventKind::SubagentSpawn)
            .expect("a spawn event");
        let SourceValue::Recorded(subagent) = &spawn.subagent else {
            panic!("expected a recorded subagent");
        };
        assert_eq!(
            subagent.agent_type,
            SourceValue::Recorded("explore".to_string())
        );
        assert_eq!(subagent.outcome, SourceValue::Absent);
        assert_eq!(subagent.duration_ms, SourceValue::Absent);
        assert_eq!(subagent.token_usage, SourceValue::Absent);
        assert_eq!(subagent.native_id, SourceValue::Absent);
        assert!(parsed.relationships.is_empty());
    }

    /// The older `Task` spelling is the same delegation; both must surface as a
    /// spawn rather than as an ordinary tool call.
    #[test]
    fn a_task_named_delegation_is_a_spawn_not_a_tool_call() {
        let value = json!({
            "type": "assistant",
            "uuid": "a",
            "sessionId": "s",
            "message": {
                "role": "assistant",
                "content": [{"type": "tool_use", "id": "call-1", "name": "Task", "input": {"description": "look around"}}]
            }
        });
        let parsed = parse_records(record, Harness::ClaudeCode, &[value]);

        assert!(parsed
            .events
            .iter()
            .all(|event| event.kind != EventKind::ToolCall));
        let spawn = parsed
            .events
            .iter()
            .find(|event| event.kind == EventKind::SubagentSpawn)
            .expect("a spawn event");
        let SourceValue::Recorded(subagent) = &spawn.subagent else {
            panic!("expected a recorded subagent");
        };
        // Only a description was recorded; it is the prompt evidence there is.
        assert_eq!(
            subagent.prompt,
            SourceValue::Recorded("look around".to_string())
        );
        assert_eq!(subagent.agent_type, SourceValue::Absent);
    }

    /// A result whose `usage` carries no rolled-up total still recorded one
    /// beside it, and the payload must read it rather than report no tokens.
    #[test]
    fn a_results_total_tokens_fill_in_for_a_usage_object_that_carries_none() {
        let result = json!({
            "status": "completed",
            "totalTokens": 128450,
            "usage": {"input_tokens": 4210, "output_tokens": 9330}
        });

        let subagent = subagent_from_agent_call(
            &json!({"type": "tool_use", "id": "c", "name": "Agent", "input": {}}),
            Some(&result),
        );

        let SourceValue::Recorded(usage) = &subagent.token_usage else {
            panic!("expected recorded tokens");
        };
        assert_eq!(usage.total, SourceValue::Recorded(128450));
        assert_eq!(usage.input, SourceValue::Recorded(4210));
        assert_eq!(usage.scope, "subagent");
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

    #[test]
    fn a_top_level_iso_timestamp_string_is_recorded() {
        let value = json!({
            "type": "user",
            "uuid": "a",
            "sessionId": "s",
            "timestamp": "2026-01-02T12:00:00.000Z",
            "message": {"role": "user", "content": "hello"}
        });
        let parsed = parse_records(record, Harness::ClaudeCode, &[value]);

        assert_eq!(
            parsed.events[0].timestamp,
            SourceValue::Recorded("2026-01-02T12:00:00.000Z".to_string())
        );
    }

    #[test]
    fn a_missing_timestamp_key_is_absent() {
        let value = json!({
            "type": "user",
            "uuid": "a",
            "sessionId": "s",
            "message": {"role": "user", "content": "hello"}
        });
        let parsed = parse_records(record, Harness::ClaudeCode, &[value]);

        assert_eq!(parsed.events[0].timestamp, SourceValue::Absent);
    }

    #[test]
    fn a_numeric_timestamp_is_malformed() {
        let value = json!({
            "type": "user",
            "uuid": "a",
            "sessionId": "s",
            "timestamp": 1735819200000_u64,
            "message": {"role": "user", "content": "hello"}
        });
        let parsed = parse_records(record, Harness::ClaudeCode, &[value]);

        assert_eq!(parsed.events[0].timestamp, SourceValue::Malformed);
    }

    #[test]
    fn sibling_events_from_one_assistant_record_inherit_the_record_timestamp() {
        let stamp = "2026-01-02T12:00:00.000Z";
        let value = json!({
            "type": "assistant",
            "uuid": "a",
            "sessionId": "s",
            "timestamp": stamp,
            "message": {
                "role": "assistant",
                "content": [
                    {"type": "text", "text": "delegating"},
                    {"type": "tool_use", "id": "call-1", "name": "Read", "input": {"file_path": "README.md"}},
                    {"type": "tool_use", "id": "call-2", "name": "Agent", "input": {"subagent_type": "explore", "prompt": "look around"}}
                ]
            }
        });
        let parsed = parse_records(record, Harness::ClaudeCode, &[value]);

        assert!(parsed.events.len() >= 3);
        let expected = SourceValue::Recorded(stamp.to_string());
        for event in &parsed.events {
            assert_eq!(
                event.timestamp, expected,
                "sibling {:?} must inherit the record timestamp",
                event.kind
            );
        }
    }
}
