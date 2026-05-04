# Dependencies: Rig `Agent`, `AgentBuilder`, and the Anthropic provider

## Findings

Pinned at `rig-core = "0.36.0"` ([Cargo.toml:18](../../../Cargo.toml#L18)). Optional `rig-bedrock = "0.4.5"` is also declared ([Cargo.toml:17](../../../Cargo.toml#L17)).

### `Agent` struct

Defined at `src/agent/completion.rs:166-200` in the local cargo cache (`~/.cargo/registry/src/index.crates.io-*/rig-core-0.36.0/`):

```rust
pub struct Agent<M, P = ()>
where M: CompletionModel, P: PromptHook<M>
{
    pub name: Option<String>,
    pub description: Option<String>,
    pub model: Arc<M>,
    pub preamble: Option<String>,
    pub static_context: Vec<Document>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u64>,
    pub additional_params: Option<serde_json::Value>,
    pub tool_server_handle: ToolServerHandle,
    pub dynamic_context: Arc<Vec<(usize, Arc<...>)>>,
    pub tool_choice: Option<ToolChoice>,
    pub default_max_turns: Option<usize>,
    pub hook: Option<P>,
    pub output_schema: Option<schemars::Schema>,
}
```

`preamble` is a flat `Option<String>`. There is no per-block structure, no array, no cache-control hook.

### `AgentBuilder`

Setters at `src/agent/builder.rs:128-213`:

- `preamble(impl Into<String>)` — sets the system string.
- `context(Document)` — appends a static document.
- `dynamic_context(usize, vector_store)` — registers a RAG store with a sample count.
- `tool(...)`, `tools(...)` — transition the typestate to `WithBuilderTools`.
- `dynamic_tools(...)` — RAG-indexed tools.

### Preamble flow into `CompletionRequest`

`build_completion_request()` at `src/agent/completion.rs:28-141` prepends the preamble as a `Message::system(...)` to the chat history before constructing the request:

```rust
let chat_history: Vec<Message> = if let Some(preamble) = preamble {
    std::iter::once(Message::system(preamble.to_owned()))
        .chain(chat_history.iter().cloned()).collect()
} else { chat_history.to_vec() };
```

The `CompletionRequest` itself also stores `pub preamble: Option<String>` (`src/completion/request.rs:505`).

### Anthropic provider mapping

This is the load-bearing detail. At `src/providers/anthropic/completion.rs:1291-1327`:

```rust
let mut system = if let Some(preamble) = req.preamble {
    if preamble.is_empty() { vec![] }
    else {
        vec![SystemContent::Text { text: preamble, cache_control: None }]
    }
} else { vec![] };

system.extend(history_system);
```

Two inputs converge into the Anthropic `system:` array:

1. `req.preamble`, set via `AgentBuilder.preamble()`.
2. `history_system`, extracted from inline `Message::System` entries by `split_system_messages_from_history`.

Both paths produce `SystemContent::Text { text, cache_control: None }`. Inline `Message::System` entries do not ride along in `messages:`. They are lifted out and appended to `system:`.

### Implications

- The Ailly engine reaches the Anthropic `system:` array today, even with `with_preamble()` unwired. The earlier conclusion that system content lands "as `Message::System` entries in the request body" was incorrect.
- `cache_control` is hard-coded to `None` in both paths in 0.36. Prefix caching on skill descriptions is unreachable through `AgentBuilder`. Reaching it requires either patching Rig, building a `CompletionRequest` directly, or post-processing the request before send.
- Multiple cacheable system blocks, the shape Anthropic's API accepts for skill bundles, cannot be expressed through `Option<String>`. Multiple inline `Message::System` entries will be merged with `cache_control: None` on each.

## Sources

- [Cargo.toml:17-18](../../../Cargo.toml#L17-L18) — Rig version pins
- `~/.cargo/registry/src/index.crates.io-*/rig-core-0.36.0/src/agent/completion.rs:166-200` — `Agent` struct
- `~/.cargo/registry/src/index.crates.io-*/rig-core-0.36.0/src/agent/builder.rs:128-213` — `AgentBuilder` setters
- `~/.cargo/registry/src/index.crates.io-*/rig-core-0.36.0/src/agent/completion.rs:28-141` — preamble prepended as `Message::system()`
- `~/.cargo/registry/src/index.crates.io-*/rig-core-0.36.0/src/completion/request.rs:505` — `CompletionRequest.preamble`
- `~/.cargo/registry/src/index.crates.io-*/rig-core-0.36.0/src/providers/anthropic/completion.rs:1291-1327` — preamble + history system merged into Anthropic `system:` array
- [docs.rs/rig-core/0.36.0](https://docs.rs/rig-core/0.36.0/rig/agent/struct.Agent.html) — public API reference
