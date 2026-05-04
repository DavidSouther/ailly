# Tools — CLI registration surface

The `2026-05-03-B-tool-calls` design wired tool calls through the engine. The `2026-05-03-C-conversation-tools` design resolved declarations into an `Arc<dyn ToolRegistry>` on `Generator`. The CLI today constructs `Generator` with no `with_registry` call, so the default `EmptyRegistry` is in place. Any `.ailly.toml` declaring `tools = [...]` therefore fails with `strict_tools = true` on the first unknown name, per `docs/developer/2026-05-03-C-conversation-tools/design.md` "CLI behavior".

Three pieces ship together.

- **CLI tool registration**: a way for the CLI binary to register concrete `Arc<dyn rig::tool::ToolDyn>` implementations into a `HashMapRegistry` before the `Generator` runs. Shape is open. Candidates include a CLI flag, a `[tools]` table in a top-level config file, a Rust feature flag per tool, and a Skill loader that reads `allowed-tools` frontmatter. The first concrete tools (project file readers, shell) likely ship alongside this surface.
- **`--lenient-tools` flag**: expose `Settings::strict_tools` through a CLI flag. Default stays `true`. The defer was scoped to "until a real built-in tool ships", so this lands together with the registration surface. See `docs/developer/2026-05-03-C-conversation-tools/design.md` "Deferred decisions".
- **Synthetic tool emission in `Noop`**: `e2e/20_tools/tools.sh` today asserts schema round-trip via `--clean` and strict-mode failure. A second phase needs real tool execution end-to-end. The `Noop` engine accepts the tools slice but never emits `EngineEvent::ToolCall`, per `docs/developer/2026-05-03-B-tool-calls/design.md` "Noop implementation". A synthetic-emission mode on `Noop` (for example a `Noop::with_tool_script(..)` builder that emits a fixed `ToolCall, ToolResult, Final` sequence) lets the e2e harness exercise the loop without a network call.

Touch points: the CLI binary entry point for the registration plumbing, `src/engine/noop.rs` for synthetic emission, `e2e/20_tools/tools.sh` for the new phase, and the CLI argument parser for `--lenient-tools`.
