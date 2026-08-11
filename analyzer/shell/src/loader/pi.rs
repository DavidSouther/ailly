//! Pi JSONL adapter.

use super::*;

pub(crate) fn record(value: &Value, parsed: &mut ParsedSession, path: &str, source: Provenance) {
    let entry_type = recorded_string(value, &["type"]);
    if entry_type.as_deref() == Some("session") {
        let native = string(value, &["id"]);
        let native_text = recorded_string(value, &["id"]);
        let session = session_id(Harness::Pi, path, native_text);
        parsed.session = Some(Session {
            id: session,
            harness: Harness::Pi,
            source,
            native_id: native,
            project: string(value, &["cwd"]),
            parent_session: string(value, &["parentSession"]),
        });
        return;
    }
    let session = parsed
        .session
        .as_ref()
        .map(|item| item.id.clone())
        .unwrap_or_else(|| session_id(Harness::Pi, path, None));
    let native = string(value, &["id"]);
    let kind = match entry_type.as_deref() {
        Some("message") => match value
            .get("message")
            .and_then(|message| message.get("role"))
            .and_then(Value::as_str)
        {
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
    let mut entry = event(&session, kind, source.clone(), native.clone());
    entry.timestamp = string(value, &["timestamp"]);
    if entry_type.as_deref() == Some("message") {
        let message = value.get("message").unwrap_or(value);
        entry.turn = recorded_string(message, &["role"])
            .map(|role| {
                SourceValue::Recorded(Turn {
                    role,
                    text: content_text(message.get("content")),
                })
            })
            .unwrap_or(SourceValue::Absent);
        entry.token_usage = usage(message.get("usage"), "message");
        if message.get("role").and_then(Value::as_str) == Some("assistant") {
            if let Some(blocks) = message.get("content").and_then(Value::as_array) {
                for block in blocks
                    .iter()
                    .filter(|block| block.get("type").and_then(Value::as_str) == Some("toolCall"))
                {
                    let mut call = event(
                        &session,
                        EventKind::ToolCall,
                        source.clone(),
                        string(block, &["id"]),
                    );
                    call.tool_call = SourceValue::Recorded(tool_call(block, "name", "arguments"));
                    parsed.events.push(call);
                }
            }
        }
        if let Some(call_id) = recorded_string(message, &["toolCallId"]) {
            parsed.relationships.push(Relationship {
                kind: RelationshipKind::ToolCallResult,
                from_event_id: entry.id.clone(),
                to_native_id: call_id,
                source: source.clone(),
            });
        }
    }
    if kind == EventKind::Unknown {
        entry.detail = entry_type
            .map(SourceValue::Recorded)
            .unwrap_or(SourceValue::Absent);
    }
    if let Some(parent) = recorded_string(value, &["parentId"]) {
        parsed.relationships.push(Relationship {
            kind: RelationshipKind::ConversationTreeParent,
            from_event_id: entry.id.clone(),
            to_native_id: parent,
            source: source.clone(),
        });
    }
    parsed.events.push(entry);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn records_pi_tree_edges_without_calling_them_subagents() {
        let value = json!({"type":"message","id":"child","parentId":"parent","message":{"role":"user","content":"hello"}});
        let mut parsed = ParsedSession::default();
        record(
            &value,
            &mut parsed,
            "fixture.jsonl",
            Provenance {
                harness: Harness::Pi,
                path: "fixture.jsonl".into(),
                line: 1,
                ordinal: 1,
            },
        );
        assert_eq!(
            parsed.relationships[0].kind,
            RelationshipKind::ConversationTreeParent
        );
        assert_ne!(
            parsed.relationships[0].kind,
            RelationshipKind::SubagentSpawn
        );
    }
}
