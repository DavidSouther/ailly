# rig-core 0.36 Public API Survey

Scope: design notes for an Engine module in `ailly_rust` on top of `rig-core = "0.36.0"`. Citations reference docs.rs for the exact 0.36 build.

## 1. Message model

`rig::completion::message::Message` is a three-variant enum (re-exported as `rig::message::Message`). 0.36 declares:

```rust
pub enum Message { System { content: ... }, User { content: ... }, Assistant { id: ..., content: ... } }
```

Source: `id="variant.System"`, `id="variant.User"`, `id="variant.Assistant"` on the Message page. Constructor methods exist: `system`, `user`, `assistant`, `assistant_with_id`, `tool_result`, `tool_result_with_call_id` (method ids visible on the same page). So existing `Message::system(...)` calls are valid.

Multi-turn tool calling lives on the content enums, not on Message variants directly:

- `pub enum UserContent { Text, ToolResult, Image, Audio, Video, Document }` carries the `ToolResult` reply variant.
- `pub enum AssistantContent { Text, ToolCall, Reasoning, Image }` carries the `ToolCall` request variant.

Both enums are wrapped in `OneOrMany<...>` inside the `User` and `Assistant` variants of `Message`.

## 2. Completion / chat traits

The trait an engine provider implements is `rig::completion::CompletionModel` (also re-exported through `rig::completion::request`). 0.36 shape:

```rust
pub trait CompletionModel: Clone + WasmCompatSend + WasmCompatSync {
    type Response: Send + Sync + Serialize + DeserializeOwned;
    type StreamingResponse: Clone + Unpin + Send + Sync + Serialize + DeserializeOwned + GetTokenUsage;
    type Client;
    fn make(client: &Self::Client, model: impl Into<String>) -> Self;
    fn completion(&self, request: CompletionRequest)
        -> impl Future<Output = Result<CompletionResponse<Self::Response>, CompletionError>> + Send;
    fn stream(&self, request: CompletionRequest)
        -> impl Future<Output = Result<StreamingCompletionResponse<Self::StreamingResponse>, CompletionError>> + Send;
    fn completion_request(&self, prompt: impl Into<Message>) -> CompletionRequestBuilder<Self> { ... }
}
```

Streaming is an integral method on `CompletionModel::stream` (no separate `StreamingCompletionModel` trait in 0.36). The high-level user-facing helpers live in `rig::streaming` as `StreamingPrompt`, `StreamingChat`, `StreamingCompletion`, but those are consumer traits (e.g. `Agent` impls them); a provider only needs `CompletionModel`.

`CompletionRequest` is the input record: `model`, `preamble`, `chat_history: OneOrMany<Message>`, `documents`, `tools: Vec<ToolDefinition>`, `temperature`, `max_tokens`, `tool_choice`, `additional_params`, `output_schema`.

`CompletionResponse<T>` is the non-streaming output: `choice: OneOrMany<AssistantContent>`, `usage: Usage`, `raw_response: T`, `message_id: Option<String>`.

## 3. Built-in providers

`rig::providers` in 0.36 ships modules for: anthropic, azure, chatgpt, cohere, copilot, deepseek, galadriel, gemini, groq, huggingface, hyperbolic, llamafile, minimax, mira, mistral, moonshot, ollama, openai, openrouter, perplexity, together, voyageai, xai, xiaomimimo, zai.

OpenAI yes. Anthropic yes. Mistral yes. Bedrock no — there is no `bedrock` module in `rig::providers`; it lives in the separate crate `rig-bedrock` (latest 0.4.5).

The crate has no per-provider feature flags. Provider gating is via the `reqwest` / `rustls` / `native-tls` / `experimental` features only; provider modules are unconditional.

## 4. Agent type

`rig::agent::Agent` (the constructed object) and `rig::agent::AgentBuilder` (the staged builder). Builder offers: `new(model: M)`, `name`, `description`, `preamble`, `append_preamble`, `without_preamble`, `context`, `dynamic_context`, `tool`, `tools`, `rmcp_tool`, `rmcp_tools`, `dynamic_tools`, `tool_choice`, `default_max_turns`, `temperature`, `max_tokens`, `additional_params`, `output_schema`, `output_schema_raw`, `hook`, `tool_server_handle`, `build`.

Agent itself implements:

- `fn prompt(&self, prompt: impl Into<Message>) -> PromptRequest` (one-shot).
- `async fn chat(&self, prompt, chat_history) -> Result<String, PromptError>` (multi-turn).
- `async fn completion(&self, prompt, chat_history) -> Result<CompletionRequestBuilder, CompletionError>`.
- `fn stream_prompt(...) -> StreamingPromptRequest` and `fn stream_chat(prompt, chat_history) -> StreamingPromptRequest`.

The chat_history parameter is `IntoIterator<Item = T>` where `T: Into<Message>`, so a pre-built `Vec<Message>` works directly. Preamble is on the builder, so the engine layer composes `preamble + history + new user prompt` naturally.

## 5. Streaming output

`fn stream(...)` returns `Result<StreamingCompletionResponse<R>, CompletionError>`. The struct is:

```rust
pub struct StreamingCompletionResponse<R: Clone + Unpin + GetTokenUsage> {
    pub choice: OneOrMany<AssistantContent>,
    pub response: Option<R>,
    pub final_response_yielded: AtomicBool,
    pub message_id: Option<String>,
    /* private fields */
}
```

It is itself a `Stream`. Two relevant item enums sit alongside it in `rig::streaming`:

- `RawStreamingChoice<R>` (the low-level provider yield): `Message(String)`, `ToolCall(RawStreamingToolCall)`, `ToolCallDelta { id, internal_call_id, content: ToolCallDeltaContent }`, `Reasoning { id, content }`, `ReasoningDelta { id, reasoning }`, `FinalResponse(R)`, `MessageId(String)`.
- `StreamedAssistantContent<R>` (the consumer-facing yield): `Text(Text)`, `ToolCall { tool_call, internal_call_id }`, `ToolCallDelta { ... }`, `Reasoning(Reasoning)`, `ReasoningDelta { ... }`, `Final(R)`.

So the contract is: stream yields text deltas, tool-call deltas, reasoning deltas, and a terminal `FinalResponse` / `Final` carrying the typed provider response. `choice` and `message_id` on the response struct are populated by the time the stream finishes (docblock: "message and response are populated at the end of the inner stream"). The "stop reason" is whatever each provider's `R` carries (e.g. OpenAI's `StreamingCompletionResponse`); rig itself does not impose a unified stop-reason enum.

## 6. Tools

`rig::tool::Tool`:

```rust
pub trait Tool: Sized + Send + Sync {
    type Error: Error + Send + Sync + 'static;
    type Args: for<'de> Deserialize<'de> + Send + Sync;
    type Output: Serialize;
    const NAME: &'static str;
    fn definition(&self, _prompt: String) -> impl Future<Output = ToolDefinition> + Send + Sync;
    fn call(&self, args: Self::Args) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send;
}
```

Inputs are parsed for you: `Self::Args` is deserialized from the model's JSON before `call` runs. Registration on `AgentBuilder` is via `.tool(impl Tool + 'static)` or `.tools(Vec<Box<dyn ToolDyn>>)`. There is also `ToolSet` for grouped registration and `dynamic_tools` for vector-store-driven retrieval, plus first-class MCP via `rmcp_tool` / `rmcp_tools`.

## 7. Custom (noop) provider

Implement `CompletionModel` plus the three associated types and three required methods (`make`, `completion`, `stream`). Concretely you must provide:

- `type Response`: any `Send + Sync + Serialize + DeserializeOwned` (a unit-style `()` works; an empty struct is cleaner).
- `type StreamingResponse`: must add `Clone + Unpin + GetTokenUsage`. Easiest is a tiny wrapper that returns `Usage::default()` from `GetTokenUsage::token_usage`.
- `type Client = ()` is fine.
- `make`: ignore inputs, return `Self`.
- `completion`: synthesize a `CompletionResponse` with a chosen `OneOrMany<AssistantContent>` and a `Usage::default()`.
- `stream`: build a `StreamingCompletionResponse` over a `futures::stream::iter([...])` of `RawStreamingChoice` values terminated with `FinalResponse(R)`.

You do not need to implement `Tool`, `Chat`, `StreamingChat`, `Agent`, etc. The trait is dyn-incompatible (uses `Self`, `impl Future`), so wrap behind generics or your own object-safe shim.

## Sources

- Crate features and dep list: `https://crates.io/api/v1/crates/rig-core/0.36.0` (`"features": { "default": ["reqwest", "rustls"], ... }` — no provider flags).
- Module index: `https://docs.rs/rig-core/0.36.0/rig/` ("mod rig::providers", "mod rig::streaming", "mod rig::completion", "mod rig::agent", "mod rig::tool").
- Providers list: `https://docs.rs/rig-core/0.36.0/rig/providers/index.html` (mod links for anthropic, openai, mistral, ... no bedrock).
- Bedrock external: `https://crates.io/api/v1/crates?q=rig-bedrock` (`"name": "rig-bedrock", "description": "AWS Bedrock model provider for Rig integration."`).
- Message enum: `https://docs.rs/rig-core/0.36.0/rig/completion/message/enum.Message.html` (variant ids `System`, `User`, `Assistant`; method ids `system`, `user`, `assistant`, `assistant_with_id`, `tool_result`, `tool_result_with_call_id`).
- UserContent: `https://docs.rs/rig-core/0.36.0/rig/completion/message/enum.UserContent.html` (`pub enum UserContent { Text, ToolResult, Image, Audio, Video, Document }`).
- AssistantContent: `https://docs.rs/rig-core/0.36.0/rig/completion/message/enum.AssistantContent.html` (`pub enum AssistantContent { Text, ToolCall, Reasoning, Image }`).
- CompletionModel trait: `https://docs.rs/rig-core/0.36.0/rig/completion/request/trait.CompletionModel.html` (`type Response`, `type StreamingResponse`, `type Client`, `fn make`, `fn completion`, `fn stream`, `fn completion_request`).
- Completion / Chat: `https://docs.rs/rig-core/0.36.0/rig/completion/request/trait.Completion.html` and `.../trait.Chat.html` (`fn completion(&self, prompt, chat_history) -> ...`, `fn chat(&self, prompt, chat_history) -> Result<String, PromptError>`).
- CompletionRequest: `https://docs.rs/rig-core/0.36.0/rig/completion/request/struct.CompletionRequest.html` (`pub chat_history: OneOrMany<Message>`, `pub tools: Vec<ToolDefinition>`, `pub preamble: Option<String>`).
- CompletionResponse: `https://docs.rs/rig-core/0.36.0/rig/completion/request/struct.CompletionResponse.html` (`pub choice: OneOrMany<AssistantContent>`, `pub usage: Usage`, `pub raw_response: T`, `pub message_id: Option<String>`).
- Streaming module index: `https://docs.rs/rig-core/0.36.0/rig/streaming/index.html` (StreamingCompletionResponse, RawStreamingChoice, StreamedAssistantContent, ToolCallDeltaContent, RawStreamingToolCall, StreamingResult, StreamingPrompt, StreamingChat, StreamingCompletion).
- StreamingCompletion trait: `https://docs.rs/rig-core/0.36.0/rig/streaming/trait.StreamingCompletion.html` (`fn stream_completion(&self, prompt, chat_history) -> impl Future<Output = Result<CompletionRequestBuilder<Self>, CompletionError>>`).
- StreamingCompletionResponse: `https://docs.rs/rig-core/0.36.0/rig/streaming/struct.StreamingCompletionResponse.html` ("message and response are populated at the end of the inner stream").
- RawStreamingChoice: `https://docs.rs/rig-core/0.36.0/rig/streaming/enum.RawStreamingChoice.html` (variants Message, ToolCall, ToolCallDelta, Reasoning, ReasoningDelta, FinalResponse, MessageId).
- StreamedAssistantContent: `https://docs.rs/rig-core/0.36.0/rig/streaming/enum.StreamedAssistantContent.html` (variants Text, ToolCall, ToolCallDelta, Reasoning, ReasoningDelta, Final).
- AgentBuilder: `https://docs.rs/rig-core/0.36.0/rig/agent/struct.AgentBuilder.html` (`pub fn preamble`, `pub fn tool`, `pub fn tools`, `pub fn rmcp_tool`, `pub fn build(self) -> Agent<...>`).
- Agent surface: `https://docs.rs/rig-core/0.36.0/rig/agent/struct.Agent.html` (`fn prompt`, `async fn chat`, `async fn completion`, `fn stream_prompt`, `fn stream_chat`).
- Tool trait: `https://docs.rs/rig-core/0.36.0/rig/tool/trait.Tool.html` (`type Args: for<'de> Deserialize<'de>`, `const NAME: &'static str`, `fn call(&self, args: Self::Args) -> ...`).
