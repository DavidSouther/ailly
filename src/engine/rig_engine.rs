//! Adapter that wraps a `rig::completion::CompletionModel` as an `Engine`.
//!
//! The module is named `rig_engine` so use sites do not collide with the
//! `rig` crate. Concrete provider constructors (`anthropic_from_env`,
//! `openai_from_env`, `gemini_from_env`, and `bedrock_from_env` under the
//! `bedrock` feature) live alongside the adapter.

use std::sync::Arc;

use crate::engine::prelude::{PreludeExchange, SentEnvelope, tools_exchange};
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

/// Re-box a shared `Arc<dyn ToolDyn>` as the `Box<dyn ToolDyn>` shape that
/// `AgentBuilder::knowledge::tools` requires, while leaving the caller's `Arc` intact.
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
    engine_name: &'static str,
    model_id: String,
}

impl<M> std::fmt::Debug for RigEngine<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RigEngine")
            .field("engine_name", &self.engine_name)
            .field("model_id", &self.model_id)
            .finish()
    }
}

impl<M> RigEngine<M> {
    pub fn new(model: M, engine_name: &'static str, model_id: impl Into<String>) -> Self {
        Self {
            model,
            engine_name,
            model_id: model_id.into(),
        }
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
        let prelude_exchanges: Vec<PreludeExchange> = (&input_preamble).into();
        let engine_name = EngineName(self.name().to_string());
        let model_id = ModelId(self.model_id.clone());
        let max_tool_turns = settings.max_tool_turns;
        let arc_tools: Vec<Arc<dyn ToolDyn>> = tools.to_vec();
        let dyn_tools: Vec<Box<dyn ToolDyn>> = tools
            .iter()
            .map(|t| Box::new(DynToolHandle(t.clone())) as Box<dyn ToolDyn>)
            .collect();

        let builder = AgentBuilder::new(model);
        let mut prelude = prelude_exchanges;
        let mut new_prior: Vec<Message> = Vec::with_capacity(prelude.len() * 2 + prior.len());
        for exchange in &prelude {
            new_prior.push(exchange.user.clone());
            new_prior.push(exchange.assistant.clone());
        }
        new_prior.extend(prior);

        Ok(Box::pin(async_stream::stream! {
            let mut tool_defs: Vec<ToolDefinition> = Vec::with_capacity(arc_tools.len());
            for tool in &arc_tools {
                tool_defs.push(tool.definition(String::new()).await);
            }

            if let Some(extra) = tools_exchange(&tool_defs) {
                prelude.push(extra);
            }

            yield EngineEvent::Envelope(SentEnvelope {
                prelude,
                tools: tool_defs,
            });

            let agent = builder.tools(dyn_tools).build();

            let mut stream = agent
                .stream_chat(last_user_text, new_prior)
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
                    // Forward whole reasoning blocks. `Reasoning` is rig
                    // vocabulary; the engine layer never reaches into
                    // content::* to translate it.
                    Some(Ok(MultiTurnStreamItem::StreamAssistantItem(
                        StreamedAssistantContent::Reasoning(reasoning),
                    ))) => {
                        yield EngineEvent::Reasoning(reasoning);
                    }
                    // Forward reasoning deltas. The recorder is responsible
                    // for the id-keyed merge; the engine is purely a forwarder.
                    Some(Ok(MultiTurnStreamItem::StreamAssistantItem(
                        StreamedAssistantContent::ReasoningDelta { id, reasoning },
                    ))) => {
                        yield EngineEvent::ReasoningDelta { id, text: reasoning };
                    }
                    // ToolCallDelta falls through.
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

        /// `CompletionModel` impl that replays a fixed sequence of
        /// `RawStreamingChoice` items on every `stream()` call. Use to
        /// script one-shot scenarios; reach for `ScriptedToolModel`
        /// instead when the script must vary per call.
        ///
        /// `captured_request` records the `CompletionRequest` the agent
        /// submitted on the most recent `stream()` call, so tests can
        /// assert on the preamble and chat history rig actually saw.
        #[derive(Clone)]
        struct ScriptedFake {
            items: Vec<RawStreamingChoice<FakeStreamingResponse>>,
            captured_request: Arc<std::sync::Mutex<Option<CompletionRequest>>>,
        }

        impl ScriptedFake {
            fn new(items: Vec<RawStreamingChoice<FakeStreamingResponse>>) -> Self {
                Self {
                    items,
                    captured_request: Arc::new(std::sync::Mutex::new(None)),
                }
            }
        }

        impl CompletionModel for ScriptedFake {
            type Response = serde_json::Value;
            type StreamingResponse = FakeStreamingResponse;
            type Client = ();

            fn make(_client: &Self::Client, _model: impl Into<String>) -> Self {
                Self::new(Vec::new())
            }

            async fn completion(
                &self,
                _request: CompletionRequest,
            ) -> Result<CompletionResponse<Self::Response>, CompletionError> {
                Err(CompletionError::ProviderError("unused".to_string()))
            }

            async fn stream(
                &self,
                request: CompletionRequest,
            ) -> Result<StreamingCompletionResponse<Self::StreamingResponse>, CompletionError>
            {
                *self.captured_request.lock().unwrap() = Some(request);
                let items = self.items.clone();
                let inner = Box::pin(async_stream::stream! {
                    for item in items {
                        yield Ok(item);
                    }
                });
                Ok(StreamingCompletionResponse::stream(inner))
            }
        }

        #[tokio::test]
        async fn populates_model_and_usage_from_final_response() {
            let engine = RigEngine::new(
                ScriptedFake::new(vec![
                    RawStreamingChoice::Message("hi".to_string()),
                    RawStreamingChoice::FinalResponse(FakeStreamingResponse),
                ]),
                "fake",
                "fake-model-id",
            );
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

        fn user_text(message: &Message) -> String {
            match message {
                Message::User { content } => match content.first() {
                    rig::message::UserContent::Text(t) => t.text.clone(),
                    other => panic!("expected user text, got {other:?}"),
                },
                other => panic!("expected User message, got {other:?}"),
            }
        }

        fn assistant_text(message: &Message) -> String {
            match message {
                Message::Assistant { content, .. } => match content.first() {
                    rig::message::AssistantContent::Text(t) => t.text.clone(),
                    other => panic!("expected assistant text, got {other:?}"),
                },
                other => panic!("expected Assistant message, got {other:?}"),
            }
        }

        #[tokio::test]
        async fn rig_engine_stream_prepends_prelude_messages_to_history() {
            use crate::content::{Preamble, PreambleBlock};

            let fake = ScriptedFake::new(vec![
                RawStreamingChoice::Message("ok".to_string()),
                RawStreamingChoice::FinalResponse(FakeStreamingResponse),
            ]);
            let captured = fake.captured_request.clone();
            let engine = RigEngine::new(fake, "fake", "fake-model-id");

            let preamble = Preamble {
                blocks: vec![
                    PreambleBlock::InheritedSystem {
                        text: "I-TEXT".to_string(),
                    },
                    PreambleBlock::LocalSystem {
                        text: "L-TEXT".to_string(),
                    },
                ],
            };
            let history = vec![Message::user("ask")];

            let stream = engine
                .stream(
                    EngineInput { preamble, history },
                    &Settings::default(),
                    &[],
                    "label",
                )
                .expect("stream() should succeed");
            let _events: Vec<EngineEvent> = stream.collect().await;

            let request = captured
                .lock()
                .unwrap()
                .clone()
                .expect("rig must have submitted a CompletionRequest");
            let chat: Vec<Message> = request.chat_history.into_iter().collect();
            assert!(
                chat.len() >= 5,
                "chat history must contain prelude (4 messages) plus the user prompt; got {}",
                chat.len()
            );
            assert_eq!(user_text(&chat[0]), "I-TEXT");
            assert_eq!(
                assistant_text(&chat[1]),
                "Understood. I will follow the inherited instructions."
            );
            assert_eq!(user_text(&chat[2]), "L-TEXT");
            assert_eq!(
                assistant_text(&chat[3]),
                "Understood. I will follow the local instructions."
            );
            assert_eq!(user_text(chat.last().unwrap()), "ask");
        }

        #[tokio::test]
        async fn rig_engine_no_longer_calls_agent_builder_preamble() {
            use crate::content::{Preamble, PreambleBlock};

            let fake = ScriptedFake::new(vec![
                RawStreamingChoice::Message("ok".to_string()),
                RawStreamingChoice::FinalResponse(FakeStreamingResponse),
            ]);
            let captured = fake.captured_request.clone();
            let engine = RigEngine::new(fake, "fake", "fake-model-id");

            let preamble = Preamble {
                blocks: vec![PreambleBlock::InheritedSystem {
                    text: "SHOULD-NOT-LEAK".to_string(),
                }],
            };
            let stream = engine
                .stream(
                    EngineInput {
                        preamble,
                        history: vec![Message::user("ask")],
                    },
                    &Settings::default(),
                    &[],
                    "label",
                )
                .expect("stream() should succeed");
            let _events: Vec<EngineEvent> = stream.collect().await;

            let request = captured
                .lock()
                .unwrap()
                .clone()
                .expect("rig must have submitted a CompletionRequest");
            assert!(
                request.preamble.is_none() || request.preamble.as_deref() == Some(""),
                "AgentBuilder::preamble must not be called; CompletionRequest::preamble was {:?}",
                request.preamble
            );
        }

        /// Compile-time companion to the doc-comment guarantee that the
        /// helper that previously concatenated the preamble into a single
        /// string has been removed. Scans the engine source tree for the
        /// symbol; zero hits means the rewrite is complete. The file
        /// containing this test is skipped because the function name
        /// itself contains the symbol.
        #[test]
        fn merge_preamble_is_removed() {
            use std::path::Path;
            fn collect_rs(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
                for entry in std::fs::read_dir(dir).unwrap() {
                    let entry = entry.unwrap();
                    let path = entry.path();
                    if path.is_dir() {
                        collect_rs(&path, out);
                    } else if path.extension().and_then(|s| s.to_str()) == Some("rs") {
                        out.push(path);
                    }
                }
            }
            let mut files = Vec::new();
            collect_rs(Path::new("src"), &mut files);
            let forbidden = format!("{}{}", "merge_", "preamble");
            let self_path = Path::new("src/engine/rig_engine.rs");
            let mut scanned = 0usize;
            for path in &files {
                if path == self_path {
                    continue;
                }
                let body = std::fs::read_to_string(path).unwrap();
                assert!(
                    !body.contains(forbidden.as_str()),
                    "{}: must not reference the removed preamble-merge helper",
                    path.display()
                );
                scanned += 1;
            }
            assert!(
                scanned > 0,
                "scan must inspect at least one source file beyond the test's own"
            );
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
                    EngineEvent::Envelope(_)
                    | EngineEvent::Text(_)
                    | EngineEvent::Reasoning(_)
                    | EngineEvent::ReasoningDelta { .. } => None,
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
        async fn forwards_reasoning_whole_block_before_text_with_signature_intact() {
            let engine = RigEngine::new(
                ScriptedFake::new(vec![
                    RawStreamingChoice::Reasoning {
                        id: Some("r1".to_string()),
                        content: rig::message::ReasoningContent::Text {
                            text: "thinking".to_string(),
                            signature: Some("sig".to_string()),
                        },
                    },
                    RawStreamingChoice::Message("answer".to_string()),
                    RawStreamingChoice::FinalResponse(FakeStreamingResponse),
                ]),
                "fake",
                "fake-model-id",
            );
            let stream = engine
                .stream(
                    EngineInput::with_history(vec![rig::message::Message::user("ask")]),
                    &Settings::default(),
                    &[],
                    "label",
                )
                .expect("stream() should succeed");
            let events: Vec<EngineEvent> = stream.collect().await;

            let kinds: Vec<&'static str> = events
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
            assert_eq!(
                kinds,
                vec!["envelope", "reasoning", "text", "final"],
                "rig adapter must yield Envelope first, then forward Reasoning before Text, then Final; got {events:?}"
            );

            let reasoning = events
                .iter()
                .find_map(|e| match e {
                    EngineEvent::Reasoning(r) => Some(r),
                    _ => None,
                })
                .expect("Reasoning event present");
            assert_eq!(reasoning.id.as_deref(), Some("r1"));
            assert_eq!(reasoning.content.len(), 1);
            let rig::message::ReasoningContent::Text { text, signature } = &reasoning.content[0]
            else {
                panic!(
                    "expected Text reasoning block, got {:?}",
                    reasoning.content[0]
                );
            };
            assert_eq!(text, "thinking");
            assert_eq!(signature.as_deref(), Some("sig"));
        }

        #[tokio::test]
        async fn forwards_reasoning_deltas_with_matching_id() {
            let engine = RigEngine::new(
                ScriptedFake::new(vec![
                    RawStreamingChoice::ReasoningDelta {
                        id: Some("r1".to_string()),
                        reasoning: "ab".to_string(),
                    },
                    RawStreamingChoice::ReasoningDelta {
                        id: Some("r1".to_string()),
                        reasoning: "cd".to_string(),
                    },
                    RawStreamingChoice::FinalResponse(FakeStreamingResponse),
                ]),
                "fake",
                "fake-model-id",
            );
            let stream = engine
                .stream(
                    EngineInput::with_history(vec![rig::message::Message::user("ask")]),
                    &Settings::default(),
                    &[],
                    "label",
                )
                .expect("stream() should succeed");
            let events: Vec<EngineEvent> = stream.collect().await;

            let deltas: Vec<(Option<&str>, &str)> = events
                .iter()
                .filter_map(|e| match e {
                    EngineEvent::ReasoningDelta { id, text } => {
                        Some((id.as_deref(), text.as_str()))
                    }
                    _ => None,
                })
                .collect();

            assert_eq!(
                deltas,
                vec![(Some("r1"), "ab"), (Some("r1"), "cd")],
                "rig adapter must forward both ReasoningDelta items with matching id and texts in arrival order; got {events:?}"
            );
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
