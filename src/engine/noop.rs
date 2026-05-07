use std::fmt::Write;
use std::sync::Arc;

use rig::OneOrMany;
use rig::message::{
    AssistantContent, Message, Text as RigText, ToolCall, ToolFunction, ToolResult,
    ToolResultContent, UserContent,
};
use rig::tool::ToolDyn;

use crate::content::PreambleBlock;
use crate::engine::prelude::{PreludeExchange, SentEnvelope, tools_exchange};
use crate::engine::{
    Engine, EngineEvent, EngineInput, EngineResponse, EngineStream, ModelId, Settings, StopReason,
};

pub const DEFAULT_CHUNK_BYTES: usize = 32;
const TOOL_CALL_ID: &str = "call_1";

pub struct Noop {
    pub chunk: usize,
    pub override_response: Option<String>,
}

impl Default for Noop {
    fn default() -> Self {
        Self {
            chunk: DEFAULT_CHUNK_BYTES,
            override_response: std::env::var("AILLY_NOOP_RESPONSE").ok(),
        }
    }
}

impl Engine for Noop {
    fn name(&self) -> &'static str {
        "noop"
    }

    fn stream(
        &self,
        input: EngineInput,
        _settings: &Settings,
        tools: &[Arc<dyn ToolDyn>],
        request_label: &str,
    ) -> anyhow::Result<EngineStream> {
        let chunk = self.chunk;
        let override_text = self.override_response.clone();
        let request_label = request_label.to_string();
        let tools: Vec<Arc<dyn ToolDyn>> = tools.to_vec();

        let engine_name = super::EngineName("noop".to_string());
        let model_id = ModelId("noop".to_string());

        let last_user_text = input
            .history
            .iter()
            .rev()
            .find(|m| matches!(m, Message::User { .. }))
            .map(message_text)
            .unwrap_or_default();
        let directive = parse_use_directive(&last_user_text);
        let matched_tool = directive
            .as_ref()
            .and_then(|d| tools.iter().find(|t| t.name() == d.tool).cloned());

        let prelude_exchanges: Vec<PreludeExchange> = (&input.preamble).into();
        let envelope_tools = tools.clone();

        Ok(Box::pin(async_stream::stream! {
            let mut tool_defs: Vec<rig::completion::request::ToolDefinition> =
                Vec::with_capacity(envelope_tools.len());
            for tool in &envelope_tools {
                tool_defs.push(tool.definition(String::new()).await);
            }
            let mut prelude = prelude_exchanges;
            if let Some(extra) = tools_exchange(&tool_defs) {
                prelude.push(extra);
            }
            yield EngineEvent::Envelope(SentEnvelope {
                prelude,
                tools: tool_defs,
            });

            if override_text.is_none()
                && let Some(directive) = directive
                && let Some(tool) = matched_tool
            {
                let pre = format!("using {}. ", directive.tool);
                for piece in split_into_chunks(&pre, chunk) {
                    yield EngineEvent::Text(piece);
                }

                let arguments: serde_json::Value = serde_json::from_str(&directive.args)
                    .unwrap_or(serde_json::Value::Null);
                let call = ToolCall::new(
                    TOOL_CALL_ID.to_string(),
                    ToolFunction::new(directive.tool.clone(), arguments),
                );
                yield EngineEvent::ToolCall(call);

                let tool_text = match tool.call(directive.args.clone()).await {
                    Ok(s) => s,
                    Err(e) => format!("TOOL FAILED {e}"),
                };
                yield EngineEvent::ToolResult(ToolResult {
                    id: TOOL_CALL_ID.to_string(),
                    call_id: None,
                    content: OneOrMany::one(ToolResultContent::Text(RigText {
                        text: tool_text,
                    })),
                });

                let post = "done.".to_string();
                for piece in split_into_chunks(&post, chunk) {
                    yield EngineEvent::Text(piece);
                }

                yield EngineEvent::Final(EngineResponse {
                    text: format!("{pre}{post}"),
                    model_id,
                    stop_reason: StopReason::EndTurn,
                    usage: None,
                    engine_name,
                });
                return;
            }

            let text = match &override_text {
                Some(s) => s.clone(),
                None => build_envelope(&request_label, &input),
            };
            for piece in split_into_chunks(&text, chunk) {
                yield EngineEvent::Text(piece);
            }
            yield EngineEvent::Final(EngineResponse {
                text,
                model_id,
                stop_reason: StopReason::EndTurn,
                usage: None,
                engine_name,
            });
        }))
    }
}

struct UseDirective {
    tool: String,
    args: String,
}

fn parse_use_directive(text: &str) -> Option<UseDirective> {
    let start = text.find("USE ")?;
    let after = &text[start + "USE ".len()..];
    let with_pos = after.find(" WITH ")?;
    let tool = after[..with_pos].trim().to_string();
    if tool.is_empty() {
        return None;
    }
    let after_with = &after[with_pos + " WITH ".len()..];
    let end = after_with.find('\n').unwrap_or(after_with.len());
    let args = after_with[..end].trim().to_string();
    Some(UseDirective { tool, args })
}

fn build_envelope(request_label: &str, input: &EngineInput) -> String {
    let mut out = String::new();
    writeln!(out, "noop response for {request_label}:").unwrap();
    for (i, block) in input.preamble.blocks.iter().enumerate() {
        match block {
            PreambleBlock::InheritedSystem { text } => {
                writeln!(out, "[preamble {i}] inherited: {text}").unwrap();
            }
            PreambleBlock::Skill(skill) => {
                writeln!(
                    out,
                    "[preamble {i}] skill {name}: {body}",
                    name = skill.name().as_str(),
                    body = skill.body().as_str()
                )
                .unwrap();
            }
            PreambleBlock::LocalSystem { text } => {
                writeln!(out, "[preamble {i}] local: {text}").unwrap();
            }
        }
    }
    out.push_str("[history follows]\n");
    for (i, m) in input.history.iter().enumerate() {
        writeln!(out, "[message {i}] {}: {}", role_str(m), message_text(m)).unwrap();
    }
    let last_user = input
        .history
        .iter()
        .rev()
        .find(|m| matches!(m, Message::User { .. }))
        .map(message_text)
        .unwrap_or_default();
    write!(out, "[response] {last_user}").unwrap();
    out
}

fn role_str(m: &Message) -> &'static str {
    match m {
        Message::System { .. } => "system",
        Message::User { .. } => "user",
        Message::Assistant { .. } => "assistant",
    }
}

fn message_text(m: &Message) -> String {
    match m {
        Message::System { content } => content.clone(),
        Message::User { content } => match content.first() {
            UserContent::Text(t) => t.text.clone(),
            _ => String::new(),
        },
        Message::Assistant { content, .. } => match content.first() {
            AssistantContent::Text(t) => t.text.clone(),
            _ => String::new(),
        },
    }
}

fn split_into_chunks(text: &str, chunk: usize) -> Vec<String> {
    if text.is_empty() {
        return Vec::new();
    }
    if chunk == 0 {
        return vec![text.to_string()];
    }
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    for ch in text.chars() {
        if !current.is_empty() && current.len() + ch.len_utf8() > chunk {
            out.push(std::mem::take(&mut current));
        }
        current.push(ch);
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;
    use rig::completion::request::ToolDefinition;
    use rig::tool::Tool;
    use serde::Deserialize;
    use serde_json::json;

    fn collect_text_and_final(events: Vec<EngineEvent>) -> (Vec<String>, EngineResponse) {
        let mut texts: Vec<String> = Vec::new();
        let mut final_ev: Option<EngineResponse> = None;
        for ev in events {
            match ev {
                EngineEvent::Text(t) => texts.push(t),
                EngineEvent::Final(r) => {
                    assert!(final_ev.is_none(), "exactly one Final per stream");
                    final_ev = Some(r);
                }
                EngineEvent::ToolCall(_) | EngineEvent::ToolResult(_) => {
                    panic!("envelope path never emits tool events");
                }
                EngineEvent::Reasoning(_) | EngineEvent::ReasoningDelta { .. } => {
                    panic!("envelope path never emits reasoning events");
                }
                EngineEvent::Envelope(_) => {
                    // The Noop adapter yields a SentEnvelope as its first
                    // event from step 2 onward. These tests assert on text
                    // and final shape, so the envelope is dropped here.
                }
            }
        }
        (texts, final_ev.expect("Noop must emit a Final"))
    }

    #[test]
    fn noop_engine_name_is_noop() {
        let noop = Noop::default();
        assert_eq!(noop.name(), "noop");
    }

    #[tokio::test]
    async fn empty_history_emits_chunked_envelope_then_final() {
        let noop = Noop {
            chunk: DEFAULT_CHUNK_BYTES,
            override_response: None,
        };

        let stream = noop
            .stream(EngineInput::default(), &Settings::default(), &[], "alpha")
            .unwrap();
        let events: Vec<EngineEvent> = stream.collect().await;

        let (texts, final_resp) = collect_text_and_final(events);
        let assembled: String = texts.iter().cloned().collect();

        let expected = "noop response for alpha:\n[history follows]\n[response] ";
        assert_eq!(assembled, expected);
        assert_eq!(final_resp.text, expected);
        assert!(matches!(final_resp.stop_reason, StopReason::EndTurn));
        assert!(final_resp.usage.is_none());

        for chunk in texts.iter().take(texts.len().saturating_sub(1)) {
            assert!(
                chunk.len() <= DEFAULT_CHUNK_BYTES,
                "non-final chunk {chunk:?} exceeds chunk size"
            );
        }
    }

    #[tokio::test]
    async fn override_response_streams_override_then_final() {
        let noop = Noop {
            chunk: DEFAULT_CHUNK_BYTES,
            override_response: Some("hi".to_string()),
        };

        let stream = noop
            .stream(
                EngineInput {
                    preamble: Default::default(),
                    history: vec![Message::user("ignored")],
                },
                &Settings::default(),
                &[],
                "label",
            )
            .unwrap();
        let events: Vec<EngineEvent> = stream.collect().await;

        let (texts, final_resp) = collect_text_and_final(events);
        let assembled: String = texts.iter().cloned().collect();
        assert_eq!(assembled, "hi");
        assert_eq!(final_resp.text, "hi");
        assert!(matches!(final_resp.stop_reason, StopReason::EndTurn));
    }

    #[tokio::test]
    async fn two_runs_produce_byte_equal_text_payloads() {
        let noop = Noop {
            chunk: 16,
            override_response: None,
        };
        let history = vec![Message::system("sys"), Message::user("ask something")];

        let s1 = noop
            .stream(
                EngineInput {
                    preamble: Default::default(),
                    history: history.clone(),
                },
                &Settings::default(),
                &[],
                "label",
            )
            .unwrap();
        let e1: Vec<EngineEvent> = s1.collect().await;
        let s2 = noop
            .stream(
                EngineInput {
                    preamble: Default::default(),
                    history: history.clone(),
                },
                &Settings::default(),
                &[],
                "label",
            )
            .unwrap();
        let e2: Vec<EngineEvent> = s2.collect().await;

        let texts1: Vec<String> = e1
            .into_iter()
            .filter_map(|e| match e {
                EngineEvent::Text(t) => Some(t),
                _ => None,
            })
            .collect();
        let texts2: Vec<String> = e2
            .into_iter()
            .filter_map(|e| match e {
                EngineEvent::Text(t) => Some(t),
                _ => None,
            })
            .collect();
        assert_eq!(texts1, texts2);
    }

    #[derive(Debug, thiserror::Error)]
    #[error("echo tool error")]
    struct EchoError;

    #[derive(Deserialize)]
    struct EchoArgs {
        text: String,
    }

    #[derive(Default)]
    struct EchoTool;

    impl Tool for EchoTool {
        const NAME: &'static str = "echo";
        type Error = EchoError;
        type Args = EchoArgs;
        type Output = String;

        async fn definition(&self, _prompt: String) -> ToolDefinition {
            ToolDefinition {
                name: "echo".to_string(),
                description: "echoes the input text".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": { "text": { "type": "string" } },
                    "required": ["text"]
                }),
            }
        }

        async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
            Ok(args.text)
        }
    }

    #[test]
    fn parse_use_directive_extracts_tool_and_args() {
        let d = parse_use_directive(r#"please USE echo WITH {"text":"ok"} now"#).unwrap();
        assert_eq!(d.tool, "echo");
        assert_eq!(d.args, r#"{"text":"ok"} now"#);
    }

    #[test]
    fn parse_use_directive_stops_at_newline() {
        let d = parse_use_directive("USE echo WITH ok\nmore text").unwrap();
        assert_eq!(d.tool, "echo");
        assert_eq!(d.args, "ok");
    }

    #[test]
    fn parse_use_directive_returns_none_when_pattern_absent() {
        assert!(parse_use_directive("just a normal prompt").is_none());
        assert!(parse_use_directive("USE echo without with-clause").is_none());
    }

    #[tokio::test]
    async fn use_directive_drives_tool_round_trip() {
        let noop = Noop {
            chunk: DEFAULT_CHUNK_BYTES,
            override_response: None,
        };
        let echo: Arc<dyn ToolDyn> = Arc::new(EchoTool);

        let stream = noop
            .stream(
                EngineInput::with_history(vec![Message::user(r#"USE echo WITH {"text":"ok"}"#)]),
                &Settings::default(),
                &[echo],
                "label",
            )
            .unwrap();
        let events: Vec<EngineEvent> = stream.collect().await;

        let kinds: Vec<&str> = events
            .iter()
            .map(|e| match e {
                EngineEvent::Envelope(_) => "envelope",
                EngineEvent::Text(_) => "text",
                EngineEvent::ToolCall(_) => "tool_call",
                EngineEvent::ToolResult(_) => "tool_result",
                EngineEvent::Reasoning(_) => "reasoning",
                EngineEvent::ReasoningDelta { .. } => "reasoning_delta",
                EngineEvent::Final(_) => "final",
            })
            .collect();

        let tc = kinds
            .iter()
            .position(|k| *k == "tool_call")
            .expect("tool_call present");
        let tr = kinds
            .iter()
            .position(|k| *k == "tool_result")
            .expect("tool_result present");
        let fi = kinds
            .iter()
            .position(|k| *k == "final")
            .expect("final present");
        assert!(tc > 0, "at least one text event before tool_call");
        assert!(tc < tr, "tool_call before tool_result");
        assert!(tr < fi, "tool_result before final");
        assert_eq!(
            kinds[0], "envelope",
            "first event must be the SentEnvelope record"
        );
        for k in &kinds[1..tc] {
            assert_eq!(*k, "text", "events before tool_call must be text");
        }
        for k in &kinds[tr + 1..fi] {
            assert_eq!(
                *k, "text",
                "events between tool_result and final must be text"
            );
        }

        let EngineEvent::ToolCall(call) = &events[tc] else {
            unreachable!()
        };
        assert_eq!(call.function.name, "echo");
        assert_eq!(call.id, "call_1");

        let EngineEvent::ToolResult(result) = &events[tr] else {
            unreachable!()
        };
        assert_eq!(result.id, "call_1");
        let ToolResultContent::Text(t) = result.content.first() else {
            panic!("tool result must be text");
        };
        assert_eq!(t.text, "ok");
    }

    #[tokio::test]
    async fn engine_event_envelope_arrives_before_first_text() {
        let noop = Noop {
            chunk: DEFAULT_CHUNK_BYTES,
            override_response: None,
        };

        let stream = noop
            .stream(
                EngineInput::with_history(vec![Message::user("ask")]),
                &Settings::default(),
                &[],
                "alpha",
            )
            .unwrap();
        let events: Vec<EngineEvent> = stream.collect().await;
        let kinds: Vec<&str> = events
            .iter()
            .map(|e| match e {
                EngineEvent::Envelope(_) => "envelope",
                EngineEvent::Text(_) => "text",
                EngineEvent::Final(_) => "final",
                EngineEvent::ToolCall(_) => "tool_call",
                EngineEvent::ToolResult(_) => "tool_result",
                EngineEvent::Reasoning(_) => "reasoning",
                EngineEvent::ReasoningDelta { .. } => "reasoning_delta",
            })
            .collect();
        let envelope_idx = kinds
            .iter()
            .position(|k| *k == "envelope")
            .expect("envelope event must be present");
        let first_text_idx = kinds.iter().position(|k| *k == "text");
        assert_eq!(envelope_idx, 0, "Envelope must be the very first event");
        if let Some(t) = first_text_idx {
            assert!(envelope_idx < t, "Envelope must precede any Text event");
        }
    }

    #[tokio::test]
    async fn engine_event_envelope_carries_one_exchange_per_preamble_block() {
        use crate::content::{Preamble, PreambleBlock};

        let noop = Noop {
            chunk: DEFAULT_CHUNK_BYTES,
            override_response: None,
        };
        let preamble = Preamble {
            blocks: vec![
                PreambleBlock::InheritedSystem {
                    text: "I".to_string(),
                },
                PreambleBlock::LocalSystem {
                    text: "L".to_string(),
                },
            ],
        };
        let input = EngineInput {
            preamble,
            history: vec![Message::user("ask")],
        };

        let stream = noop
            .stream(input, &Settings::default(), &[], "label")
            .unwrap();
        let events: Vec<EngineEvent> = stream.collect().await;
        let envelope = events
            .iter()
            .find_map(|e| match e {
                EngineEvent::Envelope(env) => Some(env),
                _ => None,
            })
            .expect("Envelope event must be present");
        assert_eq!(
            envelope.prelude.len(),
            2,
            "every PreambleBlock must produce exactly one PreludeExchange"
        );
        assert!(envelope.tools.is_empty(), "no tools were advertised");
    }

    #[tokio::test]
    async fn use_directive_without_matching_tool_falls_back_to_envelope() {
        let noop = Noop {
            chunk: DEFAULT_CHUNK_BYTES,
            override_response: None,
        };

        let stream = noop
            .stream(
                EngineInput::with_history(vec![Message::user("USE missing WITH whatever")]),
                &Settings::default(),
                &[],
                "label",
            )
            .unwrap();
        let events: Vec<EngineEvent> = stream.collect().await;

        let (_texts, final_resp) = collect_text_and_final(events);
        assert!(final_resp.text.contains("noop response for label:"));
    }
}
