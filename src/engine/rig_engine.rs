//! Adapter that wraps a `rig::completion::CompletionModel` as an `Engine`.
//!
//! The module is named `rig_engine` so use sites do not collide with the
//! `rig` crate. Concrete provider constructors (`anthropic_from_env`,
//! `openai_from_env`, `gemini_from_env`, and `bedrock_from_env` under the
//! `bedrock` feature) live alongside the adapter.

use std::sync::Arc;

use crate::engine::{
    Engine, EngineEvent, EngineInput, EngineName, EngineResponse, EngineStream, ModelId, Settings,
    StopReason, Usage,
};
use anyhow::{Result, anyhow};
use futures::StreamExt;
use rig::agent::StreamingError;
use rig::agent::{AgentBuilder, MultiTurnStreamItem};
use rig::client::ProviderClient;
use rig::client::completion::CompletionClient;
use rig::completion::request::{PromptError, ToolDefinition};
use rig::completion::{CompletionModel, GetTokenUsage};
use rig::message::{Message, Text, UserContent};
use rig::streaming::{StreamedAssistantContent, StreamedUserContent, StreamingChat};
use rig::tool::{ToolDyn, ToolError};
use rig::wasm_compat::WasmBoxedFuture;

use crate::content::PreambleBlock;

/// Re-box a shared `Arc<dyn ToolDyn>` as the `Box<dyn ToolDyn>` shape that
/// `AgentBuilder::tools` requires, while leaving the caller's `Arc` intact.
/// `Box<dyn ToolDyn>` is not `Clone`, but every method on the trait can be
/// forwarded to the inner `Arc`.
struct DynToolHandle(Arc<dyn ToolDyn>);

impl ToolDyn for DynToolHandle {
    fn name(&self) -> String {
        self.0.name()
    }

    fn definition<'a>(&'a self, prompt: String) -> WasmBoxedFuture<'a, ToolDefinition> {
        self.0.definition(prompt)
    }

    fn call<'a>(&'a self, args: String) -> WasmBoxedFuture<'a, Result<String, ToolError>> {
        self.0.call(args)
    }
}

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
        settings: &Settings,
        tools: &[Arc<dyn ToolDyn>],
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
        let engine_name = EngineName(self.name().to_string());
        let model_id = ModelId(self.model_id.clone());
        let max_tool_turns = settings.max_tool_turns;
        let dyn_tools: Vec<Box<dyn ToolDyn>> = tools
            .iter()
            .map(|t| Box::new(DynToolHandle(t.clone())) as Box<dyn ToolDyn>)
            .collect();

        Ok(Box::pin(async_stream::stream! {
            let mut builder = AgentBuilder::new(model);
            if let Some(p) = preamble.as_deref() {
                builder = builder.preamble(p);
            }
            let agent = builder.tools(dyn_tools).build();

            let mut stream = agent
                .stream_chat(last_user_text, prior)
                .multi_turn(max_tool_turns)
                .await;

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
                    Some(Ok(MultiTurnStreamItem::StreamAssistantItem(
                        StreamedAssistantContent::ToolCall { tool_call, .. },
                    ))) => {
                        yield EngineEvent::ToolCall(tool_call);
                    }
                    Some(Ok(MultiTurnStreamItem::StreamUserItem(
                        StreamedUserContent::ToolResult { tool_result, .. },
                    ))) => {
                        yield EngineEvent::ToolResult(tool_result);
                    }
                    // ToolCallDelta, Reasoning, ReasoningDelta fall through.
                    Some(Ok(MultiTurnStreamItem::FinalResponse(final_response))) => {
                        usage = Some(Usage::from(final_response.usage()));
                        break StopReason::EndTurn;
                    }
                    None => break StopReason::EndTurn,
                    Some(Ok(_)) => {}
                    Some(Err(StreamingError::Prompt(boxed)))
                        if matches!(*boxed, PromptError::MaxTurnsError { .. }) =>
                    {
                        break StopReason::ToolLimit;
                    }
                    Some(Err(e)) => break StopReason::Error(e.to_string()),
                }
            };

            yield EngineEvent::Final(EngineResponse {
                text: assembled,
                stop_reason,
                engine_name,
                model_id,
                usage,
            });
        }))
    }
}

/// Flatten an `EngineInput` preamble into a single string, joined onto any
/// constructor-supplied preamble. Inherited and local system blocks render
/// as their text; skill blocks render as `## <name>\n\n<body>`.
fn merge_preamble(constructor: Option<String>, input: &crate::content::Preamble) -> Option<String> {
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
    use crate::content::Preamble;

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
        let msg =
            err_message(engine.stream(EngineInput::default(), &Settings::default(), &[], "label"));
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
            &[],
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
        let msg =
            err_message(engine.stream(EngineInput::default(), &Settings::default(), &[], "label"));
        assert!(msg.contains("history is empty"));
    }

    #[test]
    fn gemini_rejects_assistant_last_message() {
        let engine = build_dummy_gemini();
        let history = vec![Message::user("hi"), Message::assistant("there")];
        let msg = err_message(engine.stream(
            EngineInput {
                preamble: Preamble::default(),
                history,
            },
            &Settings::default(),
            &[],
            "label",
        ));
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
        use rig::completion::request::ToolDefinition;
        use rig::completion::{
            CompletionError, CompletionModel, CompletionRequest, CompletionResponse, GetTokenUsage,
            Usage as RigUsage,
        };
        use rig::streaming::{
            RawStreamingChoice, RawStreamingToolCall, StreamingCompletionResponse,
        };
        use rig::tool::{Tool, ToolDyn};
        use serde::{Deserialize, Serialize};
        use serde_json::json;
        use std::sync::Arc;
        use std::sync::atomic::{AtomicUsize, Ordering};

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
                    &[],
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

            assert_eq!(final_response.model_id.0, "fake-model-id");
            let usage = final_response.usage.expect("usage must be populated");
            assert_eq!(usage.input_tokens, 7);
            assert_eq!(usage.output_tokens, 3);
            assert!(matches!(final_response.stop_reason, StopReason::EndTurn));
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

        /// Scripted model: each `stream()` call yields a tool call until the
        /// counter exceeds `tool_calls_to_emit`, then yields a final response.
        #[derive(Clone)]
        struct ScriptedToolModel {
            calls: Arc<AtomicUsize>,
            tool_calls_to_emit: usize,
        }

        impl ScriptedToolModel {
            fn new(tool_calls_to_emit: usize) -> Self {
                Self {
                    calls: Arc::new(AtomicUsize::new(0)),
                    tool_calls_to_emit,
                }
            }
        }

        impl CompletionModel for ScriptedToolModel {
            type Response = serde_json::Value;
            type StreamingResponse = FakeStreamingResponse;
            type Client = ();

            fn make(_client: &Self::Client, _model: impl Into<String>) -> Self {
                Self::new(0)
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
                let call_index = self.calls.fetch_add(1, Ordering::SeqCst);
                let limit = self.tool_calls_to_emit;
                let inner = Box::pin(async_stream::stream! {
                    if call_index < limit {
                        let id = format!("call_{}", call_index + 1);
                        yield Ok(RawStreamingChoice::ToolCall(RawStreamingToolCall::new(
                            id,
                            "echo".to_string(),
                            json!({"text": "ok"}),
                        )));
                    } else {
                        yield Ok(RawStreamingChoice::Message("done".to_string()));
                        yield Ok(RawStreamingChoice::FinalResponse(FakeStreamingResponse));
                    }
                });
                Ok(StreamingCompletionResponse::stream(inner))
            }
        }

        #[tokio::test]
        async fn multi_round_trip_surfaces_tool_call_then_result_pairs_in_order() {
            let engine = RigEngine::new(ScriptedToolModel::new(2), "fake", "fake-model-id");
            let echo: Arc<dyn ToolDyn> = Arc::new(EchoTool);
            let stream = engine
                .stream(
                    EngineInput::with_history(vec![rig::message::Message::user("ask")]),
                    &Settings::default(),
                    &[echo],
                    "label",
                )
                .expect("stream() should succeed");
            let events: Vec<EngineEvent> = stream.collect().await;

            let kinds: Vec<&'static str> = events
                .iter()
                .filter_map(|e| match e {
                    EngineEvent::ToolCall(_) => Some("call"),
                    EngineEvent::ToolResult(_) => Some("result"),
                    EngineEvent::Final(_) => Some("final"),
                    EngineEvent::Text(_) => None,
                })
                .collect();

            assert_eq!(
                kinds,
                vec!["call", "result", "call", "result", "final"],
                "two consecutive tool round-trips must surface as ToolCall, ToolResult, ToolCall, ToolResult, then Final; got {events:?}"
            );

            let final_event = events
                .iter()
                .find_map(|e| match e {
                    EngineEvent::Final(r) => Some(r),
                    _ => None,
                })
                .expect("Final event must be present");
            assert!(matches!(final_event.stop_reason, StopReason::EndTurn));
        }

        #[tokio::test]
        async fn tool_limit_exhaustion_yields_stop_reason_tool_limit() {
            let engine =
                RigEngine::new(ScriptedToolModel::new(usize::MAX), "fake", "fake-model-id");
            let echo: Arc<dyn ToolDyn> = Arc::new(EchoTool);
            let mut settings = Settings::default();
            settings.max_tool_turns = 0;

            let stream = engine
                .stream(
                    EngineInput::with_history(vec![rig::message::Message::user("ask")]),
                    &settings,
                    &[echo],
                    "label",
                )
                .expect("stream() should succeed");
            let events: Vec<EngineEvent> = stream.collect().await;

            let final_event = events
                .iter()
                .find_map(|e| match e {
                    EngineEvent::Final(r) => Some(r),
                    _ => None,
                })
                .expect("Final event must be present even when max turns are exhausted");
            assert!(
                matches!(final_event.stop_reason, StopReason::ToolLimit),
                "exhausting max_tool_turns must surface as StopReason::ToolLimit; got {:?}",
                final_event.stop_reason
            );
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
        let msg =
            err_message(engine.stream(EngineInput::default(), &Settings::default(), &[], "label"));
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
            &[],
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
