# Project Design: Tool Calls

**Type:** Project (the five-phase loop scaled up, per `developer/references/project-cycle.md`)
**Status:** Review
**Closing Bell:** [.ailly/developer/2026-06-14-A-tool-calls/closing-bell.md](closing-bell.md)
**Release flag:** None. The per-assembly tool declaration is the gate (see §4, *Release Flag*)
**Research:** [research.md](research.md)

## Project Phases

| Phase | Meaning | Entered when |
|---|---|---|
| **Review** | This doc, the per-feature designs, the plan, and the Closing Bell are drafted and refined. The project-altitude `*Draft*` gate. | now |
| **Implement** | The plan is approved; feature-steps are built. Each tool stays dark until registered, so the mainline is releasable at every step. | plan cleared |
| **Completed** | The Closing Bell study passed. Status becomes `completed: YYYY-MM-DD`. | bell passes |

## 1. Purpose

Ailly's conversation schema, eval assertions, and e2e fixtures already anticipate tool calls. `ContentBlock::ToolUse`/`ToolResult`, `Role::Tool`, the `must_call_tool` and `tool_call_order` assertions, and the `context/tools/*.json` fixtures all exist today. The wiring that makes them live is what is absent. The model is never told which tools it may call, and `Conversation::run` never acts on a `tool_use` block.

This project closes that gap end to end: an assembly can declare tools, `ailly run` can drive a multi-turn tool loop, and `ailly eval` can score the resulting tool calls. The pieces deliver value only together. A harness with no tool is inert; a tool with no harness cannot be reached; and neither is verifiable without the e2e fixtures that exercise the whole loop. That mutual dependency is what makes this a project rather than three unrelated features.

The project is bounded to the **minimal vertical slice** that proves one real tool through the whole `assemble → run → eval → report` loop: the harness, the two web tools, and the e2e/testing that exercises them. File, Script, Clarify, and Subagent tools are a named follow-on project (§6).

## 2. Prior Art

- **Anthropic agentic loop.** The canonical request → `stop_reason: tool_use` → execute → `tool_result` → repeat shape, where `tools` is a *per-request* parameter (research.md [1][2]). Ailly mirrors this with `model.completion()` kept unary.
- **Rig's tool API.** `AgentBuilder.multi_turn` internalises the loop; rejected as the loop owner because it hides intermediate turns and breaks Ailly's inline-trace and mid-run-save guarantees (research.md [3]; §5).
- **Scripted tool-result fakes.** Anthropic test helpers, LangChain `FakeTool`, and AutoGen `FunctionTool` share one shape: a map from tool name to deterministic replies consumed in call order. This is the model for `NoopToolExecutor`.
- **`domain-driven-design/research/e2e`.** The structural model for `e2e/research/` (research.md [4]): `assemblies/`, `evals/`, `prompts/`, `context/`, `ci.sh`.
- **Ailly's own `content` / `knowledge` split.** Assertion *shapes* live in `content/evaluation.rs`; assertion *execution* lives in `knowledge/`. This project follows the same seam: `ToolDefinition` (a schema value) in `content/`, `ToolExecutor` and the tools (behavior) in `knowledge/tools/`.

## 3. User Journey and Metrics

The end-to-end journey, across all three features, in the user's language:

> A competent Ailly user, fluent in `assemble → run → eval → report` but new to tools, declares a `tools` prefix block in an assembly, runs a multi-turn tool conversation, writes assertions over the tool calls, and reads the pass/fail in the report. The `web_search → web_fetch` research e2e runs green from a single `ci.sh`.

The **measure of done is the Closing Bell** ([closing-bell.md](closing-bell.md)), a summative usability study run once near completion. It is not a continuously-running gate; it fixes the definition of done up front. Each in-scope feature also carries its own executable feature test (defined in that feature's own design), which is the continuous regression guard at feature altitude.

## 4. Specification

### Step 0: Shared contract (settle before parallel work)

The harness feature delivers the contract every later feature depends on. Parallel tool features that agree on it integrate; those that do not collide.

```
ToolDefinition            → src/content/      { name, description, input_schema: Value }
                                               serializes into meta.tools; a schema value,
                                               not behavior.

meta.tools                → conversation       new optional field on the conversation meta
  : Vec<ToolDefinition>                         header (DESIGN.md schema change). Default
                                                empty; skip-serialized when empty so existing
                                                tool-free conversations are byte-identical.

ToolExecutor              → src/knowledge/     async dispatch: a tool call → a tool result.
NoopToolExecutor          →   tools/           scripted replies in call order, for tests and
                                                the structural CI gate.

Run-loop tool-turn        → Conversation::run  after a blank assistant slot is filled, if its
protocol                                        content carries tool_use blocks: execute each
                                                via the executor, append one Role::Tool message
                                                with the tool_result block(s), append a fresh
                                                blank assistant slot, and continue the loop.
```

The data flow that makes replay work: `assemble` resolves `PrefixBlock::Tools` (JSON files) into structured `ToolDefinition`s and writes them onto `meta.tools`. This is the one prefix-block kind that stops being concatenated system text. `run` reads `meta.tools` straight off the conversation file and forwards it on every `CompletionRequest`. Because the definitions live in the run artifact, a conversation replays without re-opening its assembly, preserving the conversation-as-artifact guarantee.

### Features

| # | Feature | Relationship | Delivers |
|---|---|---|---|
| 1 | **Harness** | sequential, first | The Step-0 contract: `ToolDefinition`, `meta.tools`, `CompletionRequest.tools`, `rig_engine` forwarding, `ToolExecutor` + `NoopToolExecutor`, the `Conversation::run` tool loop. |
| 2 | **Web tools** | parallel | `web_search`, `web_fetch` in `knowledge/tools/web.rs`, each with its own unit test exercising the tool logic directly. Depends on: Harness (Step-0 contract). |
| 3 | **e2e + testing** | sequential, last | `e2e/research/` (assembly declaring `web_search` + `web_fetch`, a research prompt, evals asserting the tool calls + order, noop-run `ci.sh`); the `insurance-claim` multi-turn structural gate. Depends on: Harness, Web tools. |

Each feature is its own design → plan → build → cleanup cycle. Fine-grained interface decisions deliberately deferred to the owning feature's design, not settled here: whether `Conversation::run` takes `executor: Option<&dyn ToolExecutor>` or always-required with an empty default (research.md open #2), and whether the insurance-claim noop fixture is a standalone YAML file or generated by `ci.sh` (research.md open #3).

### DESIGN.md change

The conversation `meta` schema gains `tools?: ToolDefinition[]`, and the prose that says every prefix block is "ordered text concatenated into the window" is amended: `kind: tools` resolves to structured `ToolDefinition`s carried on `meta.tools`, not text. The agentic-loop shape (assistant `tool_use` → `Role::Tool` `tool_result` → next turn) is documented alongside the existing "blank assistant slot" description.

### Release Flag

The methodology defaults to one project-level release flag, whose job is to decouple deploy from release for a user-facing surface. This project has no such surface to gate, so it ships without a flag. Ailly is a library and CLI, and tool support has no always-on exposure. Tools are inert until an assembly *declares* a `tools` block **and** `run` is handed an executor. An assembly that names an unregistered tool fails loud rather than doing nothing silently. Each tool therefore stays dark until its feature registers it, and the mainline stays releasable at every step without a runtime flag.

This is a deliberate departure from the default. It is justified by the absence of a release surface, not by a claim that the per-assembly opt-in substitutes for one. Revisit only if a tool ever becomes active without an explicit per-assembly declaration.

## 5. Alternatives

| Approach | Tool defs flow | Loop owner | Inline-trace + mid-run save | Replay from file alone | Verdict |
|---|---|---|---|---|---|
| **A: `meta.tools` + injected `ToolExecutor` + unary loop** | `assemble` → `meta.tools`; `run` forwards per request | `Conversation::run`, `model.completion()` stays unary | preserved | preserved | **chosen** |
| B: Rig `AgentBuilder.multi_turn` | Rig-internal registry | Rig | **broken** (intermediate turns hidden) | n/a | rejected |
| C: tools-as-text; `run` re-reads the assembly | stay as `Role::System` text | `Conversation::run` | preserved | **broken** (`run` needs the assembly) | rejected |

**Build vs off-the-shelf.** No off-the-shelf agent loop preserves all three Ailly guarantees at once: inline per-turn trace, deterministic noop scripting, and conversation-as-artifact replay. Rig (already a dependency) is kept for the single completion call but not for the loop. (research.md *Falsification*.)

## 6. Summary

**Follow-on project, additional tools.** File (`File:Read`/`Edit`/`Glob`), Script (`Script:Bash`/`Node`/`Python`), `General:Clarify`, `Subagent`, and the `Todo:*` group are deferred to a named follow-on project. Their research.md seeds are captured in [`TASK-NOTES-tool-calls-additional-tools.md`](../TASK-NOTES-tool-calls-additional-tools.md), with a `TASKS.md` entry: per-tool minimums, the globbing-library and runtime-detection open questions, and the version-advertisement and cwd/env requirements. They reuse this project's Step-0 contract unchanged.

This narrows the scope listed in research.md, whose *Scope / In* section placed those tool groups in this project. That listing is superseded here; research.md is itself internally inconsistent on the point (its *What is deferred* note already treats them as later work).

**Other deferred decisions** (research.md *What is deferred*): streaming tool responses; tool-call error recovery / `is_error: true` retry loop; per-turn tool variation (tools stay static per conversation in v1); `Message.cache` forwarding on tool turns; `NoopEngine::from_table` keyed-by-binding; Rig `AgentBuilder` / `multi_turn`.

**Open within this project, settled in feature designs:** `ToolExecutor` `Option`-vs-required signature; the insurance-claim noop-fixture shape. (research.md open #2, #3.)
