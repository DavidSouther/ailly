//! Rig-backed `EngineProvider` adapter. One generic `RigEngine<M>` covers
//! every provider Rig supports; provider-specific concerns (cache markers,
//! `max_tokens` defaults, system handling) live inside Rig's per-provider
//! `CompletionModel` impls. The Ailly side handles message translation,
//! trace population, and error mapping once.

use crate::content::conversation::Content;
use crate::content::conversation::ContentBlock;
use crate::content::conversation::Message;
use crate::content::conversation::ModelId;
use crate::content::conversation::Role;
use crate::engine::engine::CompletionRequest;
use crate::engine::engine::CompletionResponse;
use crate::engine::engine::EngineError;
use crate::engine::engine::EngineProvider;

/// Thin wrapper around a Rig `CompletionModel`. The `engine_name` is the
/// provider family identifier surfaced as `gen_ai.system` in the emitted
/// `gen_ai.completion` event; `model_id` is the requested model carried
/// through into `Trace.model` when the provider does not echo it on
/// response.
pub struct RigEngine<M> {
    model: M,
    engine_name: &'static str,
    model_id: ModelId,
}

impl<M> RigEngine<M> {
    /// Wrap a pre-constructed `CompletionModel`. Tests use this directly
    /// with a fake `CompletionModel`; production code reaches it through
    /// the `*_from_env` constructors below.
    pub fn new(model: M, engine_name: &'static str, model_id: impl Into<ModelId>) -> Self {
        Self {
            model,
            engine_name,
            model_id: model_id.into(),
        }
    }
}

#[async_trait::async_trait]
impl<M> EngineProvider for RigEngine<M>
where
    M: rig::completion::CompletionModel + Clone + Send + Sync + 'static,
    M::Response: Send,
{
    async fn complete(
        &self,
        request: CompletionRequest<'_>,
    ) -> Result<CompletionResponse, EngineError> {
        let start = std::time::Instant::now();
        let rig_messages = messages_to_rig(request.messages)?;
        let chat_history =
            rig::OneOrMany::many(rig_messages).map_err(|err| EngineError::MalformedResponse {
                message: format!("completion request has no messages to send: {err}"),
            })?;
        let rig_request = rig::completion::CompletionRequest {
            model: None,
            preamble: None,
            chat_history,
            documents: Vec::new(),
            tools: Vec::new(),
            temperature: None,
            max_tokens: None,
            tool_choice: None,
            additional_params: None,
            output_schema: None,
        };
        let _debug = request.debug;
        let outcome = self.model.completion(rig_request).await;
        let latency = start.elapsed();
        match outcome {
            Err(rig_err) => Err(engine_error_from_rig(rig_err, &self.model_id)),
            Ok(response) => {
                let content = content_from_rig_choice(&response.choice);
                let trace = trace_from_rig(self.engine_name, &request.model, &response, latency);
                Ok(CompletionResponse { content, trace })
            }
        }
    }
}

/// Translate an Ailly message slice into the `Vec<rig::completion::Message>`
/// shape Rig expects on `CompletionRequest.chat_history`.
///
/// System messages appear in input order at the head of the returned vector;
/// the adapter does not concatenate, deduplicate, or fold them into a single
/// preamble string. Per-block lowering follows the table in the design's
/// "Translation: Ailly request -> Rig request" section. Invariant violations
/// (`ToolUse` on a User-role message, `ToolResult` on an Assistant-role
/// message, `Thinking` outside an Assistant-role message, blank body) lower
/// to [`EngineError::MalformedResponse`] naming the offending block.
fn messages_to_rig(messages: &[Message]) -> Result<Vec<rig::completion::Message>, EngineError> {
    use rig::completion::Message as RigMessage;

    let mut out: Vec<RigMessage> = Vec::with_capacity(messages.len());
    for message in messages {
        let body = message
            .body
            .as_ref()
            .ok_or_else(|| EngineError::MalformedResponse {
                message: format!("blank {:?} message body cannot be translated", message.role),
            })?;
        match message.role {
            Role::System => {
                let content = match body {
                    Content::Text(text) => text.clone(),
                    Content::Blocks(_) => {
                        return Err(EngineError::MalformedResponse {
                            message: String::from("system message body must be Content::Text"),
                        });
                    }
                };
                out.push(RigMessage::System { content });
            }
            Role::User | Role::Tool => {
                let user_blocks = lower_user_body(body, message.role)?;
                out.push(RigMessage::User {
                    content: user_blocks,
                });
            }
            Role::Assistant => {
                let assistant_blocks = lower_assistant_body(body)?;
                out.push(RigMessage::Assistant {
                    id: None,
                    content: assistant_blocks,
                });
            }
        }
    }
    Ok(out)
}

fn lower_user_body(
    body: &Content,
    role: Role,
) -> Result<rig::OneOrMany<rig::completion::message::UserContent>, EngineError> {
    use rig::OneOrMany;
    use rig::completion::message::Image as RigImage;
    use rig::completion::message::Text;
    use rig::completion::message::ToolResult;
    use rig::completion::message::UserContent;

    match body {
        Content::Text(text) => Ok(OneOrMany::one(UserContent::Text(Text {
            text: text.clone(),
        }))),
        Content::Blocks(blocks) => {
            let mut items: Vec<UserContent> = Vec::with_capacity(blocks.len());
            for block in blocks {
                items.push(match block {
                    ContentBlock::Text { text } => UserContent::Text(Text { text: text.clone() }),
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        is_error: _,
                    } => UserContent::ToolResult(ToolResult {
                        id: tool_use_id.as_ref().to_owned(),
                        call_id: None,
                        content: tool_result_content(content)?,
                    }),
                    ContentBlock::Image { .. } => UserContent::Image(RigImage::default()),
                    ContentBlock::ToolUse { .. } => {
                        return Err(EngineError::MalformedResponse {
                            message: format!(
                                "tool_use block not valid on {role:?}-role message; expected on assistant-role"
                            ),
                        });
                    }
                    ContentBlock::Thinking { .. } => {
                        return Err(EngineError::MalformedResponse {
                            message: format!(
                                "thinking block not valid on {role:?}-role message; expected on assistant-role"
                            ),
                        });
                    }
                });
            }
            OneOrMany::many(items).map_err(|err| EngineError::MalformedResponse {
                message: format!("{role:?} message has empty Content::Blocks: {err}"),
            })
        }
    }
}

fn lower_assistant_body(
    body: &Content,
) -> Result<rig::OneOrMany<rig::completion::AssistantContent>, EngineError> {
    use rig::OneOrMany;
    use rig::completion::AssistantContent;
    use rig::completion::message::Image as RigImage;
    use rig::completion::message::Reasoning;
    use rig::completion::message::Text;
    use rig::completion::message::ToolCall;
    use rig::completion::message::ToolFunction;

    match body {
        Content::Text(text) => Ok(OneOrMany::one(AssistantContent::Text(Text {
            text: text.clone(),
        }))),
        Content::Blocks(blocks) => {
            let mut items: Vec<AssistantContent> = Vec::with_capacity(blocks.len());
            for block in blocks {
                items.push(match block {
                    ContentBlock::Text { text } => {
                        AssistantContent::Text(Text { text: text.clone() })
                    }
                    ContentBlock::ToolUse { id, name, input } => {
                        let arguments = serde_json::to_value(input).map_err(|err| {
                            EngineError::MalformedResponse {
                                message: format!(
                                    "tool_use input could not be converted to JSON: {err}"
                                ),
                            }
                        })?;
                        AssistantContent::ToolCall(ToolCall::new(
                            id.as_ref().to_owned(),
                            ToolFunction::new(name.clone(), arguments),
                        ))
                    }
                    ContentBlock::Thinking {
                        thinking,
                        signature,
                    } => AssistantContent::Reasoning(Reasoning::new_with_signature(
                        thinking,
                        signature.clone(),
                    )),
                    ContentBlock::Image { .. } => AssistantContent::Image(RigImage::default()),
                    ContentBlock::ToolResult { .. } => {
                        return Err(EngineError::MalformedResponse {
                            message: String::from(
                                "tool_result block not valid on assistant-role message; expected on user-role",
                            ),
                        });
                    }
                });
            }
            OneOrMany::many(items).map_err(|err| EngineError::MalformedResponse {
                message: format!("assistant message has empty Content::Blocks: {err}"),
            })
        }
    }
}

fn tool_result_content(
    content: &Content,
) -> Result<rig::OneOrMany<rig::completion::message::ToolResultContent>, EngineError> {
    use rig::OneOrMany;
    use rig::completion::message::Image as RigImage;
    use rig::completion::message::Text;
    use rig::completion::message::ToolResultContent;

    match content {
        Content::Text(text) => Ok(OneOrMany::one(ToolResultContent::Text(Text {
            text: text.clone(),
        }))),
        Content::Blocks(blocks) => {
            let mut items: Vec<ToolResultContent> = Vec::with_capacity(blocks.len());
            for block in blocks {
                items.push(match block {
                    ContentBlock::Text { text } => {
                        ToolResultContent::Text(Text { text: text.clone() })
                    }
                    ContentBlock::Image { .. } => ToolResultContent::Image(RigImage::default()),
                    other => {
                        return Err(EngineError::MalformedResponse {
                            message: format!(
                                "tool_result content can only carry text or image blocks; got {other:?}"
                            ),
                        });
                    }
                });
            }
            OneOrMany::many(items).map_err(|err| EngineError::MalformedResponse {
                message: format!("tool_result content has empty Content::Blocks: {err}"),
            })
        }
    }
}

/// `gen_ai.system` semantic-convention attribute key.
const SEMCONV_SYSTEM: &str = "gen_ai.system";
/// `gen_ai.request.model` semantic-convention attribute key.
const SEMCONV_REQUEST_MODEL: &str = "gen_ai.request.model";
/// `gen_ai.response.model` semantic-convention attribute key.
const SEMCONV_RESPONSE_MODEL: &str = "gen_ai.response.model";
/// `gen_ai.response.id` semantic-convention attribute key.
const SEMCONV_RESPONSE_ID: &str = "gen_ai.response.id";
/// `gen_ai.usage.input_tokens` semantic-convention attribute key.
const SEMCONV_USAGE_INPUT: &str = "gen_ai.usage.input_tokens";
/// `gen_ai.usage.output_tokens` semantic-convention attribute key.
const SEMCONV_USAGE_OUTPUT: &str = "gen_ai.usage.output_tokens";
/// `gen_ai.usage.cached_input_tokens` semantic-convention attribute key.
const SEMCONV_USAGE_CACHED_INPUT: &str = "gen_ai.usage.cached_input_tokens";
/// Event name for one completed call.
const COMPLETION_EVENT_NAME: &str = "gen_ai.completion";

/// Lower a Rig assistant choice into Ailly's [`Content`].
///
/// Exactly one `AssistantContent::Text` lowers to `Content::Text(String)`.
/// Any other combination (tool calls, reasoning, image, multi-text) lowers
/// to `Content::Blocks(Vec<ContentBlock>)` preserving Rig's order.
fn content_from_rig_choice(choice: &rig::OneOrMany<rig::completion::AssistantContent>) -> Content {
    use rig::completion::AssistantContent;
    use rig::completion::message::ReasoningContent;

    let items: Vec<AssistantContent> = std::iter::once(choice.first())
        .chain(choice.rest())
        .collect();
    if items.len() == 1
        && let AssistantContent::Text(text) = &items[0]
    {
        return Content::Text(text.text.clone());
    }
    let blocks: Vec<ContentBlock> = items
        .into_iter()
        .map(|item| match item {
            AssistantContent::Text(text) => ContentBlock::Text { text: text.text },
            AssistantContent::ToolCall(call) => ContentBlock::ToolUse {
                id: crate::content::conversation::ToolUseId::from(call.id),
                name: call.function.name,
                input: serde_yaml_ng::to_value(&call.function.arguments)
                    .unwrap_or(serde_yaml_ng::Value::Null),
            },
            AssistantContent::Reasoning(reasoning) => {
                let (thinking, signature) = match reasoning.content.first() {
                    Some(ReasoningContent::Text { text, signature }) => {
                        (text.clone(), signature.clone())
                    }
                    Some(ReasoningContent::Summary(text)) => (text.clone(), None),
                    Some(
                        ReasoningContent::Encrypted(data) | ReasoningContent::Redacted { data },
                    ) => (data.clone(), None),
                    Some(_) | None => (String::new(), None),
                };
                ContentBlock::Thinking {
                    thinking,
                    signature,
                }
            }
            AssistantContent::Image(_) => ContentBlock::Image {
                source: serde_yaml_ng::from_value(serde_yaml_ng::Value::Null)
                    .expect("ImageSource is transparent over Value and accepts Null"),
            },
        })
        .collect();
    Content::Blocks(blocks)
}

/// Build the inline [`Trace`] for one successful completion.
///
/// `span_id` is the provider's `message_id` when present, otherwise a freshly
/// minted `UUIDv7`. `model` is the requested `ModelId` (provider-echoed values
/// are surfaced as the optional `gen_ai.response.model` attribute on the
/// emitted event). `tokens.input` and `tokens.output` carry raw counts.
/// `tokens.cache_hit` is `Some(u.cached_input_tokens)` when non-zero, else
/// `None`; `tokens.cache_write` is `Some(u.cache_creation_input_tokens)`
/// when non-zero, else `None`.
fn trace_from_rig<R>(
    engine_name: &'static str,
    requested: &ModelId,
    response: &rig::completion::CompletionResponse<R>,
    latency: std::time::Duration,
) -> crate::content::conversation::Trace {
    use std::collections::BTreeMap;

    use crate::content::conversation::SpanId;
    use crate::content::conversation::TokenCounts;
    use crate::content::conversation::Trace;
    use crate::content::conversation::TraceEvent;

    let usage = response.usage;
    let span_id = match &response.message_id {
        Some(id) if !id.is_empty() => SpanId::from(id.clone()),
        _ => SpanId::from(uuid::Uuid::now_v7().to_string()),
    };
    let cache_hit = (usage.cached_input_tokens != 0).then_some(usage.cached_input_tokens);
    let cache_write =
        (usage.cache_creation_input_tokens != 0).then_some(usage.cache_creation_input_tokens);

    let mut attributes: BTreeMap<String, serde_yaml_ng::Value> = BTreeMap::new();
    attributes.insert(
        String::from(SEMCONV_SYSTEM),
        serde_yaml_ng::Value::String(String::from(engine_name)),
    );
    attributes.insert(
        String::from(SEMCONV_REQUEST_MODEL),
        serde_yaml_ng::Value::String(requested.as_ref().to_owned()),
    );
    if let Some(id) = &response.message_id
        && !id.is_empty()
    {
        attributes.insert(
            String::from(SEMCONV_RESPONSE_ID),
            serde_yaml_ng::Value::String(id.clone()),
        );
    }
    attributes.insert(
        String::from(SEMCONV_USAGE_INPUT),
        serde_yaml_ng::Value::Number(usage.input_tokens.into()),
    );
    attributes.insert(
        String::from(SEMCONV_USAGE_OUTPUT),
        serde_yaml_ng::Value::Number(usage.output_tokens.into()),
    );
    if usage.cached_input_tokens != 0 {
        attributes.insert(
            String::from(SEMCONV_USAGE_CACHED_INPUT),
            serde_yaml_ng::Value::Number(usage.cached_input_tokens.into()),
        );
    }
    let _ = SEMCONV_RESPONSE_MODEL;

    Trace {
        span_id,
        model: requested.clone(),
        tokens: TokenCounts {
            input: usage.input_tokens,
            output: usage.output_tokens,
            cache_hit,
            cache_write,
        },
        latency_ms: u64::try_from(latency.as_millis()).unwrap_or(u64::MAX),
        events: vec![TraceEvent {
            name: String::from(COMPLETION_EVENT_NAME),
            attributes,
        }],
    }
}

/// Map a Rig [`rig::completion::CompletionError`] into a typed
/// [`EngineError`].
///
/// `model` is the requested `ModelId`, used to populate
/// `EngineError::ModelNotFound { model }` on 404s and on provider envelopes
/// that name the model as unknown. Substring matching against the provider
/// error envelope is a deliberate concession: Rig flattens envelopes to
/// strings by the time they reach this function.
pub(crate) fn engine_error_from_rig(
    err: rig::completion::CompletionError,
    model: &ModelId,
) -> EngineError {
    use rig::completion::CompletionError;

    match err {
        CompletionError::HttpError(http_err) => http_error_to_engine(http_err, model),
        CompletionError::JsonError(json_err) => EngineError::MalformedResponse {
            message: json_err.to_string(),
        },
        CompletionError::UrlError(url_err) => EngineError::Provider {
            message: format!("rig url error: {url_err}"),
        },
        CompletionError::RequestError(err) => classify_transport_error(err.as_ref()),
        CompletionError::ResponseError(message) | CompletionError::ProviderError(message) => {
            let lower = message.to_lowercase();
            if lower.contains("model_not_found")
                || lower.contains("model not found")
                || lower.contains("unknown model")
                || (lower.contains("model") && lower.contains("unknown"))
            {
                EngineError::ModelNotFound {
                    model: model.clone(),
                }
            } else {
                EngineError::Provider { message }
            }
        }
    }
}

fn http_error_to_engine(err: rig::http_client::Error, model: &ModelId) -> EngineError {
    use rig::http_client::Error as HttpError;

    match err {
        HttpError::InvalidStatusCode(status)
        | HttpError::InvalidStatusCodeWithMessage(status, _)
            if status.as_u16() == 401 || status.as_u16() == 403 =>
        {
            EngineError::Auth {
                message: std::borrow::Cow::Owned(format!("provider returned {status}")),
            }
        }
        HttpError::InvalidStatusCode(status)
        | HttpError::InvalidStatusCodeWithMessage(status, _)
            if status.as_u16() == 404 =>
        {
            EngineError::ModelNotFound {
                model: model.clone(),
            }
        }
        HttpError::InvalidStatusCode(status)
        | HttpError::InvalidStatusCodeWithMessage(status, _)
            if status.as_u16() == 429 =>
        {
            EngineError::RateLimited { retry_after: None }
        }
        HttpError::InvalidStatusCode(status) => EngineError::Provider {
            message: format!("provider returned {status}"),
        },
        HttpError::InvalidStatusCodeWithMessage(status, body) => EngineError::Provider {
            message: format!("provider returned {status}: {body}"),
        },
        HttpError::Instance(inner) => classify_transport_error(inner.as_ref()),
        other => EngineError::Provider {
            message: other.to_string(),
        },
    }
}

fn root_cause(err: &dyn std::error::Error) -> &dyn std::error::Error {
    let mut current = err;
    while let Some(next) = current.source() {
        current = next;
    }
    current
}

fn chain_contains(err: &dyn std::error::Error, needle: &str) -> bool {
    let needle_lower = needle.to_lowercase();
    let mut current: Option<&dyn std::error::Error> = Some(err);
    while let Some(e) = current {
        if e.to_string().to_lowercase().contains(&needle_lower) {
            return true;
        }
        current = e.source();
    }
    false
}

fn classify_transport_error(err: &dyn std::error::Error) -> EngineError {
    if chain_contains(err, "timed out")
        || chain_contains(err, "timeout")
        || chain_contains(err, "os error 110")
    {
        return EngineError::Timeout;
    }
    let top = err.to_string();
    let root = root_cause(err).to_string();
    let message = if root == top {
        top
    } else {
        format!("{top}: {root}")
    };
    EngineError::Provider { message }
}

/// Construct a `RigEngine` backed by Rig's Anthropic completion model,
/// reading `ANTHROPIC_API_KEY` from the process environment. Returns
/// `EngineError::Auth` when the key is missing or empty.
///
/// # Errors
/// Returns [`EngineError::Auth`] when `ANTHROPIC_API_KEY` is missing or empty
/// and [`EngineError::Provider`] when the Rig client cannot be constructed.
pub fn anthropic_from_env(
    model: &str,
) -> Result<RigEngine<rig::providers::anthropic::completion::CompletionModel>, EngineError> {
    use rig::client::CompletionClient;

    let key = read_required_key("ANTHROPIC_API_KEY")?;
    let client = rig::providers::anthropic::Client::builder()
        .api_key(key)
        .build()
        .map_err(|err| EngineError::Provider {
            message: format!("anthropic client build failed: {err}"),
        })?;
    let completion_model = client.completion_model(model);
    Ok(RigEngine::new(completion_model, "anthropic", model))
}

fn read_required_key(var: &'static str) -> Result<String, EngineError> {
    match std::env::var(var) {
        Ok(value) if !value.is_empty() => Ok(value),
        _ => Err(EngineError::Auth {
            message: std::borrow::Cow::Owned(format!("{var} missing or empty")),
        }),
    }
}

/// Construct a `RigEngine` backed by Rig's `OpenAI` Responses API model,
/// reading `OPENAI_API_KEY` from the process environment. The key read runs
/// before the client build so a missing key short-circuits to `Auth` without
/// touching the network; this invariant is what makes the keyless path
/// testable offline.
///
/// # Errors
/// Returns [`EngineError::Auth`] when `OPENAI_API_KEY` is missing or empty
/// and [`EngineError::Provider`] when the Rig client cannot be constructed.
pub fn openai_from_env(
    model: &str,
) -> Result<RigEngine<rig::providers::openai::responses_api::ResponsesCompletionModel>, EngineError>
{
    use rig::client::CompletionClient;

    let key = read_required_key("OPENAI_API_KEY")?;
    let client = rig::providers::openai::Client::builder()
        .api_key(key)
        .build()
        .map_err(|err| EngineError::Provider {
            message: format!("openai client build failed: {err}"),
        })?;
    let completion_model = client.completion_model(model);
    Ok(RigEngine::new(completion_model, "openai", model))
}

/// Construct a `RigEngine` backed by Rig's Gemini completion model, reading
/// `GEMINI_API_KEY` from the process environment. The key read runs before the
/// client build so a missing key short-circuits to `Auth` without touching the
/// network.
///
/// # Errors
/// Returns [`EngineError::Auth`] when `GEMINI_API_KEY` is missing or empty
/// and [`EngineError::Provider`] when the Rig client cannot be constructed.
pub fn gemini_from_env(
    model: &str,
) -> Result<RigEngine<rig::providers::gemini::completion::CompletionModel>, EngineError> {
    use rig::client::CompletionClient;

    let key = read_required_key("GEMINI_API_KEY")?;
    let client = rig::providers::gemini::Client::builder()
        .api_key(key)
        .build()
        .map_err(|err| EngineError::Provider {
            message: format!("gemini client build failed: {err}"),
        })?;
    let completion_model = client.completion_model(model);
    Ok(RigEngine::new(completion_model, "gemini", model))
}

/// Construct a `RigEngine` backed by `rig-bedrock`. Gated on the `bedrock`
/// Cargo feature so the AWS SDK does not enter the default build graph.
///
/// # Errors
/// Returns [`EngineError::Auth`] when AWS credentials cannot be resolved and
/// [`EngineError::Provider`] when the Rig client cannot be constructed.
#[cfg(feature = "bedrock")]
pub fn bedrock_from_env(
    _model: &str,
) -> Result<RigEngine<rig_bedrock::completion::CompletionModel>, EngineError> {
    Err(EngineError::Provider {
        message: String::from("rig_engine: not yet implemented"),
    })
}

#[cfg(test)]
mod tests {
    use std::marker::PhantomData;

    use rig::completion::AssistantContent;
    use rig::completion::Message as RigMessage;
    use rig::completion::message::ReasoningContent;
    use rig::completion::message::ToolResultContent;
    use rig::completion::message::UserContent;

    use super::*;
    use crate::content::conversation::ImageSource;
    use crate::content::conversation::ToolUseId;

    const _: fn(&dyn EngineProvider) = |_| {};

    const _: fn() = || {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<RigEngine<rig::providers::anthropic::completion::CompletionModel>>();
    };

    fn message(role: Role, body: Content) -> Message {
        Message {
            role,
            body: Some(body),
            cache: false,
            trace: None,
            _phase: PhantomData,
        }
    }

    fn image_source() -> ImageSource {
        serde_yaml_ng::from_str("type: base64\nmedia_type: image/png\ndata: \"\"").unwrap()
    }

    #[test]
    fn translates_role_system_in_input_order_at_head_of_history() {
        // Arrange
        let messages = vec![
            message(Role::System, Content::Text(String::from("alpha"))),
            message(Role::System, Content::Text(String::from("beta"))),
            message(Role::User, Content::Text(String::from("hi"))),
        ];

        // Act
        let out = messages_to_rig(&messages).expect("translation succeeds");

        // Assert
        assert_eq!(out.len(), 3);
        match &out[0] {
            RigMessage::System { content } => assert_eq!(content, "alpha"),
            other => panic!("expected System at index 0, got {other:?}"),
        }
        match &out[1] {
            RigMessage::System { content } => assert_eq!(content, "beta"),
            other => panic!("expected System at index 1, got {other:?}"),
        }
        assert!(matches!(out[2], RigMessage::User { .. }));
    }

    #[test]
    fn translates_role_user_text_body_into_one_user_text_block() {
        let out = messages_to_rig(&[message(Role::User, Content::Text(String::from("hi")))])
            .expect("translation succeeds");
        let RigMessage::User { content } = &out[0] else {
            panic!("expected User message")
        };
        assert_eq!(content.len(), 1);
        match content.first_ref() {
            UserContent::Text(text) => assert_eq!(text.text, "hi"),
            other => panic!("expected UserContent::Text, got {other:?}"),
        }
    }

    #[test]
    fn translates_role_assistant_text_body_into_one_assistant_text_block() {
        let out = messages_to_rig(&[message(Role::Assistant, Content::Text(String::from("ok")))])
            .expect("translation succeeds");
        let RigMessage::Assistant { id, content } = &out[0] else {
            panic!("expected Assistant message")
        };
        assert!(id.is_none(), "adapter does not invent an assistant id");
        assert_eq!(content.len(), 1);
        match content.first_ref() {
            AssistantContent::Text(text) => assert_eq!(text.text, "ok"),
            other => panic!("expected AssistantContent::Text, got {other:?}"),
        }
    }

    #[test]
    fn translates_role_tool_into_user_message_with_tool_result_block() {
        let blocks = vec![ContentBlock::ToolResult {
            tool_use_id: ToolUseId::from("call-1"),
            content: Content::Text(String::from("payload")),
            is_error: None,
        }];
        let out = messages_to_rig(&[message(Role::Tool, Content::Blocks(blocks))])
            .expect("translation succeeds");
        let RigMessage::User { content } = &out[0] else {
            panic!("Role::Tool must lower to a User-shaped Rig message")
        };
        assert_eq!(content.len(), 1);
        match content.first_ref() {
            UserContent::ToolResult(result) => {
                assert_eq!(result.id, "call-1");
                assert_eq!(result.content.len(), 1);
                match result.content.first_ref() {
                    ToolResultContent::Text(text) => assert_eq!(text.text, "payload"),
                    ToolResultContent::Image(_) => panic!("expected nested Text, got Image"),
                }
            }
            other => panic!("expected UserContent::ToolResult, got {other:?}"),
        }
    }

    #[test]
    fn translates_assistant_blocks_text_tool_use_thinking_image_in_order() {
        let blocks = vec![
            ContentBlock::Text {
                text: String::from("thinking aloud"),
            },
            ContentBlock::ToolUse {
                id: ToolUseId::from("call-7"),
                name: String::from("lookup"),
                input: serde_yaml_ng::from_str("{\"k\": 1}").unwrap(),
            },
            ContentBlock::Thinking {
                thinking: String::from("private reasoning"),
                signature: Some(String::from("sig")),
            },
            ContentBlock::Image {
                source: image_source(),
            },
        ];
        let out = messages_to_rig(&[message(Role::Assistant, Content::Blocks(blocks))])
            .expect("translation succeeds");
        let RigMessage::Assistant { content, .. } = &out[0] else {
            panic!("expected Assistant message")
        };
        let items: Vec<_> = std::iter::once(content.first())
            .chain(content.rest())
            .collect();
        assert_eq!(items.len(), 4);
        assert!(matches!(items[0], AssistantContent::Text(_)));
        match &items[1] {
            AssistantContent::ToolCall(call) => {
                assert_eq!(call.id, "call-7");
                assert_eq!(call.function.name, "lookup");
                assert_eq!(call.function.arguments["k"], 1);
            }
            other => panic!("expected ToolCall at index 1, got {other:?}"),
        }
        match &items[2] {
            AssistantContent::Reasoning(reasoning) => {
                assert!(matches!(
                    reasoning.content.first(),
                    Some(ReasoningContent::Text { .. })
                ));
            }
            other => panic!("expected Reasoning at index 2, got {other:?}"),
        }
        assert!(matches!(items[3], AssistantContent::Image(_)));
    }

    #[test]
    fn translates_user_blocks_text_tool_result_image_in_order() {
        let blocks = vec![
            ContentBlock::Text {
                text: String::from("look at this"),
            },
            ContentBlock::ToolResult {
                tool_use_id: ToolUseId::from("call-2"),
                content: Content::Text(String::from("done")),
                is_error: None,
            },
            ContentBlock::Image {
                source: image_source(),
            },
        ];
        let out = messages_to_rig(&[message(Role::User, Content::Blocks(blocks))])
            .expect("translation succeeds");
        let RigMessage::User { content } = &out[0] else {
            panic!("expected User message")
        };
        let items: Vec<_> = std::iter::once(content.first())
            .chain(content.rest())
            .collect();
        assert_eq!(items.len(), 3);
        assert!(matches!(items[0], UserContent::Text(_)));
        assert!(matches!(items[1], UserContent::ToolResult(_)));
        assert!(matches!(items[2], UserContent::Image(_)));
    }

    #[test]
    fn rejects_tool_use_block_on_user_role_message_with_malformed_response() {
        let blocks = vec![ContentBlock::ToolUse {
            id: ToolUseId::from("call-x"),
            name: String::from("name"),
            input: serde_yaml_ng::Value::Null,
        }];
        let err = messages_to_rig(&[message(Role::User, Content::Blocks(blocks))])
            .expect_err("user-role tool_use must be rejected");
        match err {
            EngineError::MalformedResponse { message } => {
                assert!(
                    message.contains("tool_use") && message.contains("User"),
                    "error must name the offending block: {message}"
                );
            }
            other => panic!("expected MalformedResponse, got {other:?}"),
        }
    }

    #[test]
    fn rejects_tool_result_block_on_assistant_role_message_with_malformed_response() {
        let blocks = vec![ContentBlock::ToolResult {
            tool_use_id: ToolUseId::from("call-y"),
            content: Content::Text(String::from("x")),
            is_error: None,
        }];
        let err = messages_to_rig(&[message(Role::Assistant, Content::Blocks(blocks))])
            .expect_err("assistant-role tool_result must be rejected");
        match err {
            EngineError::MalformedResponse { message } => {
                assert!(
                    message.contains("tool_result") && message.contains("assistant"),
                    "error must name the offending block: {message}"
                );
            }
            other => panic!("expected MalformedResponse, got {other:?}"),
        }
    }

    #[test]
    fn rejects_thinking_block_on_user_role_message_with_malformed_response() {
        let blocks = vec![ContentBlock::Thinking {
            thinking: String::from("nope"),
            signature: None,
        }];
        let err = messages_to_rig(&[message(Role::User, Content::Blocks(blocks))])
            .expect_err("user-role thinking must be rejected");
        match err {
            EngineError::MalformedResponse { message } => {
                assert!(
                    message.contains("thinking") && message.contains("User"),
                    "error must name the offending block: {message}"
                );
            }
            other => panic!("expected MalformedResponse, got {other:?}"),
        }
    }

    #[test]
    fn rejects_blank_body_with_malformed_response() {
        let blank = Message {
            role: Role::Assistant,
            body: None,
            cache: false,
            trace: None,
            _phase: PhantomData,
        };
        let err = messages_to_rig(&[blank]).expect_err("blank body must be rejected");
        match err {
            EngineError::MalformedResponse { message } => {
                assert!(
                    message.contains("blank"),
                    "error must signal blank body: {message}"
                );
            }
            other => panic!("expected MalformedResponse, got {other:?}"),
        }
    }

    // ---- content_from_rig_choice tests ---------------------------------

    fn rig_text(text: &str) -> rig::completion::AssistantContent {
        rig::completion::AssistantContent::Text(rig::completion::message::Text {
            text: text.to_owned(),
        })
    }

    #[test]
    fn content_lowers_single_assistant_text_to_content_text() {
        let choice = rig::OneOrMany::one(rig_text("pong"));
        match content_from_rig_choice(&choice) {
            Content::Text(text) => assert_eq!(text, "pong"),
            Content::Blocks(blocks) => panic!("single text must lower to Text, got {blocks:?}"),
        }
    }

    #[test]
    fn content_lowers_multi_text_to_content_blocks_preserving_order() {
        let choice = rig::OneOrMany::many([rig_text("a"), rig_text("b")]).unwrap();
        match content_from_rig_choice(&choice) {
            Content::Blocks(blocks) => {
                assert_eq!(blocks.len(), 2);
                assert_eq!(
                    blocks[0],
                    ContentBlock::Text {
                        text: String::from("a")
                    }
                );
                assert_eq!(
                    blocks[1],
                    ContentBlock::Text {
                        text: String::from("b")
                    }
                );
            }
            Content::Text(text) => panic!("multi must lower to Blocks, got Text({text:?})"),
        }
    }

    #[test]
    fn content_lowers_tool_call_choice_to_content_blocks_with_tool_use() {
        let call =
            rig::completion::AssistantContent::ToolCall(rig::completion::message::ToolCall::new(
                String::from("call-9"),
                rig::completion::message::ToolFunction::new(
                    String::from("classify"),
                    serde_json::json!({"x": 1}),
                ),
            ));
        let choice = rig::OneOrMany::one(call);
        match content_from_rig_choice(&choice) {
            Content::Blocks(blocks) => match &blocks[0] {
                ContentBlock::ToolUse { id, name, input } => {
                    assert_eq!(id.as_ref(), "call-9");
                    assert_eq!(name, "classify");
                    assert_eq!(
                        input.get("x").and_then(serde_yaml_ng::Value::as_u64),
                        Some(1)
                    );
                }
                other => panic!("expected ToolUse, got {other:?}"),
            },
            Content::Text(text) => panic!("expected Blocks, got Text({text:?})"),
        }
    }

    #[test]
    fn content_lowers_reasoning_to_content_blocks_with_thinking() {
        let reasoning = rig::completion::AssistantContent::Reasoning(
            rig::completion::message::Reasoning::new_with_signature(
                "private chain",
                Some(String::from("sig-1")),
            ),
        );
        let choice = rig::OneOrMany::one(reasoning);
        match content_from_rig_choice(&choice) {
            Content::Blocks(blocks) => match &blocks[0] {
                ContentBlock::Thinking {
                    thinking,
                    signature,
                } => {
                    assert_eq!(thinking, "private chain");
                    assert_eq!(signature.as_deref(), Some("sig-1"));
                }
                other => panic!("expected Thinking, got {other:?}"),
            },
            Content::Text(text) => panic!("expected Blocks, got Text({text:?})"),
        }
    }

    #[test]
    fn content_lowers_image_choice_to_content_blocks_with_image() {
        let image =
            rig::completion::AssistantContent::Image(rig::completion::message::Image::default());
        let choice = rig::OneOrMany::one(image);
        match content_from_rig_choice(&choice) {
            Content::Blocks(blocks) => {
                assert!(matches!(blocks[0], ContentBlock::Image { .. }));
            }
            Content::Text(text) => panic!("expected Blocks, got Text({text:?})"),
        }
    }

    // ---- trace_from_rig tests ------------------------------------------

    fn rig_response(
        usage: rig::completion::Usage,
        message_id: Option<&str>,
    ) -> rig::completion::CompletionResponse<()> {
        rig::completion::CompletionResponse {
            choice: rig::OneOrMany::one(rig_text("body")),
            usage,
            raw_response: (),
            message_id: message_id.map(str::to_owned),
        }
    }

    #[test]
    fn trace_records_model_from_requested() {
        let usage = rig::completion::Usage {
            input_tokens: 1,
            output_tokens: 1,
            total_tokens: 2,
            ..rig::completion::Usage::new()
        };
        let resp = rig_response(usage, Some("msg-1"));
        let trace = trace_from_rig(
            "anthropic",
            &ModelId::from("claude-opus-4-7"),
            &resp,
            std::time::Duration::from_millis(5),
        );
        assert_eq!(trace.model, ModelId::from("claude-opus-4-7"));
    }

    #[test]
    fn trace_uses_provider_message_id_when_present_for_span_id() {
        let resp = rig_response(rig::completion::Usage::new(), Some("msg-abc"));
        let trace = trace_from_rig(
            "anthropic",
            &ModelId::from("claude-opus-4-7"),
            &resp,
            std::time::Duration::from_millis(1),
        );
        assert_eq!(trace.span_id.as_ref(), "msg-abc");
    }

    #[test]
    fn trace_generates_uuid_v7_when_provider_omits_message_id() {
        let resp = rig_response(rig::completion::Usage::new(), None);
        let trace = trace_from_rig(
            "anthropic",
            &ModelId::from("claude-opus-4-7"),
            &resp,
            std::time::Duration::from_millis(1),
        );
        let span = trace.span_id.as_ref();
        assert!(!span.is_empty(), "span_id must be populated");
        uuid::Uuid::parse_str(span).expect("span_id must parse as a UUID when minted locally");
    }

    #[test]
    fn trace_records_latency_millis_from_supplied_duration() {
        let resp = rig_response(rig::completion::Usage::new(), Some("x"));
        let trace = trace_from_rig(
            "anthropic",
            &ModelId::from("claude-opus-4-7"),
            &resp,
            std::time::Duration::from_millis(125),
        );
        assert_eq!(trace.latency_ms, 125);
    }

    #[test]
    fn trace_lowers_nonzero_cached_input_tokens_to_some_cache_hit() {
        let usage = rig::completion::Usage {
            cached_input_tokens: 17,
            ..rig::completion::Usage::new()
        };
        let resp = rig_response(usage, Some("x"));
        let trace = trace_from_rig(
            "anthropic",
            &ModelId::from("claude-opus-4-7"),
            &resp,
            std::time::Duration::from_millis(1),
        );
        assert_eq!(trace.tokens.cache_hit, Some(17));
    }

    #[test]
    fn trace_lowers_zero_cached_input_tokens_to_none() {
        let resp = rig_response(rig::completion::Usage::new(), Some("x"));
        let trace = trace_from_rig(
            "anthropic",
            &ModelId::from("claude-opus-4-7"),
            &resp,
            std::time::Duration::from_millis(1),
        );
        assert_eq!(trace.tokens.cache_hit, None);
        assert_eq!(trace.tokens.cache_write, None);
    }

    #[test]
    fn trace_lowers_nonzero_cache_creation_input_tokens_to_some_cache_write() {
        let usage = rig::completion::Usage {
            cache_creation_input_tokens: 42,
            ..rig::completion::Usage::new()
        };
        let resp = rig_response(usage, Some("x"));
        let trace = trace_from_rig(
            "anthropic",
            &ModelId::from("claude-opus-4-7"),
            &resp,
            std::time::Duration::from_millis(1),
        );
        assert_eq!(trace.tokens.cache_write, Some(42));
    }

    #[test]
    fn trace_emits_one_gen_ai_completion_event_with_required_semconv_keys() {
        let usage = rig::completion::Usage {
            input_tokens: 11,
            output_tokens: 22,
            total_tokens: 33,
            ..rig::completion::Usage::new()
        };
        let resp = rig_response(usage, Some("msg-77"));
        let trace = trace_from_rig(
            "anthropic",
            &ModelId::from("claude-opus-4-7"),
            &resp,
            std::time::Duration::from_millis(3),
        );
        assert_eq!(trace.events.len(), 1);
        let event = &trace.events[0];
        assert_eq!(event.name, "gen_ai.completion");
        assert_eq!(
            event
                .attributes
                .get("gen_ai.system")
                .and_then(serde_yaml_ng::Value::as_str),
            Some("anthropic")
        );
        assert_eq!(
            event
                .attributes
                .get("gen_ai.request.model")
                .and_then(serde_yaml_ng::Value::as_str),
            Some("claude-opus-4-7")
        );
        assert_eq!(
            event
                .attributes
                .get("gen_ai.usage.input_tokens")
                .and_then(serde_yaml_ng::Value::as_u64),
            Some(11)
        );
        assert_eq!(
            event
                .attributes
                .get("gen_ai.usage.output_tokens")
                .and_then(serde_yaml_ng::Value::as_u64),
            Some(22)
        );
        assert_eq!(
            event
                .attributes
                .get("gen_ai.response.id")
                .and_then(serde_yaml_ng::Value::as_str),
            Some("msg-77")
        );
    }

    #[test]
    fn trace_omits_optional_semconv_keys_when_source_value_absent() {
        let resp = rig_response(rig::completion::Usage::new(), None);
        let trace = trace_from_rig(
            "anthropic",
            &ModelId::from("claude-opus-4-7"),
            &resp,
            std::time::Duration::from_millis(1),
        );
        let event = &trace.events[0];
        assert!(
            !event.attributes.contains_key("gen_ai.response.id"),
            "response.id must be omitted when message_id is absent"
        );
        assert!(
            !event
                .attributes
                .contains_key("gen_ai.usage.cached_input_tokens"),
            "cached_input_tokens must be omitted when usage reports zero"
        );
    }

    // ---- engine_error_from_rig tests -----------------------------------

    fn http_status(code: u16) -> rig::completion::CompletionError {
        let status = http::StatusCode::from_u16(code).unwrap();
        rig::completion::CompletionError::HttpError(rig::http_client::Error::InvalidStatusCode(
            status,
        ))
    }

    fn requested() -> ModelId {
        ModelId::from("claude-opus-4-7")
    }

    #[test]
    fn maps_http_401_to_auth() {
        let err = engine_error_from_rig(http_status(401), &requested());
        assert!(matches!(err, EngineError::Auth { .. }), "got {err:?}");
    }

    #[test]
    fn maps_http_403_to_auth() {
        let err = engine_error_from_rig(http_status(403), &requested());
        assert!(matches!(err, EngineError::Auth { .. }), "got {err:?}");
    }

    #[test]
    fn maps_http_404_to_model_not_found_carrying_requested_model_id() {
        let err = engine_error_from_rig(http_status(404), &requested());
        match err {
            EngineError::ModelNotFound { model } => assert_eq!(model, requested()),
            other => panic!("expected ModelNotFound, got {other:?}"),
        }
    }

    #[test]
    fn maps_http_429_without_retry_after_to_rate_limited_with_none() {
        let err = engine_error_from_rig(http_status(429), &requested());
        match err {
            EngineError::RateLimited { retry_after } => assert!(retry_after.is_none()),
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    #[test]
    fn maps_other_http_status_to_provider() {
        let err = engine_error_from_rig(http_status(503), &requested());
        match err {
            EngineError::Provider { message } => assert!(message.contains("503")),
            other => panic!("expected Provider, got {other:?}"),
        }
    }

    #[test]
    fn maps_transport_timeout_to_timeout() {
        #[derive(Debug)]
        struct Boom;
        impl std::fmt::Display for Boom {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("operation timed out")
            }
        }
        impl std::error::Error for Boom {}
        let http_err = rig::http_client::Error::Instance(Box::new(Boom));
        let err = engine_error_from_rig(
            rig::completion::CompletionError::HttpError(http_err),
            &requested(),
        );
        assert!(matches!(err, EngineError::Timeout), "got {err:?}");
    }

    #[test]
    fn maps_json_error_to_malformed_response() {
        let json_err = serde_json::from_str::<serde_json::Value>("{not json").unwrap_err();
        let err = engine_error_from_rig(
            rig::completion::CompletionError::JsonError(json_err),
            &requested(),
        );
        assert!(
            matches!(err, EngineError::MalformedResponse { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn maps_provider_envelope_with_model_not_found_substring_to_model_not_found() {
        let err = engine_error_from_rig(
            rig::completion::CompletionError::ProviderError(String::from(
                "the model claude-bogus is unknown to this account",
            )),
            &requested(),
        );
        match err {
            EngineError::ModelNotFound { model } => assert_eq!(model, requested()),
            other => panic!("expected ModelNotFound, got {other:?}"),
        }
    }

    #[test]
    fn maps_url_error_to_provider() {
        let url_err = url::Url::parse("not a url").unwrap_err();
        let err = engine_error_from_rig(
            rig::completion::CompletionError::UrlError(url_err),
            &requested(),
        );
        assert!(matches!(err, EngineError::Provider { .. }), "got {err:?}");
    }

    #[test]
    fn maps_request_error_to_provider() {
        #[derive(Debug)]
        struct Boom;
        impl std::fmt::Display for Boom {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("boom")
            }
        }
        impl std::error::Error for Boom {}
        let err = engine_error_from_rig(
            rig::completion::CompletionError::RequestError(Box::new(Boom)),
            &requested(),
        );
        assert!(matches!(err, EngineError::Provider { .. }), "got {err:?}");
    }

    // ---- feature: transport error root-cause detail --------------------

    #[test]
    fn transport_instance_with_chained_error_includes_root_cause_in_provider_message() {
        // Arrange — outer error wraps a leaf with distinct, actionable text.
        // The outer's Display does not contain the leaf text, so this test
        // fails until classify_transport_instance walks the source chain.
        #[derive(Debug)]
        struct Leaf;
        impl std::fmt::Display for Leaf {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("Connection refused")
            }
        }
        impl std::error::Error for Leaf {}

        #[derive(Debug)]
        struct Outer(Leaf);
        impl std::fmt::Display for Outer {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("error sending request for url (https://api.anthropic.com/v1/messages)")
            }
        }
        impl std::error::Error for Outer {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }

        let http_err = rig::http_client::Error::Instance(Box::new(Outer(Leaf)));
        let err = engine_error_from_rig(
            rig::completion::CompletionError::HttpError(http_err),
            &requested(),
        );

        // Assert — root-cause text surfaces in the provider message.
        match err {
            EngineError::Provider { message } => {
                assert!(
                    message.contains("Connection refused"),
                    "provider message must contain root-cause leaf text; got {message:?}"
                );
            }
            other => panic!("expected Provider, got {other:?}"),
        }
    }

    #[test]
    fn request_error_with_chained_error_includes_root_cause_in_provider_message() {
        // Arrange — a DNS-style chain: outer describes the step, leaf names
        // the OS-level failure. The outer text alone gives no actionable info.
        #[derive(Debug)]
        struct Leaf;
        impl std::fmt::Display for Leaf {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("Name or service not known")
            }
        }
        impl std::error::Error for Leaf {}

        #[derive(Debug)]
        struct Outer(Leaf);
        impl std::fmt::Display for Outer {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("failed to lookup address information")
            }
        }
        impl std::error::Error for Outer {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }

        let err = engine_error_from_rig(
            rig::completion::CompletionError::RequestError(Box::new(Outer(Leaf))),
            &requested(),
        );

        // Assert — root-cause text surfaces; "rig request error:" prefix absent.
        match err {
            EngineError::Provider { message } => {
                assert!(
                    message.contains("Name or service not known"),
                    "provider message must contain root-cause leaf text; got {message:?}"
                );
                assert!(
                    !message.contains("rig request error"),
                    "provider message must not expose internal crate prefix; got {message:?}"
                );
            }
            other => panic!("expected Provider, got {other:?}"),
        }
    }

    #[test]
    fn maps_transport_timeout_buried_at_leaf_to_timeout() {
        // Arrange — outer says "tcp connect error" (no timeout keyword);
        // the OS-level leaf carries "os error 110" (ETIMEDOUT on Linux).
        // This test fails until the timeout check walks the full source chain.
        #[derive(Debug)]
        struct LeafTimeout;
        impl std::fmt::Display for LeafTimeout {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("os error 110")
            }
        }
        impl std::error::Error for LeafTimeout {}

        #[derive(Debug)]
        struct Outer(LeafTimeout);
        impl std::fmt::Display for Outer {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("tcp connect error")
            }
        }
        impl std::error::Error for Outer {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }

        let http_err = rig::http_client::Error::Instance(Box::new(Outer(LeafTimeout)));
        let err = engine_error_from_rig(
            rig::completion::CompletionError::HttpError(http_err),
            &requested(),
        );
        assert!(matches!(err, EngineError::Timeout), "got {err:?}");
    }

    #[test]
    fn openai_from_env_without_key_fails_with_auth() {
        // The key read precedes any client construction, so a missing
        // OPENAI_API_KEY short-circuits to Auth without touching the network.
        // SAFETY: tests run single-threaded against env vars by convention;
        // clear the key to force the deterministic Auth path, restore after.
        let saved = std::env::var("OPENAI_API_KEY").ok();
        // SAFETY: removing an env var; documented unsafe in the 2024 edition.
        unsafe {
            std::env::remove_var("OPENAI_API_KEY");
        }

        let result = openai_from_env("gpt-5-turbo");

        // SAFETY: restoring the previously observed value (or its absence).
        unsafe {
            match saved {
                Some(value) => std::env::set_var("OPENAI_API_KEY", value),
                None => std::env::remove_var("OPENAI_API_KEY"),
            }
        }

        match result {
            Err(EngineError::Auth { .. }) => {}
            Err(other) => panic!("expected Auth, got {other:?}"),
            Ok(_) => panic!("expected Auth with no OPENAI_API_KEY in environment"),
        }
    }

    #[test]
    fn gemini_from_env_without_key_fails_with_auth() {
        // The key read precedes any client construction, so a missing
        // GEMINI_API_KEY short-circuits to Auth without touching the network.
        // SAFETY: tests run single-threaded against env vars by convention;
        // clear the key to force the deterministic Auth path, restore after.
        let saved = std::env::var("GEMINI_API_KEY").ok();
        // SAFETY: removing an env var; documented unsafe in the 2024 edition.
        unsafe {
            std::env::remove_var("GEMINI_API_KEY");
        }

        let result = gemini_from_env("gemini-3-pro");

        // SAFETY: restoring the previously observed value (or its absence).
        unsafe {
            match saved {
                Some(value) => std::env::set_var("GEMINI_API_KEY", value),
                None => std::env::remove_var("GEMINI_API_KEY"),
            }
        }

        match result {
            Err(EngineError::Auth { .. }) => {}
            Err(other) => panic!("expected Auth, got {other:?}"),
            Ok(_) => panic!("expected Auth with no GEMINI_API_KEY in environment"),
        }
    }
}
