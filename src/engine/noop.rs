use std::fmt::Write;

use rig::message::{AssistantContent, Message, UserContent};

use crate::engine::{Engine, EngineEvent, EngineResponse, EngineStream, Settings, StopReason};

pub const DEFAULT_CHUNK_BYTES: usize = 32;

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
        history: Vec<Message>,
        _settings: &Settings,
        request_label: &str,
    ) -> anyhow::Result<EngineStream> {
        let text = match &self.override_response {
            Some(s) => s.clone(),
            None => build_envelope(request_label, &history),
        };

        let mut events: Vec<EngineEvent> = split_into_chunks(&text, self.chunk)
            .into_iter()
            .map(EngineEvent::Text)
            .collect();
        events.push(EngineEvent::Final(EngineResponse {
            text,
            model: None,
            stop_reason: StopReason::EndTurn,
            usage: None,
        }));

        Ok(Box::pin(futures::stream::iter(events)))
    }
}

fn build_envelope(request_label: &str, history: &[Message]) -> String {
    let mut out = String::new();
    writeln!(out, "noop response for {request_label}:").unwrap();
    out.push_str("[history follows]\n");
    for (i, m) in history.iter().enumerate() {
        writeln!(out, "[message {i}] {}: {}", role_str(m), message_text(m)).unwrap();
    }
    let last_user = history
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
            .stream(Vec::new(), &Settings::default(), "alpha")
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
                vec![Message::user("ignored")],
                &Settings::default(),
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
            .stream(history.clone(), &Settings::default(), "label")
            .unwrap();
        let e1: Vec<EngineEvent> = s1.collect().await;
        let s2 = noop
            .stream(history.clone(), &Settings::default(), "label")
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
}
