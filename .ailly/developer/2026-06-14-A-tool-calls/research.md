# Tool Call Handling

## Topic and Intent

Add end-to-end tool call support to Ailly. This is a **project** (multiple sequential and parallel features) rather than a single slice. The overall shape:

- **Harness** (sequential first): `ToolDefinition`, `ToolExecutor` trait, `Conversation::run` multi-turn loop, noop scripting. These must exist before any individual tool is useful.
- **Individual tools** in parallel, each group a feature:
  - Web: `WebSearch` and `WebFetch` call a search engine provider (tbd) and web site fetcher.
  - File: `File:Glob` to list directories (`./path/*` to list all files, `./path/**/*` to recurse, `./path/*.txt` or `./path/*{foo|bar}.{ts|tsx}`, research an appropriate globbing library). `File:Read` (with optional line ranges) to read a file. `File:Edit` (with line ranges, including full file to replace or write).
  - Scripts: `Script:Bash` runs a shell script using a bash on the path; same for Python and Node. If such a runner isn't available, don't advertise the tool. The tool advertisement needs to include the version of the scripting engine it has, and needs to provide ways to specify the cwd and env. (Lots to research there.)
  - Clarify: `Clarify` takes an LLM question and attempts to research an answer in a subagent. If the results are inconclusive or contradictory, it then asks the user to resolve the question.
  - Subagent: same as other agentic framework subagent skills.
- **e2e and testing** (sequential last): insurance-claim multi-turn run; `e2e/research/` demonstrating the research skill calling web_search then web_fetch.

Each individual tool has its own unit tests, separate from the conversation harness tests. The conversation harness tests use `NoopToolExecutor`; the tool unit tests exercise each tool's logic directly.

Tools belong in the **`knowledge`** module (`src/knowledge/`), alongside `assertions.rs` and `eval.rs`. They operate on conversations (inspecting content, appending results) and are not part of the content schema or the engine adapter.

## Search / Expand

**LLM tool-calling pattern (Anthropic docs).** The canonical shape per the Anthropic API [1]:

1. Send a request with `tools` array and the user message.
2. Claude responds with `stop_reason: "tool_use"` and one or more `tool_use` blocks.
3. Execute each tool; format outputs as `tool_result` blocks.
4. Send a new request with the original messages + assistant response + a user message with `tool_result` blocks.
5. Repeat while `stop_reason == "tool_use"`. Exit on `"end_turn"`, `"max_tokens"`, `"stop_sequence"`, or `"refusal"`.

The `tools` parameter is **per API request**, not per conversation. The contract is: the caller provides the tool list on each individual completion call. This means tool offerings can differ across turns within a conversation — Claude only sees what is passed in the current request's `tools` array.

**Rig's tool API.** Rig exposes an `AgentBuilder` with `.tools(...)` and a `.multi_turn(...)` path. The engine deferred note [3] names this as the reference when tools are wired. However, using `AgentBuilder` internalises the loop inside Rig, breaking Ailly's inline-trace guarantee (trace must be written to the conversation file after each assistant turn) and the mid-run save-on-progress guarantee. The correct path: keep `model.completion()` unary, pass `tools` on each call, detect tool calls in the response, execute them in the knowledge layer, and continue. This mirrors the Anthropic agentic loop shape exactly.

**Scripted tool results for tests.** Standard pattern in major SDKs (Anthropic test helpers, LangChain `FakeTool`, AutoGen `FunctionTool`): a map from tool name to a deterministic response string or closure, consumed in call order or keyed by `(tool_name, call_index)`.

**Research e2e reference.** The `domain-driven-design/research/e2e` project [4] is the structural model. It has `assemblies/`, `evals/`, `prompts/`, `context/`, and `ci.sh`. The Ailly `e2e/research/` project follows the same layout: one assembly that declares `web_search` and `web_fetch` tools; a prompt that asks a research question requiring both; evals asserting `must_call_tool: web_search` and `must_call_tool: web_fetch` (plus a tool_call_order assertion); a `ci.sh` that runs assemble → run (noop-scripted) → eval → report. The research e2e does NOT need to call a live web API; the point is that the harness routes tool calls to the noop executor and the eval assertions fire on the resulting conversation.

## Falsification / Refine

**Is this a bug, a feature, or a project?** Project. The schema, assertions, and e2e fixtures anticipate tool calls — the wiring is missing. Each tool group is a substantial amount of work. These are individual features, this groups them as a project.

**Can an off-the-shelf tool do it?** No. The inline-trace guarantee, noop scripting, and conversation-as-artifact replay are Ailly-specific.

**Smallest version of each component:**

| Component | Minimum |
| --- | --- |
| `ToolDefinition` | name + description + input_schema (JSON Value). Mirrors the JSON in `e2e/insurance-claim/context/tools/*.json`. |
| `CompletionRequest.tools` | `Vec<ToolDefinition>` field; empty by default. |
| `PrefixBlock::Tools` resolver | Resolve JSON files to `Vec<ToolDefinition>` (not text). One block kind stops being text. |
| `rig_engine.rs` | Forward `tools` to Rig (replace `tools: Vec::new()`). |
| `ToolExecutor` trait | `async fn execute(&self, call: &ToolCall) -> ToolResult`. One method. |
| `Conversation::run` loop | Detect tool_use in response → execute all calls → append `Role::Tool` message → continue. |
| `NoopToolExecutor` | Keyed by tool name → scripted `Vec<String>` replies consumed in call order. Lives in `src/knowledge/`. |
| Per-tool unit tests | One unit test file per tool (web_search, web_fetch). Tests the tool logic directly, not the harness. |
| Integration test | Noop-scripted: user → assistant(tool_use) → tool(tool_result) → assistant(text). Verifies session shape and that eval assertions fire. |
| `e2e/insurance-claim` | Existing assertions verified against actual multi-turn noop run. `ci.sh` uses a noop script fixture for the structural gate; live run gated on `ANTHROPIC_API_KEY`. |
| `e2e/research/` | Assembly with `web_search` + `web_fetch` tools, one research prompt, evals asserting tool calls and order. Noop run; no live web API. |

**What is deferred.** Rig `AgentBuilder` / `multi_turn` (existing deferred note). Streaming tool responses. Tool-call error recovery or retry. Tool result cache markers. Per-turn tool variation. `NoopEngine::from_table` keyed-by-binding (existing deferred note).

## Scope

**In:**

- `ToolDefinition` value type in `src/knowledge/` (or `src/content/` — see open question).
- `CompletionRequest.tools: Vec<ToolDefinition>`.
- `PrefixBlock::Tools` resolved as structured `Vec<ToolDefinition>`, not concatenated text.
- `rig_engine.rs` forwards `tools` to Rig completion request.
- `ToolExecutor` trait in `src/knowledge/`.
- `NoopToolExecutor` in `src/knowledge/`.
- `Conversation::run` extended to drive the tool loop (second parameter: `&dyn ToolExecutor`).
- `Web:Search` and `Web:Fetch` tool implementations in `src/knowledge/tools/web.rs`.
- `File:Read`, `File:Edit`, `File:Glob`, `Script:Bash/Node/Python`, `General:Clarify`, `Todo:*` tools in `src/knowledge/tools/{file,script,general,todo}`.
- Unit tests for `web_search` and `web_fetch`, separate from harness tests.
- Integration test for the multi-turn noop run.
- `e2e/insurance-claim` structural gate (noop fixture for CI; live gate gated on key).
- `e2e/research/` project.
- DESIGN.md update: `PrefixBlock.kind: tools` produces structured definitions, not text; agentic loop shape documented.

**Out:**

- Rig `AgentBuilder` / `multi_turn` path.
- Streaming tool responses.
- Tool-call error recovery or `is_error: true` retry loop.
- Per-turn tool variation (tools are static per conversation in v1).
- `Message.cache` forwarding for tool turns.
- `NoopEngine::from_table` keyed-by-binding.

## Resolved Decisions

| Question | Resolution |
| --- | --- |
| Project or feature? | Project. Harness is sequential (must precede tools); tools are parallel features; e2e and testing are sequential after tools. |
| Do we use Rig's multi_turn path? | No. Keep `model.completion()` unary; loop in `Conversation::run`. Preserves inline-trace and mid-run save guarantees. |
| Where does the tool executor live? | `ToolExecutor` trait injected into `Conversation::run` as a second parameter. Lives in `src/knowledge/`. |
| Does `PrefixBlock::Tools` stop being text? | Yes. Only this one block kind becomes structured data on `CompletionRequest`. |
| Can tools change between turns in a conversation? | Yes, per the Anthropic API (tools is per-request). Ailly v1 passes the same tool list on every call (from the assembly). Per-turn variation is deferred. |
| Research e2e scope? | Two tools only (web_search, web_fetch). Noop run — no live API. Follows the `domain-driven-design/research/e2e` layout. |
| Do individual tools get individual tests? | Yes. Each tool (`web_search`, `web_fetch`) has its own unit test file exercising the tool logic directly. Harness tests use `NoopToolExecutor`. |

**Open for design:**

1. Does `ToolDefinition` live in `src/content/` (part of the assembly/conversation schema) or `src/knowledge/` (where it's used at execution time)? The assembly references it via `PrefixBlock::Tools`; the run loop uses it at execution time.
2. Does `Conversation::run` take `executor: Option<&dyn ToolExecutor>` (returning an error if tool_use appears with no executor) or always require an executor (callers pass `NoopToolExecutor::empty()` when no tools are needed)?
3. For `e2e/insurance-claim`: is the noop fixture a standalone YAML conversation file in `e2e/insurance-claim/runs/fixture/`, or is it generated by the CI script from the assembly with scripted replies injected?

## Sources

[1] Anthropic, "How tool use works," Claude API documentation, 2026. [Online]. Available: [platform.claude.com/docs/en/agents-and-tools/tool-use/how-tool-use-works](https://platform.claude.com/docs/en/agents-and-tools/tool-use/how-tool-use-works)

[2] Anthropic, "Define tools," Claude API documentation, 2026. [Online]. Available: [platform.claude.com/docs/en/agents-and-tools/tool-use/define-tools](https://platform.claude.com/docs/en/agents-and-tools/tool-use/define-tools)

[3] Ailly `TASK-NOTES-engine-deferred.md`, "Tool-definition wiring on requests," deferred note, 2026. Local: `.ailly/developer/TASK-NOTES-engine-deferred.md`.

[4] `domain-driven-design/research/e2e`, research eval harness layout reference. Local: `~/devel/davidsouther/domain-driven-design/research/e2e/`.

[5] Ailly `DESIGN.md`, assembly `PrefixBlock` schema, 2026. Local: `DESIGN.md:51–64`.
