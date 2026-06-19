# Feature 1 Feature Test: Tool-call harness loop

**Project:** [../design.md](../design.md) | **Feature design:** [design.md](design.md) | **Plan:** [../plan.md](../plan.md)
**Type:** Feature (Feature 1 of 3) | **Status:** Red (failing as intended; type-first stubs keep `check` green)

## User Story

**Given** a conversation whose blank assistant slot is about to be filled, and
a model (noop-scripted) whose first reply is an assistant `tool_use` block and
whose second reply is assistant text, and a tool executor (noop-scripted) that
returns the matching `tool_result`,
**When** the operator drives `Conversation::run` with both the engine and the
executor,
**Then** the resulting session has the agentic shape
`user → assistant(tool_use) → tool(tool_result) → assistant(text)` — the
assistant's `tool_use` was dispatched through the executor, its `tool_result`
was appended as a `Role::Tool` message, and a fresh blank assistant slot was
filled with the model's follow-up text.

## Test

- **File:** `tests/tool_loop.rs` — `run_drives_tool_use_then_tool_result_then_text`.
- Drives `Conversation::run(&engine, &executor)` directly so both sides are
  deterministic and hermetic: `NoopEngine::from_scripts` scripts the
  `tool_use`-then-text replies; `NoopToolExecutor::from_scripts` scripts the
  `tool_result` for the named tool. No live model, no live tool, no network.
- Asserts each resulting message's role and block kind: `session[1]` is an
  assistant `ToolUse` echoing the call id, `session[2]` is a `Role::Tool`
  `ToolResult` echoing that id, `session[3]` is the assistant follow-up text,
  and no blank slot remains.

## Resolved deferred decisions (per [design.md](design.md), authorized auto-clear)

- **Open #1** — `ToolDefinition` lands in `src/content/conversation.rs` (schema
  value, serialized into `Meta.tools`; `content`/`engine` must not depend up
  into `knowledge`). The feature test does not reference `ToolDefinition`
  directly (it scripts the loop through `NoopEngine`/`NoopToolExecutor`); the
  placement is exercised by later schema steps.
- **Open #2** — `Conversation::run(&mut self, engine, executor: &dyn ToolExecutor)`
  with the executor **always required**; no-tools callers pass
  `NoopToolExecutor::default()`. The test passes a scripted executor; the CLI
  caller (`src/cli/run.rs`) passes the empty default.

## Type-first stubs (this phase only — no real logic)

To keep `mise run check` green while the feature test fails at runtime:

- `src/knowledge/tools/mod.rs` (new; registered via `pub mod tools;` in
  `src/knowledge/mod.rs`): `ToolError`, the `ToolExecutor` trait, and
  `NoopToolExecutor` (`new` / `from_scripts` / `Default`). The trait impl body
  is `todo!()`.
- `src/content/conversation.rs`: `Conversation::run` gains the
  `executor: &dyn ToolExecutor` parameter (open #2); `RunError` gains the
  `Tool(#[from] ToolError)` variant. The loop body is unchanged — it fills
  blanks but does not yet dispatch the tool turn, so the conversation stops at
  `user → assistant(tool_use)` and the session-shape assertion fails. That is
  the intended **red**.
- `src/cli/run.rs`: the existing `conv.run` caller passes
  `NoopToolExecutor::default()` so the crate keeps compiling.

## Verification (actual command runs)

- `mise run check` — **green** (crate compiles, including the stubs).
- `mise run test` — the feature test
  `tool_loop::run_drives_tool_use_then_tool_result_then_text` is **red** (the
  session has only `user → assistant`, so the role-vector assertion fails);
  every other test is green.

## Follow-on TASK

After Feature 1's seven build steps are green, run `developer:refactor` /
`developer:cleanup` to settle the new `knowledge/tools/` surface and the
changed assembly render path before Features 2 and 3 build on it (per the
project plan's Feature 1 cleanup checkpoint).
