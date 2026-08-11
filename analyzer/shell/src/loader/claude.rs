//! Claude Code JSONL adapter.

use super::*;

pub(crate) fn record(value: &Value, parsed: &mut ParsedSession, path: &str, source: Provenance) {
    let record_type = recorded_string(value, &["type"]);
    let native = string(value, &["uuid"]);
    let session = session_id(
        Harness::ClaudeCode,
        path,
        value
            .get("sessionId")
            .and_then(Value::as_str)
            .map(str::to_owned),
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
    base.timestamp = string(value, &["timestamp"]);
    base.turn = role
        .map(|role| {
            SourceValue::Recorded(Turn {
                text: content_text(message.get("content")),
                role,
            })
        })
        .unwrap_or(SourceValue::Absent);
    base.token_usage = usage(message.get("usage"), "message");
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
    if let Some(parent) = recorded_string(value, &["parentUuid"]) {
        let id = event_id(&session, &source, &native);
        parsed.relationships.push(Relationship {
            kind: RelationshipKind::ConversationTreeParent,
            from_event_id: id,
            to_native_id: parent,
            source,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn imports_parent_and_tool_evidence_from_an_assistant_record() {
        let value = json!({"type":"assistant","uuid":"a","parentUuid":"u","sessionId":"s","message":{"role":"assistant","content":[{"type":"tool_use","id":"call","name":"Read","input":{"file_path":"README.md"}}]}});
        let mut parsed = ParsedSession::default();
        record(
            &value,
            &mut parsed,
            "fixture.jsonl",
            Provenance {
                harness: Harness::ClaudeCode,
                path: "fixture.jsonl".into(),
                line: 1,
                ordinal: 1,
            },
        );
        assert!(parsed
            .events
            .iter()
            .any(|event| event.kind == EventKind::ToolCall));
        assert_eq!(
            parsed.relationships[0].kind,
            RelationshipKind::ConversationTreeParent
        );
    }
}
