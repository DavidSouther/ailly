# Tools — MCP-backed registry

The `2026-05-03-B-tool-calls` design rejected MCP-driven dispatch as out of scope for the engine slice. The `2026-05-03-C-conversation-tools` design committed to a `ToolRegistry` trait whose `resolve` is sync, because the in-process `HashMapRegistry` does not need async, and a remote registry can wrap a sync façade around its own cached state.

When MCP comes online:

- Implement an `McpRegistry` (or similarly named) impl that holds one or more `rmcp::Client` connections and resolves names by mapping the registry's name to a (server, tool-name) pair, then returns an `Arc<dyn ToolDyn>` whose `call` forwards to the MCP client. `rig::tool::rmcp` already provides the adapter shape.
- Decide whether MCP servers are declared in `.ailly.toml` (which would couple `content` to MCP) or in a top-level CLI config (which keeps `content` MCP-free). The `tool-calls` design's metadata-leakage invariant favors the CLI-config option.
- Reassess `ToolRegistry::resolve` as async. v1 is sync. A remote MCP lookup on the hot path is the case that flips this. The mitigation today is a sync façade over cached state. An async trait method is the alternative if remote calls dominate.

Touch points: a new `src/engine/mcp_registry.rs` module, an MCP server discovery surface (likely outside `content`), and possibly an `async fn resolve` variant on the `ToolRegistry` trait.
