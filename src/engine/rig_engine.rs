//! Adapter that wraps a `rig::completion::CompletionModel` as an `Engine`.
//!
//! The module is named `rig_engine` so use sites do not collide with the
//! `rig` crate. Concrete provider constructors (`anthropic_from_env`,
//! `openai_from_env`) live alongside the adapter.

use anyhow::{Result, anyhow};
use futures::StreamExt;
use rig::agent::{AgentBuilder, MultiTurnStreamItem};
use rig::client::ProviderClient;
use rig::client::completion::CompletionClient;
use rig::completion::{CompletionModel, GetTokenUsage};
use rig::message::{Message, Text, UserContent};
use rig::streaming::{StreamedAssistantContent, StreamingChat};

use crate::engine::{Engine, EngineEvent, EngineResponse, EngineStream, Settings, StopReason};

pub struct RigEngine<M> {
    model: M,
    preamble: Option<String>,
}

impl<M> RigEngine<M> {
    pub fn new(model: M) -> Self {
        Self {
            model,
            preamble: None,
        }
    }

    pub fn with_preamble(mut self, s: impl Into<String>) -> Self {
        self.preamble = Some(s.into());
        self
    }
}

impl<M> Engine for RigEngine<M>
where
    M: CompletionModel + Clone + Send + Sync + 'static,
    M::StreamingResponse: GetTokenUsage + Send,
{
    fn stream(
        &self,
        history: Vec<Message>,
        _settings: &Settings,
        _request_label: &str,
    ) -> Result<EngineStream> {
        let last_user_text = extract_last_user_text(&history)?;
        let prior: Vec<Message> = history[..history.len() - 1].to_vec();

        let model = self.model.clone();
        let preamble = self.preamble.clone();

        Ok(Box::pin(async_stream::stream! {
            let mut builder = AgentBuilder::new(model);
            if let Some(p) = preamble.as_deref() {
                builder = builder.preamble(p);
            }
            let agent = builder.build();

            let mut stream = agent.stream_chat(last_user_text, prior).await;

            let mut assembled = String::new();
            let stop_reason = loop {
                match stream.next().await {
                    Some(Ok(MultiTurnStreamItem::StreamAssistantItem(
                        StreamedAssistantContent::Text(Text { text }),
                    ))) => {
                        assembled.push_str(&text);
                        yield EngineEvent::Text(text);
                    }
                    // ToolCall, ToolCallDelta, Reasoning, ReasoningDelta, and
                    // StreamUserItem are intentionally ignored in this slice.
                    // Per-provider mapping is deferred.
                    Some(Ok(MultiTurnStreamItem::FinalResponse(_))) | None => {
                        break StopReason::EndTurn;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => break StopReason::Error(e.to_string()),
                }
            };

            yield EngineEvent::Final(EngineResponse {
                text: assembled,
                stop_reason,
                usage: None,
            });
        }))
    }
}

fn extract_last_user_text(history: &[Message]) -> Result<String> {
    let last = history
        .last()
        .ok_or_else(|| anyhow!("history is empty; engine needs at least one user message"))?;
    let Message::User { content } = last else {
        return Err(anyhow!("last history message must be a User message"));
    };
    match content.first() {
        UserContent::Text(t) => Ok(t.text.clone()),
        _ => Err(anyhow!("last User message must contain text content")),
    }
}

pub fn anthropic_from_env(
    model: &str,
) -> Result<RigEngine<rig::providers::anthropic::completion::CompletionModel>> {
    let client = rig::providers::anthropic::Client::from_env()
        .map_err(|e| anyhow!("anthropic from_env: {e}"))?;
    Ok(RigEngine::new(client.completion_model(model)))
}

pub fn openai_from_env(
    model: &str,
) -> Result<RigEngine<rig::providers::openai::responses_api::ResponsesCompletionModel>> {
    let client = rig::providers::openai::Client::from_env()
        .map_err(|e| anyhow!("openai from_env: {e}"))?;
    Ok(RigEngine::new(client.completion_model(model)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig::message::Message;

    fn build_dummy_anthropic() -> RigEngine<rig::providers::anthropic::completion::CompletionModel>
    {
        let client = rig::providers::anthropic::Client::from_val("dummy-key".to_string())
            .expect("from_val with dummy key should construct without network");
        RigEngine::new(client.completion_model("claude-sonnet-4-5"))
    }

    fn err_message(result: Result<EngineStream>) -> String {
        match result {
            Ok(_) => panic!("expected stream() to return Err"),
            Err(e) => e.to_string(),
        }
    }

    #[test]
    fn rejects_empty_history() {
        let engine = build_dummy_anthropic();
        let msg = err_message(engine.stream(Vec::new(), &Settings::default(), "label"));
        assert!(msg.contains("history is empty"));
    }

    #[test]
    fn rejects_assistant_last_message() {
        let engine = build_dummy_anthropic();
        let history = vec![Message::user("hi"), Message::assistant("there")];
        let msg = err_message(engine.stream(history, &Settings::default(), "label"));
        assert!(msg.contains("must be a User message"));
    }
}
