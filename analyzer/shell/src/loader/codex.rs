//! Codex JSONL adapter.

use super::*;

pub(crate) fn record(value: &Value, parsed: &mut ParsedSession, path: &str, source: Provenance) {
    let kind = recorded_string(value, &["type"]);
    let payload = value.get("payload").unwrap_or(value);
    let session_native = recorded_string(payload, &["id", "session_id"]);
    let session = parsed
        .session
        .as_ref()
        .map(|existing| existing.id.clone())
        .unwrap_or_else(|| session_id(Harness::Codex, path, session_native.clone()));
    if parsed.session.is_none() {
        parsed.session = Some(Session {
            id: session.clone(),
            harness: Harness::Codex,
            source: source.clone(),
            native_id: session_native
                .map(SourceValue::Recorded)
                .unwrap_or(SourceValue::Absent),
            project: string(payload, &["cwd"]),
            parent_session: SourceValue::Absent,
        });
    }
    let item_type = recorded_string(payload, &["type"]);
    let event_kind = match (kind.as_deref(), item_type.as_deref()) {
        (Some("response_item"), Some("message"))
            if payload.get("role").and_then(Value::as_str) == Some("user") =>
        {
            EventKind::UserTurn
        }
        (Some("response_item"), Some("message")) => EventKind::AssistantTurn,
        (Some("response_item"), Some("function_call")) => EventKind::ToolCall,
        (Some("response_item"), Some("function_call_output")) => EventKind::ToolResult,
        _ => return,
    };
    let native = string(payload, &["id", "call_id"]);
    let mut entry = event(&session, event_kind, source.clone(), native.clone());
    entry.timestamp = string(value, &["timestamp"]);
    if matches!(entry.kind, EventKind::UserTurn | EventKind::AssistantTurn) {
        entry.turn = recorded_string(payload, &["role"])
            .map(|role| {
                SourceValue::Recorded(Turn {
                    role,
                    text: content_text(payload.get("content")),
                })
            })
            .unwrap_or(SourceValue::Absent);
    }
    if entry.kind == EventKind::ToolCall {
        entry.tool_call = SourceValue::Recorded(tool_call(payload, "name", "arguments"));
    }
    parsed.events.push(entry);
    if event_kind == EventKind::ToolResult {
        if let Some(call_id) = recorded_string(payload, &["call_id"]) {
            parsed.relationships.push(Relationship {
                kind: RelationshipKind::ToolCallResult,
                from_event_id: event_id(&session, &source, &native),
                to_native_id: call_id,
                source,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn links_function_output_to_the_recorded_call_id() {
        let value = json!({"type":"response_item","payload":{"type":"function_call_output","call_id":"call-1","output":"ok"}});
        let mut parsed = ParsedSession::default();
        record(
            &value,
            &mut parsed,
            "fixture.jsonl",
            Provenance {
                harness: Harness::Codex,
                path: "fixture.jsonl".into(),
                line: 1,
                ordinal: 1,
            },
        );
        assert_eq!(parsed.events[0].kind, EventKind::ToolResult);
        assert_eq!(
            parsed.relationships[0].kind,
            RelationshipKind::ToolCallResult
        );
        assert_eq!(parsed.relationships[0].to_native_id, "call-1");
    }
}
