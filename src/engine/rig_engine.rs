//! Adapter that wraps a `rig::completion::CompletionModel` as an `Engine`.
//!
//! The module is named `rig_engine` so use sites do not collide with the
//! `rig` crate. Concrete provider constructors (`anthropic_from_env`,
//! `openai_from_env`, `gemini_from_env`, and `bedrock_from_env` under the
//! `bedrock` feature) live alongside the adapter.

use anyhow::{Result, anyhow};
use futures::StreamExt;
use rig::agent::{AgentBuilder, MultiTurnStreamItem};
use rig::client::ProviderClient;
use rig::client::completion::CompletionClient;
use rig::completion::{CompletionModel, GetTokenUsage};
use rig::message::{Message, Text, UserContent};
use rig::streaming::{StreamedAssistantContent, StreamingChat};

use crate::content::PreambleBlock;
use crate::engine::{
    Engine, EngineEvent, EngineInput, EngineResponse, EngineStream, Settings, StopReason, Usage,
};

const ANTHROPIC: &str = "anthropic";
const OPENAI: &str = "openai";
const GEMINI: &str = "gemini";
#[cfg(feature = "bedrock")]
const BEDROCK: &str = "bedrock";

pub struct RigEngine<M> {
    model: M,
    preamble: Option<String>,
    engine_name: &'static str,
    model_id: String,
}

impl<M> RigEngine<M> {
    pub fn new(model: M, engine_name: &'static str, model_id: impl Into<String>) -> Self {
        Self {
            model,
            preamble: None,
            engine_name,
            model_id: model_id.into(),
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
    fn name(&self) -> &'static str {
        self.engine_name
    }

    fn stream(
        &self,
        input: EngineInput,
        _settings: &Settings,
        _request_label: &str,
    ) -> Result<EngineStream> {
        let EngineInput {
            preamble: input_preamble,
            history,
        } = input;
        let last_user_text = extract_last_user_text(&history)?;
        let prior: Vec<Message> = history[..history.len() - 1].to_vec();

        let model = self.model.clone();
        let constructor_preamble = self.preamble.clone();
        let preamble = merge_preamble(constructor_preamble, &input_preamble);
        let model_id = self.model_id.clone();

        Ok(Box::pin(async_stream::stream! {
            let mut builder = AgentBuilder::new(model);
            if let Some(p) = preamble.as_deref() {
                builder = builder.preamble(p);
            }
            let agent = builder.build();

            let mut stream = agent.stream_chat(last_user_text, prior).await;

            let mut assembled = String::new();
            let mut usage: Option<Usage> = None;
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
                    Some(Ok(MultiTurnStreamItem::FinalResponse(final_response))) => {
                        usage = Some(Usage::from(final_response.usage()));
                        break StopReason::EndTurn;
                    }
                    None => break StopReason::EndTurn,
                    Some(Ok(_)) => {}
                    Some(Err(e)) => break StopReason::Error(e.to_string()),
                }
            };

            yield EngineEvent::Final(EngineResponse {
                text: assembled,
                model: Some(model_id),
                stop_reason,
                usage,
            });
        }))
    }
}

/// Flatten an `EngineInput` preamble into a single string, joined onto any
/// constructor-supplied preamble. Inherited and local system blocks render
/// as their text; skill blocks render as `## <name>\n\n<body>`.
fn merge_preamble(
    constructor: Option<String>,
    input: &crate::content::Preamble,
) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(c) = constructor.filter(|s| !s.is_empty()) {
        parts.push(c);
    }
    for block in &input.blocks {
        match block {
            PreambleBlock::InheritedSystem { text } | PreambleBlock::LocalSystem { text } => {
                if !text.is_empty() {
                    parts.push(text.clone());
                }
            }
            PreambleBlock::Skill(skill) => {
                parts.push(format!(
                    "## {name}\n\n{body}",
                    name = skill.name.as_str(),
                    body = skill.body.as_str()
                ));
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
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

fn rig_engine_from_env<C>(
    provider: &'static str,
    model: &str,
) -> Result<RigEngine<C::CompletionModel>>
where
    C: ProviderClient + CompletionClient,
    <C as ProviderClient>::Error: std::fmt::Display,
{
    let client = C::from_env().map_err(|e| anyhow!("{provider} from_env: {e}"))?;
    Ok(RigEngine::new(
        client.completion_model(model),
        provider,
        model,
    ))
}

pub fn anthropic_from_env(
    model: &str,
) -> Result<RigEngine<rig::providers::anthropic::completion::CompletionModel>> {
    rig_engine_from_env::<rig::providers::anthropic::Client>(ANTHROPIC, model)
}

pub fn openai_from_env(
    model: &str,
) -> Result<RigEngine<rig::providers::openai::responses_api::ResponsesCompletionModel>> {
    rig_engine_from_env::<rig::providers::openai::Client>(OPENAI, model)
}

pub fn gemini_from_env(
    model: &str,
) -> Result<RigEngine<rig::providers::gemini::completion::CompletionModel>> {
    rig_engine_from_env::<rig::providers::gemini::Client>(GEMINI, model)
}

#[cfg(feature = "bedrock")]
pub fn bedrock_from_env(
    model: &str,
) -> Result<RigEngine<rig_bedrock::completion::CompletionModel>> {
    rig_engine_from_env::<rig_bedrock::client::Client>(BEDROCK, model)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig::message::Message;

    fn build_dummy_anthropic() -> RigEngine<rig::providers::anthropic::completion::CompletionModel>
    {
        let client = rig::providers::anthropic::Client::from_val("dummy-key".to_string())
            .expect("from_val with dummy key should construct without network");
        RigEngine::new(
            client.completion_model("claude-sonnet-4-5"),
            ANTHROPIC,
            "claude-sonnet-4-5",
        )
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
        let msg = err_message(engine.stream(
            EngineInput::default(),
            &Settings::default(),
            "label",
        ));
        assert!(msg.contains("history is empty"));
    }

    #[test]
    fn rejects_assistant_last_message() {
        let engine = build_dummy_anthropic();
        let history = vec![Message::user("hi"), Message::assistant("there")];
        let msg = err_message(engine.stream(
            EngineInput {
                preamble: Default::default(),
                history,
            },
            &Settings::default(),
            "label",
        ));
        assert!(msg.contains("must be a User message"));
    }

    #[test]
    fn anthropic_engine_name_is_anthropic() {
        let engine = build_dummy_anthropic();
        assert_eq!(engine.name(), "anthropic");
    }

    #[test]
    fn openai_engine_name_is_openai() {
        let client = rig::providers::openai::Client::from_val("dummy-key".to_string().into())
            .expect("openai from_val with dummy key constructs without network");
        let engine = RigEngine::new(
            client.completion_model("gpt-4o-mini"),
            OPENAI,
            "gpt-4o-mini",
        );
        assert_eq!(engine.name(), "openai");
    }

    fn build_dummy_gemini() -> RigEngine<rig::providers::gemini::completion::CompletionModel> {
        let client = rig::providers::gemini::Client::from_val("dummy-key".to_string().into())
            .expect("gemini from_val with dummy key constructs without network");
        RigEngine::new(
            client.completion_model("gemini-2.5-flash"),
            GEMINI,
            "gemini-2.5-flash",
        )
    }

    #[test]
    fn gemini_rejects_empty_history() {
        let engine = build_dummy_gemini();
        let msg = err_message(engine.stream(EngineInput::default(), &Settings::default(), "label"));
        assert!(msg.contains("history is empty"));
    }

    #[test]
    fn gemini_rejects_assistant_last_message() {
        let engine = build_dummy_gemini();
        let history = vec![Message::user("hi"), Message::assistant("there")];
        let msg = err_message(engine.stream(EngineInput::default(), &Settings::default(), "label"));
        assert!(msg.contains("must be a User message"));
    }

    #[test]
    fn gemini_engine_name_is_gemini() {
        let engine = build_dummy_gemini();
        assert_eq!(engine.name(), "gemini");
    }

    mod fake_model {
        use super::*;
        use crate::engine::EngineEvent;
        use futures::StreamExt;
        use rig::completion::{
            CompletionError, CompletionModel, CompletionRequest, CompletionResponse, GetTokenUsage,
            Usage as RigUsage,
        };
        use rig::streaming::{RawStreamingChoice, StreamingCompletionResponse};
        use serde::{Deserialize, Serialize};

        #[derive(Clone)]
        struct FakeModel;

        #[derive(Clone, Debug, Serialize, Deserialize)]
        struct FakeStreamingResponse;

        impl GetTokenUsage for FakeStreamingResponse {
            fn token_usage(&self) -> Option<RigUsage> {
                let mut u = RigUsage::new();
                u.input_tokens = 7;
                u.output_tokens = 3;
                Some(u)
            }
        }

        impl CompletionModel for FakeModel {
            type Response = serde_json::Value;
            type StreamingResponse = FakeStreamingResponse;
            type Client = ();

            fn make(_client: &Self::Client, _model: impl Into<String>) -> Self {
                Self
            }

            async fn completion(
                &self,
                _request: CompletionRequest,
            ) -> Result<CompletionResponse<Self::Response>, CompletionError> {
                Err(CompletionError::ProviderError("unused".to_string()))
            }

            async fn stream(
                &self,
                _request: CompletionRequest,
            ) -> Result<StreamingCompletionResponse<Self::StreamingResponse>, CompletionError>
            {
                let inner = Box::pin(async_stream::stream! {
                    yield Ok(RawStreamingChoice::Message("hi".to_string()));
                    yield Ok(RawStreamingChoice::FinalResponse(FakeStreamingResponse));
                });
                Ok(StreamingCompletionResponse::stream(inner))
            }
        }

        #[tokio::test]
        async fn populates_model_and_usage_from_final_response() {
            let engine = RigEngine::new(FakeModel, "fake", "fake-model-id");
            let stream = engine
                .stream(
                    EngineInput {
                        preamble: Default::default(),
                        history: vec![rig::message::Message::user("ask")],
                    },
                    &Settings::default(),
                    "label",
                )
                .expect("stream() should succeed");
            let events: Vec<EngineEvent> = stream.collect().await;

            let mut final_response: Option<EngineResponse> = None;
            for ev in events {
                if let EngineEvent::Final(r) = ev {
                    final_response = Some(r);
                }
            }
            let final_response = final_response.expect("Final event must be present");

            assert_eq!(final_response.model.as_deref(), Some("fake-model-id"));
            let usage = final_response.usage.expect("usage must be populated");
            assert_eq!(usage.input_tokens, 7);
            assert_eq!(usage.output_tokens, 3);
            assert!(matches!(final_response.stop_reason, StopReason::EndTurn));
        }
    }
}

#[cfg(all(test, feature = "bedrock"))]
mod bedrock_tests {
    use super::*;
    use aws_config::{BehaviorVersion, SdkConfig};
    use aws_credential_types::Credentials;
    use aws_credential_types::provider::SharedCredentialsProvider;
    use aws_sdk_bedrockruntime::config::Region;
    use rig::message::Message;

    fn build_dummy_bedrock() -> RigEngine<rig_bedrock::completion::CompletionModel> {
        let creds = Credentials::new("AKIA_DUMMY", "dummy_secret", None, None, "test");
        let cfg = SdkConfig::builder()
            .behavior_version(BehaviorVersion::latest())
            .region(Region::new("us-east-1"))
            .credentials_provider(SharedCredentialsProvider::new(creds))
            .build();
        let aws_client = aws_sdk_bedrockruntime::Client::new(&cfg);
        let client: rig_bedrock::client::Client = aws_client.into();
        RigEngine::new(client.completion_model("test-model"), BEDROCK, "test-model")
    }

    fn err_message(result: Result<EngineStream>) -> String {
        match result {
            Ok(_) => panic!("expected stream() to return Err"),
            Err(e) => e.to_string(),
        }
    }

    #[test]
    fn bedrock_rejects_empty_history() {
        let engine = build_dummy_bedrock();
        let msg = err_message(engine.stream(
            EngineInput::default(),
            &Settings::default(),
            "label",
        ));
        assert!(msg.contains("history is empty"));
    }

    #[test]
    fn bedrock_rejects_assistant_last_message() {
        let engine = build_dummy_bedrock();
        let history = vec![Message::user("hi"), Message::assistant("there")];
        let msg = err_message(engine.stream(
            EngineInput {
                preamble: Default::default(),
                history,
            },
            &Settings::default(),
            "label",
        ));
        assert!(msg.contains("must be a User message"));
    }

    #[test]
    fn bedrock_engine_name_is_bedrock() {
        let engine = build_dummy_bedrock();
        assert_eq!(engine.name(), "bedrock");
    }
}
