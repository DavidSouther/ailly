# Codebase: how Ailly assembles system content for Anthropic completions

## Findings

The `feat_knowledge_skills` branch builds a system chain from `.ailly.toml` files but does not yet route it through Rig's preamble setter. The wiring is staged, not finished.

### System chain assembly

`AillyRc::load()` reads the `system = "..."` string from each `.ailly.toml` along the directory hierarchy and accumulates `Message::system(...)` entries into a `Vec<Message>` ([src/content/mod.rs:631-658](../../../src/content/mod.rs#L631-L658)). The chain is governed by parent modes (`Root`, `Always`, `Never`), so deeper directories may extend, replace, or suppress ancestor system content. The accumulated chain is stored on `ConversationTurn` ([src/content/mod.rs:145](../../../src/content/mod.rs#L145)).

### Per-turn history

`Conversation::history_for(turn)` emits the message array in this order ([src/content/mod.rs:417-446](../../../src/content/mod.rs#L417-L446)):

1. The inherited system chain, unless `meta.skip_head` is set.
2. Predecessor turn prompts and assistant responses.
3. The current turn's prompt.

System messages travel inline in the `Vec<Message>`. They are not lifted into a separate field at this layer.

### Engine boundary

`RigEngine::stream()` accepts the history and passes it to the Rig agent ([src/engine/rig_engine.rs:56-107](../../../src/engine/rig_engine.rs#L56-L107)). The last user message is split off via `extract_last_user_text()` ([src/engine/rig_engine.rs:111-122](../../../src/engine/rig_engine.rs#L111-L122)) and the remainder is handed to `agent.stream_chat(last_user_text, prior)`.

`RigEngine` carries a `preamble: Option<String>` field with a `with_preamble()` setter ([src/engine/rig_engine.rs:41-44](../../../src/engine/rig_engine.rs#L41-L44)). The setter is never called from `build_engine()` in [src/ailly/mod.rs](../../../src/ailly/mod.rs). The field stays `None`. The system chain reaches Rig only as inline `Message::System` entries in the history.

### Where it lands on the wire

This was the part initially mis-read. See `dependencies.md` in this folder for the resolution: Rig's Anthropic provider extracts inline `Message::System` entries from history and merges them into the Anthropic `system:` array regardless of whether the preamble field was set. The system chain does reach the `system:` parameter today.

The cost of leaving `with_preamble()` unwired is therefore narrower than it first appeared:

- Order. Preamble-set content lands at the head of the `system:` array; history-extracted entries are appended after. With everything inline, the order is whatever `history_for` produces.
- Cache control. Rig 0.36 hard-codes `cache_control: None` on every `SystemContent::Text` in both paths. Per-block cache breakpoints are unreachable through `AgentBuilder`.
- Multi-block layout. A single `Option<String>` cannot represent multiple cacheable system blocks, which the Anthropic API supports.

## Sources

- [1] `src/content/mod.rs` [9db3a02]
- [2] `src/engine/rig_engine.rs` [9db3a02]
- [3] `src/ailly/mod.rs` [9db3a02]
