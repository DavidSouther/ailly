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

/// Codex reports usage in a `token_count` event_msg rather than on the turn it
/// measured, so this is the only place its spend can be read.
///
/// `info` holds two figures: `last_token_usage`, the delta this turn spent, and
/// `total_token_usage`, a running cumulative over the whole session. Only the
/// delta is read — folding the cumulative would count every earlier turn again
/// at each record. A record carrying only the cumulative therefore records no
/// usage, rather than an inflated one.
///
/// The record is kept as an `Unknown` event named by its `detail` instead of
/// being merged onto the neighbouring assistant turn: nothing in the record says
/// which turn it measured, and a guess there would silently misattribute spend.
/// `info` has exactly three keys and none of them names a model, verified over
/// 48,244 local records, so the model has to be carried in from whichever
/// earlier record named one. Absent when nothing has yet, which downstream
/// reads as "no price for this record" rather than as a default model.
fn token_count(
    session: &str,
    payload: &Value,
    source: Provenance,
    value: &Value,
    model: SourceValue<String>,
) -> crate::model::Event {
    let mut counted = event(
        session,
        EventKind::Unknown,
        source,
        SourceValue::Absent,
        string(value, &["timestamp"]),
    );
    counted.detail = SourceValue::Recorded("token_count".to_string());
    counted.model = model;
    counted.token_usage = usage(
        payload
            .get("info")
            .and_then(|info| info.get("last_token_usage")),
        "turn",
    );
    counted
}

/// The model a `turn_context` named. Codex writes it twice — at the top of the
/// payload and again under the collaboration mode's settings — so the nested
/// copy is read only when the first is missing.
fn turn_context_model(payload: &Value) -> SourceValue<String> {
    match string(payload, &["model"]) {
        SourceValue::Absent => payload
            .get("collaboration_mode")
            .and_then(|mode| mode.get("settings"))
            .map(|settings| string(settings, &["model"]))
            .unwrap_or(SourceValue::Absent),
        named => named,
    }
}

pub(crate) fn record(
    value: &Value,
    parsed: &mut ParsedSession,
    state: &mut AdapterState,
    path: &str,
    source: Provenance,
) {
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

    let session = parsed
        .session
        .as_ref()
        .expect("session seeded above")
        .id
        .clone();

    // Codex reports usage on a record that names no model, and names the model
    // on records that report no usage, so the two are matched by file order:
    // each `token_count` takes the model most recently named before it. This
    // record produces no event of its own — it states the turn's configuration,
    // not something that happened.
    if record_type.as_deref() == Some("turn_context") {
        state.name_model(turn_context_model(payload));
        return;
    }

    if record_type.as_deref() == Some("event_msg") {
        match recorded_string(payload, &["type"]).as_deref() {
            // A thread-scoped default, written before the first turn, which is
            // the only model 44 of 804 local transcripts name before their
            // first `token_count`.
            Some("thread_settings_applied") => state.name_model(string(
                payload.get("thread_settings").unwrap_or(payload),
                &["model"],
            )),
            Some("token_count") => parsed.events.push(token_count(
                &session,
                payload,
                source,
                value,
                state.carried_model(),
            )),
            _ => {}
        }
        return;
    }

    if record_type.as_deref() != Some("response_item") {
        return;
    }
    let payload_type = recorded_string(payload, &["type"]);
    let native = string(payload, &["id", "call_id"]);
    let timestamp = string(value, &["timestamp"]);

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
                timestamp,
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
            let mut spawn = event(
                &session,
                EventKind::SubagentSpawn,
                source,
                native,
                timestamp,
            );
            spawn.subagent = SourceValue::Recorded(subagent_from_spawn_agent(payload, None));
            parsed.events.push(spawn);
        }
        Some("function_call") => {
            let mut call = event(&session, EventKind::ToolCall, source, native, timestamp);
            call.tool_call =
                SourceValue::Recorded(tool_call(payload, "name", "arguments", SourceValue::Absent));
            parsed.events.push(call);
        }
        // A custom tool call carries its payload as a JavaScript snippet under
        // `input` rather than JSON under `arguments`. It still records the tool
        // that ran, so it surfaces as a call; whatever the snippet says stays
        // uninterpreted in `input`.
        Some("custom_tool_call") => {
            let mut call = event(&session, EventKind::ToolCall, source, native, timestamp);
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
                timestamp,
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
                // The model rides here, on a record that reports no usage, not
                // on the `token_count` at the end that reports all of it.
                json!({"type":"turn_context","payload":{"turn_id":"turn-1","model":"gpt-5.6-terra","collaboration_mode":{"mode":"default","settings":{"model":"gpt-5.6-terra"}}}}),
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
                // Codex reports usage only here, in an event_msg, never on a
                // response_item. `last_token_usage` is the per-turn delta;
                // `total_token_usage` beside it is a running cumulative.
                json!({"type":"event_msg","payload":{"type":"token_count","info":{
                    "total_token_usage": {"input_tokens":9000,"cached_input_tokens":5000,"cache_write_input_tokens":1200,"output_tokens":350,"reasoning_output_tokens":120,"total_tokens":9350},
                    "last_token_usage": {"input_tokens":5000,"cached_input_tokens":3000,"cache_write_input_tokens":800,"output_tokens":200,"reasoning_output_tokens":90,"total_tokens":5200},
                    "model_context_window": 258400
                }}}),
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
            // The turn's model is named on a `turn_context` record, so it must
            // not be copied onto the assistant turn beside it: nothing on that
            // record says which model produced it.
            assistant_model: None,
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

    /// A `turn_context` states the turn's configuration rather than reporting
    /// something that happened, so it names the model without becoming an event.
    #[test]
    fn a_turn_context_names_the_model_without_producing_an_event() {
        let value =
            json!({"type":"turn_context","payload":{"turn_id":"t","model":"gpt-5.6-terra"}});
        let parsed = parse_records(record, Harness::Codex, &[value]);

        assert!(parsed.events.is_empty());
    }

    /// The usage-bearing record names no model, so the price attributed to it
    /// rests on the last model named before it in the file.
    #[test]
    fn a_token_count_carries_the_model_the_most_recent_turn_context_named() {
        let parsed = parse_records(record, Harness::Codex, &conformance().transcript);
        let counted = parsed
            .events
            .iter()
            .find(|event| matches!(event.token_usage, SourceValue::Recorded(_)))
            .expect("the usage-bearing event");

        assert_eq!(
            counted.model,
            SourceValue::Recorded("gpt-5.6-terra".to_string())
        );
        // Codex writes no dollar figure, so its price can only ever be the
        // client's catalog estimate over these buckets and this model.
        let SourceValue::Recorded(usage) = &counted.token_usage else {
            panic!("expected recorded usage");
        };
        assert_eq!(usage.cost_total_micros, SourceValue::Absent);
    }

    /// 3 of 808 local transcripts change model mid-session, always to a
    /// different model on the first turn than every turn after. Each turn's
    /// usage must take the model in force when it was measured, not the file's
    /// first or most common one.
    #[test]
    fn usage_after_a_mid_session_model_change_carries_the_newer_model() {
        let counted = json!({"type":"event_msg","payload":{"type":"token_count","info":{
            "last_token_usage": {"input_tokens":100,"output_tokens":10,"total_tokens":110}
        }}});
        let values = [
            json!({"type":"turn_context","payload":{"turn_id":"t1","model":"gpt-5.6-luna"}}),
            counted.clone(),
            json!({"type":"turn_context","payload":{"turn_id":"t2","model":"gpt-5.6-terra"}}),
            counted,
        ];
        let parsed = parse_records(record, Harness::Codex, &values);

        let models: Vec<&SourceValue<String>> =
            parsed.events.iter().map(|event| &event.model).collect();
        assert_eq!(
            models,
            vec![
                &SourceValue::Recorded("gpt-5.6-luna".to_string()),
                &SourceValue::Recorded("gpt-5.6-terra".to_string()),
            ]
        );
    }

    /// A thread-scoped default is written before the first turn, and is the
    /// only model named ahead of the first `token_count` in 44 of 804 local
    /// transcripts.
    #[test]
    fn a_thread_settings_default_names_the_model_when_no_turn_context_has_yet() {
        let values = [
            json!({"type":"event_msg","payload":{"type":"thread_settings_applied","thread_settings":{"model":"gpt-5.6-terra","service_tier":"priority"}}}),
            json!({"type":"event_msg","payload":{"type":"token_count","info":{
                "last_token_usage": {"input_tokens":100,"output_tokens":10,"total_tokens":110}
            }}}),
        ];
        let parsed = parse_records(record, Harness::Codex, &values);

        assert_eq!(
            parsed.events[0].model,
            SourceValue::Recorded("gpt-5.6-terra".to_string())
        );
    }

    /// Usage reached before any model was named leaves the model unrecorded.
    /// Borrowing the session's most common model would fabricate the one fact a
    /// price estimate rests on.
    #[test]
    fn a_token_count_reached_before_any_model_was_named_carries_none() {
        let value = json!({"type":"event_msg","payload":{"type":"token_count","info":{
            "last_token_usage": {"input_tokens":100,"output_tokens":10,"total_tokens":110}
        }}});
        let parsed = parse_records(record, Harness::Codex, &[value]);

        assert_eq!(parsed.events[0].model, SourceValue::Absent);
    }

    /// The nested copy under the collaboration mode's settings is the same
    /// fact, and is all a payload missing the top-level key has.
    #[test]
    fn a_turn_context_with_only_a_nested_model_still_names_it() {
        let values = [
            json!({"type":"turn_context","payload":{"turn_id":"t","collaboration_mode":{"settings":{"model":"gpt-5.6-sol"}}}}),
            json!({"type":"event_msg","payload":{"type":"token_count","info":{
                "last_token_usage": {"input_tokens":1,"output_tokens":1,"total_tokens":2}
            }}}),
        ];
        let parsed = parse_records(record, Harness::Codex, &values);

        assert_eq!(
            parsed.events[0].model,
            SourceValue::Recorded("gpt-5.6-sol".to_string())
        );
    }

    /// The one usage-bearing event in the conformance transcript above, which is
    /// an `event_msg` rather than a `response_item`.
    fn only_usage(parsed: &ParsedSession) -> TokenUsage {
        let bearing: Vec<&Event> = parsed
            .events
            .iter()
            .filter(|event| matches!(event.token_usage, SourceValue::Recorded(_)))
            .collect();
        assert_eq!(bearing.len(), 1, "expected exactly one usage-bearing event");
        match &bearing[0].token_usage {
            SourceValue::Recorded(usage) => usage.clone(),
            other => panic!("expected recorded usage, got {other:?}"),
        }
    }

    /// Codex's key names differ from the ones the shared reader used to look
    /// for, so its usage read as nothing at all. Values are stored raw: no
    /// per-harness subtraction happens in the adapter.
    #[test]
    fn a_token_count_event_msg_records_the_per_turn_delta_raw() {
        let parsed = parse_records(record, Harness::Codex, &conformance().transcript);

        let usage = only_usage(&parsed);
        assert_eq!(usage.input, SourceValue::Recorded(5000));
        assert_eq!(usage.cache_read, SourceValue::Recorded(3000));
        assert_eq!(usage.cache_write, SourceValue::Recorded(800));
        assert_eq!(usage.output, SourceValue::Recorded(200));
        // The delta's own total, never the cumulative 9350 beside it.
        assert_eq!(usage.total, SourceValue::Recorded(5200));
    }

    /// The membership question the design flagged as unverified, settled here.
    ///
    /// Measured against every local Codex transcript (48,240 usage records
    /// across 809 files): 48,012 satisfy `input_tokens + output_tokens ==
    /// total_tokens` with a non-zero `cached_input_tokens`, so cache read sits
    /// *inside* `input_tokens`. Not one record had a non-zero
    /// `cache_write_input_tokens`, so the real data cannot speak to that field;
    /// this fixture encodes the continuation of the invariant the 48,012 prove,
    /// which is that both named sub-breakdowns are inside their parent — the
    /// same relation `reasoning_output_tokens` has to `output_tokens`.
    #[test]
    fn codex_cache_read_and_cache_write_both_sit_inside_its_input_tokens() {
        let parsed = parse_records(record, Harness::Codex, &conformance().transcript);
        let usage = only_usage(&parsed);
        let recorded = |value: &SourceValue<u64>| match value {
            SourceValue::Recorded(number) => *number,
            other => panic!("expected a recorded figure, got {other:?}"),
        };

        let raw_input = recorded(&usage.input);
        let output = recorded(&usage.output);
        let cache_read = recorded(&usage.cache_read);
        let cache_write = recorded(&usage.cache_write);
        let total = recorded(&usage.total);

        // Nothing outside `input_tokens` and `output_tokens` is added to reach
        // the total, which is only true if both cache figures are already
        // counted inside `input_tokens`.
        assert_eq!(raw_input + output, total);
        // So the disjoint buckets the client folds are these, and they still
        // sum to the figure the harness itself recorded.
        let fresh_input = raw_input - cache_read - cache_write;
        assert_eq!(fresh_input, 1200);
        assert_eq!(fresh_input + output + cache_read + cache_write, total);
    }

    /// A record carrying only the cumulative figure resolves the event's usage
    /// Absent. Summing `total_token_usage` across records would grow
    /// quadratically with the session.
    #[test]
    fn a_token_count_record_with_no_last_usage_records_no_usage_at_all() {
        let value = json!({"type":"event_msg","payload":{"type":"token_count","info":{
            "total_token_usage": {"input_tokens":9000,"cached_input_tokens":5000,"output_tokens":350,"total_tokens":9350}
        }}});
        let parsed = parse_records(record, Harness::Codex, &[value]);

        assert_eq!(parsed.events.len(), 1);
        assert_eq!(parsed.events[0].token_usage, SourceValue::Absent);
        // The record is still preserved rather than dropped, so it stays
        // inspectable beside the turns it measured.
        assert_eq!(
            parsed.events[0].detail,
            SourceValue::Recorded("token_count".to_string())
        );
    }

    /// Codex writes no response identity, so every one of its usage records
    /// counts once downstream rather than collapsing into a neighbour.
    #[test]
    fn codex_records_no_response_identity() {
        let parsed = parse_records(record, Harness::Codex, &conformance().transcript);

        for event in &parsed.events {
            assert_eq!(event.response_id, SourceValue::Absent);
        }
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

    #[test]
    fn a_top_level_iso_timestamp_string_is_recorded() {
        let value = json!({
            "type": "response_item",
            "timestamp": "2026-01-02T12:00:00.000Z",
            "payload": {"type": "message", "id": "turn-user", "role": "user", "content": "hello"}
        });
        let parsed = parse_records(record, Harness::Codex, &[value]);

        assert_eq!(
            parsed.events[0].timestamp,
            SourceValue::Recorded("2026-01-02T12:00:00.000Z".to_string())
        );
    }

    #[test]
    fn a_missing_timestamp_key_is_absent() {
        let value = json!({
            "type": "response_item",
            "payload": {"type": "message", "id": "turn-user", "role": "user", "content": "hello"}
        });
        let parsed = parse_records(record, Harness::Codex, &[value]);

        assert_eq!(parsed.events[0].timestamp, SourceValue::Absent);
    }

    #[test]
    fn a_numeric_timestamp_is_malformed() {
        let value = json!({
            "type": "response_item",
            "timestamp": 1735819200000_u64,
            "payload": {"type": "message", "id": "turn-user", "role": "user", "content": "hello"}
        });
        let parsed = parse_records(record, Harness::Codex, &[value]);

        assert_eq!(parsed.events[0].timestamp, SourceValue::Malformed);
    }
}
