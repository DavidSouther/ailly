//! Pi JSONL adapter.

use super::*;

/// The delegation Pi recorded, built from an `ailly_subagent` `toolCall` block
/// and the error flag on the result that answered it. Pi names no child, times
/// nothing, and counts no tokens, so those stay Absent.
pub(crate) fn subagent_from_ailly_subagent(block: &Value, is_error: SourceValue<bool>) -> Subagent {
    Subagent {
        agent_type: string_field(find_input_field(block, "arguments", &["reference"])),
        prompt: string_field(find_input_field(block, "arguments", &["task"])),
        outcome: outcome_from_error_flag(is_error),
        ..Subagent::unrecorded()
    }
}

/// Pi records how a delegation ended as a bare error flag and nothing else.
/// The returned report often opens "FAILED: …", but reading that prose would
/// be inference rather than evidence.
fn outcome_from_error_flag(is_error: SourceValue<bool>) -> SourceValue<String> {
    match is_error {
        SourceValue::Recorded(true) => SourceValue::Recorded("error".to_string()),
        SourceValue::Recorded(false) => SourceValue::Recorded("completed".to_string()),
        SourceValue::Absent => SourceValue::Absent,
        SourceValue::Unsupported => SourceValue::Unsupported,
        SourceValue::Malformed => SourceValue::Malformed,
    }
}

/// Folds a delegation's error flag back onto the spawn the same transcript
/// recorded, matched by the tool-call id the result names. Pi spells the flag
/// `isError`, which `loader::tool_result` does not read, so it is read here.
fn link_subagent_outcome(parsed: &mut ParsedSession, message: &Value) {
    let Some(tool_call_id) = recorded_string(message, &["toolCallId"]) else {
        return;
    };
    let named = SourceValue::Recorded(tool_call_id);
    let outcome = outcome_from_error_flag(bool_field(
        message.get("isError").or_else(|| message.get("is_error")),
    ));
    for event in parsed.events.iter_mut() {
        if event.kind != EventKind::SubagentSpawn || event.native_id != named {
            continue;
        }
        if let SourceValue::Recorded(subagent) = &mut event.subagent {
            subagent.outcome = outcome.clone();
        }
    }
}

pub(crate) fn record(value: &Value, parsed: &mut ParsedSession, path: &str, source: Provenance) {
    let record_type = recorded_string(value, &["type"]);

    if record_type.as_deref() == Some("session") {
        // A session header's parentSession is a fork-lineage fact, recorded on
        // the Session itself; it must never be turned into a SubagentSpawn
        // relationship edge.
        parsed.session = Some(Session {
            id: session_id(Harness::Pi, path, recorded_string(value, &["id"])),
            harness: Harness::Pi,
            source,
            native_id: string(value, &["id"]),
            project: string(value, &["cwd"]),
            parent_session: string(value, &["parentSession"]),
        });
        return;
    }

    if parsed.session.is_none() {
        // No "session" header record has been seen yet in this file (it may
        // be truncated, or start mid-stream). Seed a best-effort fallback so
        // every session file still produces exactly one normalized Session,
        // matching Claude's and Codex's unconditional seeding. Nothing here
        // was actually recorded, so every fact stays Absent rather than a
        // guess. If a genuine "session" header record is encountered later
        // in the same file, the branch above overwrites this fallback
        // unconditionally, so the real header still gets identified.
        parsed.session = Some(Session {
            id: session_id(Harness::Pi, path, None),
            harness: Harness::Pi,
            source: source.clone(),
            native_id: SourceValue::Absent,
            project: SourceValue::Absent,
            parent_session: SourceValue::Absent,
        });
    }

    let session = parsed
        .session
        .as_ref()
        .map(|session| session.id.clone())
        .expect("session is seeded immediately above when absent");
    let native = string(value, &["id"]);
    let timestamp = string(value, &["timestamp"]);
    let message = value.get("message");
    let role = message.and_then(|message| recorded_string(message, &["role"]));

    let kind = match record_type.as_deref() {
        Some("message") => match role.as_deref() {
            Some("user") => EventKind::UserTurn,
            Some("assistant") => EventKind::AssistantTurn,
            Some("toolResult") | Some("tool") => EventKind::ToolResult,
            _ => EventKind::Unknown,
        },
        Some("model_change") => EventKind::ModelChange,
        Some("thinking_level_change") => EventKind::ThinkingChange,
        Some("compaction") | Some("branch_summary") => EventKind::Summary,
        _ => EventKind::Unknown,
    };

    let mut base = event(
        &session,
        kind,
        source.clone(),
        native.clone(),
        timestamp.clone(),
    );

    if let Some(message) = message {
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
        if kind == EventKind::ToolResult {
            base.tool_result =
                SourceValue::Recorded(tool_result(message, &["toolCallId"], "content"));
        }
    }

    if kind == EventKind::Unknown {
        // Preserve the raw discriminator so the record is inspectable rather
        // than silently dropped; never fabricate an Absent here.
        base.detail = record_type
            .clone()
            .map(SourceValue::Recorded)
            .unwrap_or(SourceValue::Malformed);
    }

    let base_id = base.id.clone();
    parsed.events.push(base);

    push_relationship_if_present(
        parsed,
        RelationshipKind::ToolCallResult,
        base_id.clone(),
        message.and_then(|message| recorded_string(message, &["toolCallId"])),
        source.clone(),
    );

    if kind == EventKind::ToolResult {
        if let Some(message) = message {
            link_subagent_outcome(parsed, message);
        }
    }

    if let Some(message) = message.filter(|_| role.as_deref() == Some("assistant")) {
        if let Some(blocks) = message.get("content").and_then(Value::as_array) {
            for block in blocks
                .iter()
                .filter(|block| block.get("type").and_then(Value::as_str) == Some("toolCall"))
            {
                // A delegation is an ordinary toolCall block; only its name
                // says so. Pi names no child, so no spawn edge is ever emitted.
                if recorded_string(block, &["name"]).as_deref() == Some("ailly_subagent") {
                    let mut spawn = event(
                        &session,
                        EventKind::SubagentSpawn,
                        source.clone(),
                        string(block, &["id"]),
                        timestamp.clone(),
                    );
                    spawn.subagent = SourceValue::Recorded(subagent_from_ailly_subagent(
                        block,
                        SourceValue::Absent,
                    ));
                    parsed.events.push(spawn);
                    continue;
                }
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
                    "arguments",
                    SourceValue::Absent,
                ));
                parsed.events.push(call);
            }
        }
    }

    // A conversation-tree parent is never the reserved SubagentSpawn variant;
    // Pi records no explicit delegation evidence.
    push_relationship_if_present(
        parsed,
        RelationshipKind::ConversationTreeParent,
        base_id,
        recorded_string(value, &["parentId"]),
        source,
    );
}

#[cfg(test)]
mod tests {
    use super::conformance::{conformance_tests, parse_records, Conformance, ConformanceSpawn};
    use super::*;
    use serde_json::json;

    /// Minimal Pi transcript: a session header record, a user message, an
    /// assistant message carrying a toolCall block, usage, and a parentId
    /// pointing back at the user turn, then a toolResult message linked to
    /// the call. Pi records both edge kinds the normalized model knows.
    fn conformance() -> Conformance {
        Conformance {
            harness: Harness::Pi,
            record,
            transcript: vec![
                json!({"type":"session","version":3,"id":"native-session","cwd":"/tmp/proj"}),
                json!({"type":"message","id":"turn-user","message":{"role":"user","content":"hello"}}),
                json!({
                    "type": "message",
                    "id": "turn-assistant",
                    "parentId": "turn-user",
                    "message": {
                        "role": "assistant",
                        "content": [
                            {"type": "text", "text": "reading a file"},
                            {"type": "toolCall", "id": "call-1", "name": "read", "arguments": {"path": "README.md"}},
                            {"type": "toolCall", "id": "call-2", "name": "bash", "arguments": {"command": "cargo test"}},
                            {"type": "toolCall", "id": "call-3", "name": "ailly_subagent", "arguments": {"reference": "explore", "task": "Map the parser module"}}
                        ],
                        "usage": {"inputTokens": 10, "outputTokens": 2, "totalTokens": 12}
                    }
                }),
                json!({"type":"message","id":"result-1","message":{"role":"toolResult","toolCallId":"call-1","content":"ok"}}),
                json!({"type":"message","id":"result-3","message":{"role":"toolResult","toolCallId":"call-3","content":"mapped","isError":false}}),
            ],
            native_session_id: "native-session",
            project: "/tmp/proj",
            tool_name: "read",
            tool_path: "README.md",
            shell_tool_name: "bash",
            tool_command: "cargo test",
            // Pi records a working directory on the session header and nowhere
            // else; the shared test holds the adapter to that absence.
            tool_cwd: None,
            tool_result_call_id: Some("call-1"),
            tool_result_output: Some("ok"),
            tree_parent_id: Some("turn-user"),
            assistant_usage: Some(TokenUsage {
                input: SourceValue::Recorded(10),
                output: SourceValue::Recorded(2),
                // Not recorded by the pi.jsonl shape; must stay Absent,
                // never a fabricated zero.
                cache_read: SourceValue::Absent,
                cache_write: SourceValue::Absent,
                total: SourceValue::Recorded(12),
                scope: "message".to_string(),
            }),
            // Pi records the delegation and its error flag and nothing else,
            // so this fixture is the "no child was ever named" case: no id, no
            // duration, no tokens, and therefore no spawn edge.
            subagent_spawn: Some(ConformanceSpawn {
                agent_type: "explore",
                prompt: "Map the parser module",
                outcome: Some("completed"),
                duration_ms: None,
                child_native_id: None,
                child_token_total: None,
            }),
        }
    }

    conformance_tests!(conformance);

    // --- Pi-specific quirks ---

    #[test]
    fn parent_session_is_fork_lineage_on_the_header_not_a_subagent_spawn_edge() {
        let value = json!({"type":"session","version":3,"id":"pi-1","cwd":"/work","parentSession":"pi-parent"});
        let parsed = parse_records(record, Harness::Pi, &[value]);

        let session = parsed.session.expect("session header seeded");
        assert_eq!(
            session.parent_session,
            SourceValue::Recorded("pi-parent".to_string())
        );
        // A fork-lineage fact on the session header must never manufacture a
        // SubagentSpawn relationship edge; no adapter constructs that variant.
        assert!(parsed.relationships.is_empty());
        assert!(parsed.events.is_empty());
    }

    #[test]
    fn a_session_headers_cwd_is_never_copied_down_onto_a_tool_call() {
        // Pi records a working directory on the session header and nowhere
        // else. Copying it onto the call would turn a session-level fact into
        // a per-call one the transcript never recorded.
        let values = [
            json!({"type":"session","version":3,"id":"pi-1","cwd":"/Users/dev/repo"}),
            json!({"type":"message","id":"m1","message":{"role":"assistant","content":[
                {"type":"toolCall","id":"call-1","name":"bash","arguments":{"command":"ls -la"}}
            ]}}),
        ];
        let parsed = parse_records(record, Harness::Pi, &values);

        let call_event = parsed
            .events
            .iter()
            .find(|event| event.kind == EventKind::ToolCall)
            .expect("a tool call event");
        let SourceValue::Recorded(call) = &call_event.tool_call else {
            panic!("expected a recorded tool call");
        };
        assert_eq!(call.command, SourceValue::Recorded("ls -la".to_string()));
        assert_eq!(call.cwd, SourceValue::Absent);
        assert_eq!(
            parsed.session.expect("session header").project,
            SourceValue::Recorded("/Users/dev/repo".to_string())
        );
    }

    /// Pi's only outcome evidence is a boolean, written camelCase. Both of its
    /// values are recorded facts; neither is read from the report's prose.
    #[test]
    fn both_settings_of_the_results_error_flag_resolve_a_recorded_outcome() {
        for (flag, expected) in [(true, "error"), (false, "completed")] {
            let values = [
                json!({"type":"message","id":"m1","message":{"role":"assistant","content":[
                    {"type":"toolCall","id":"call-1","name":"ailly_subagent","arguments":{"reference":"research","task":"Trace the spec"}}
                ]}}),
                json!({"type":"message","id":"r1","message":{"role":"toolResult","toolCallId":"call-1","content":"done","isError":flag}}),
            ];
            let parsed = parse_records(record, Harness::Pi, &values);

            let spawn = parsed
                .events
                .iter()
                .find(|event| event.kind == EventKind::SubagentSpawn)
                .expect("a spawn event");
            let SourceValue::Recorded(subagent) = &spawn.subagent else {
                panic!("expected a recorded subagent");
            };
            assert_eq!(
                subagent.outcome,
                SourceValue::Recorded(expected.to_string()),
                "isError {flag} should record outcome {expected}"
            );
            assert_eq!(
                subagent.agent_type,
                SourceValue::Recorded("research".to_string())
            );
            // Pi never names a child, so it never emits a spawn edge.
            assert!(parsed
                .relationships
                .iter()
                .all(|edge| edge.kind != RelationshipKind::SubagentSpawn));
        }
    }

    #[test]
    fn a_delegation_with_no_result_records_no_outcome_rather_than_success() {
        let value = json!({"type":"message","id":"m1","message":{"role":"assistant","content":[
            {"type":"toolCall","id":"call-1","name":"ailly_subagent","arguments":{"reference":"research","task":"Trace the spec"}}
        ]}});
        let parsed = parse_records(record, Harness::Pi, &[value]);

        let spawn = parsed
            .events
            .iter()
            .find(|event| event.kind == EventKind::SubagentSpawn)
            .expect("a spawn event");
        let SourceValue::Recorded(subagent) = &spawn.subagent else {
            panic!("expected a recorded subagent");
        };
        assert_eq!(subagent.outcome, SourceValue::Absent);
        assert_eq!(subagent.duration_ms, SourceValue::Absent);
        assert_eq!(subagent.token_usage, SourceValue::Absent);
    }

    #[test]
    fn message_role_selects_the_matching_turn_or_tool_result_kind() {
        let cases = [
            ("user", EventKind::UserTurn),
            ("assistant", EventKind::AssistantTurn),
            ("toolResult", EventKind::ToolResult),
            // Older pi files spell the result role "tool".
            ("tool", EventKind::ToolResult),
        ];
        for (role, expected_kind) in cases {
            let value =
                json!({"type":"message","id":"m1","message":{"role":role,"content":"hello"}});
            let parsed = parse_records(record, Harness::Pi, &[value]);
            assert_eq!(
                parsed.events[0].kind, expected_kind,
                "role {role} should select {expected_kind:?}"
            );
        }
    }

    #[test]
    fn typed_record_types_select_their_own_event_kind_not_unknown() {
        let cases = [
            ("model_change", EventKind::ModelChange),
            ("thinking_level_change", EventKind::ThinkingChange),
            ("compaction", EventKind::Summary),
            ("branch_summary", EventKind::Summary),
        ];
        for (record_type, expected_kind) in cases {
            let value = json!({"type": record_type, "id": "e1"});
            let parsed = parse_records(record, Harness::Pi, &[value]);
            assert_eq!(
                parsed.events[0].kind, expected_kind,
                "{record_type} should select {expected_kind:?}, not Unknown"
            );
        }
    }

    #[test]
    fn an_unrecognized_type_becomes_an_unknown_event_carrying_the_raw_type_string() {
        let value = json!({"type":"future_entry","id":"unknown-1"});
        let parsed = parse_records(record, Harness::Pi, &[value]);

        assert_eq!(parsed.events.len(), 1);
        assert_eq!(parsed.events[0].kind, EventKind::Unknown);
        // Not dropped, and not Absent: the raw discriminator is preserved.
        assert_eq!(
            parsed.events[0].detail,
            SourceValue::Recorded("future_entry".to_string())
        );
    }

    #[test]
    fn a_non_header_record_seeds_a_fallback_session_when_no_header_was_ever_seen() {
        // No prior "session" record has been seen (parsed.session starts None,
        // per ParsedSession::default) — e.g. a truncated file, or one that
        // starts mid-stream. design.md requires one normalized Session per
        // session file even then.
        let value = json!({"type":"message","id":"m1","message":{"role":"user","content":"hi"}});
        let parsed = parse_records(record, Harness::Pi, &[value]);

        let session = parsed.session.expect("fallback session seeded");
        assert_eq!(session.harness, Harness::Pi);
        // Deterministic, keyed on harness + path, matching how the rest of
        // record() already resolves a session id when parsed.session is None.
        assert_eq!(session.id, session_id(Harness::Pi, "fixture.jsonl", None));
        // Nothing was actually recorded for these facts, so they stay Absent
        // rather than being fabricated from the record that happened to
        // trigger the fallback.
        assert_eq!(session.native_id, SourceValue::Absent);
        assert_eq!(session.project, SourceValue::Absent);
        assert_eq!(session.parent_session, SourceValue::Absent);
    }

    #[test]
    fn a_top_level_iso_timestamp_string_is_recorded() {
        let value = json!({
            "type": "message",
            "id": "m1",
            "timestamp": "2026-01-02T12:00:00.000Z",
            "message": {"role": "user", "content": "hello"}
        });
        let parsed = parse_records(record, Harness::Pi, &[value]);

        assert_eq!(
            parsed.events[0].timestamp,
            SourceValue::Recorded("2026-01-02T12:00:00.000Z".to_string())
        );
    }

    #[test]
    fn a_missing_timestamp_key_is_absent() {
        let value = json!({
            "type": "message",
            "id": "m1",
            "message": {"role": "user", "content": "hello"}
        });
        let parsed = parse_records(record, Harness::Pi, &[value]);

        assert_eq!(parsed.events[0].timestamp, SourceValue::Absent);
    }

    #[test]
    fn a_numeric_timestamp_is_malformed() {
        // Pi sometimes stamps top-level epoch millis; that is present-but-not-a-string.
        let value = json!({
            "type": "message",
            "id": "m1",
            "timestamp": 1735819200000_u64,
            "message": {"role": "user", "content": "hello"}
        });
        let parsed = parse_records(record, Harness::Pi, &[value]);

        assert_eq!(parsed.events[0].timestamp, SourceValue::Malformed);
    }
}
