# Web research assistant

A worked example of a tool-using application built with Ailly: a research
context window assembled from declarative parts (a system policy fragment and
two JSON Schema tool definitions), driven over a research question that needs
both tools, and graded by a suite that asserts on the *tool calls* the run
produced. The project is the e2e fixture for Ailly's tool-call slice — it proves
the `assemble -> run -> eval -> report` loop scores `must_call_tool` and
`tool_call_order` over a real multi-turn tool conversation, with no live API.

| Ailly claim | How this project demonstrates it |
|---|---|
| **Context windows built exactly as desired** | The assembly's `prefix:` names every block in order: `AGENTS.md` at position zero, the system policy fragment, then the two tool definitions resolved by `{ kind: tools, path: context/tools/*.json }` into `meta.tools`. No agent loop chooses what gets declared. |
| **Plain files, no SDK** | Every input is markdown, JSON Schema, or YAML in version control. Adding a tool is dropping a `*.json` under `context/tools/`; the glob picks it up. |
| **Replayable runs** | The pre-filled `fixtures/web-research.yaml` *is* a complete run artifact — the full `user -> assistant(tool_use) -> tool(tool_result) -> assistant(text)` shape. `ailly run` over it fills nothing (no blank assistant slot), so it is a byte-identical no-op; `ailly eval` scores the authored `tool_use` blocks exactly as if a model had emitted them. |
| **A/B testing with one command** | The same assembly and eval suite drive a noop structural gate and an optional live run from a single `ci.sh`; the report format is shared with the insurance-claim and patterns-eval suites. |

## What the assistant does

A research assistant reads a question, searches the web for a candidate source
with `web_search`, fetches that source with `web_fetch`, and answers from the
page it actually retrieved — citing the URL. The interesting property is the
*order*: search before fetch. The eval suite encodes that as a
`tool_call_order` assertion, alongside two `must_call_tool` assertions that the
run touched both tools at all.

The scenario is intentionally minimal — one system fragment, two tools, one
case — so the fixture stays readable and the tool-call assertions are the
whole story. The team owning this folder writes context and assertions, not
inference code.

## Test surface

```
e2e/research/
├── AGENTS.md                          # Project constitution. Named explicitly at prefix position zero.
├── .gitignore                         # ignores runs/ and evals/reports/.
├── context/
│   ├── system/
│   │   └── 00-research-policy.md       # "Search before you fetch; cite what you fetch."
│   └── tools/
│       ├── web_search.json             # ToolDefinition: ranked result titles + URLs.
│       └── web_fetch.json              # ToolDefinition: the body of a URL as text.
├── prompts/
│   └── web-research.md                 # One research question needing a search then a fetch.
├── assemblies/
│   └── research.yaml                   # prefix declares both tools; one-case matrix.
├── fixtures/
│   └── web-research.yaml               # Pre-filled noop tool conversation; the structural gate's run artifact.
├── runs/                               # gitignored; `ailly assemble` writes skeletons here.
└── evals/
    ├── research.yaml                   # must_call_tool web_search + web_fetch; tool_call_order [web_search, web_fetch].
    └── reports/                        # gitignored.
```

## Assembly (declarative tool-using context window)

`assemblies/research.yaml`:

```yaml
name: research
model: claude-sonnet-4-6

matrix:
  case: [web-research]

prefix:
  - { kind: file,   path: ./AGENTS.md,             cache: true }
  - { kind: system, path: context/system/*.md,     cache: true }
  - { kind: tools,  path: context/tools/*.json,     cache: true }

conversation:
  - { role: user, path: "prompts/{{ case }}.md" }
  - { role: assistant }
```

What this proves about context-window management:

- **Tools are declared, not narrated.** The `kind: tools` block resolves its
  `context/tools/*.json` glob to structured `ToolDefinition`s on `meta.tools`;
  it produces no system message. The rig adapter forwards them on the
  completion request, so the model can emit `ToolUse` blocks.
- **The matrix is the sweep.** `case:` enumerates one binding; `ailly assemble`
  writes one conversation skeleton. Adding a question is one line plus one file
  under `prompts/`.

## Evaluation (assert on the tool calls)

`evals/research.yaml`. The case `name` matches the conversation filename
produced by the matrix; no `input:` field is needed.

```yaml
name: research
cases:
  - name: web-research
    assertions:
      - { type: must_call_tool, tool: web_search }
      - { type: must_call_tool, tool: web_fetch }
      - { type: tool_call_order, sequence: [web_search, web_fetch] }
```

What this proves about LLM evaluation:

- **Behavioural assertions check actions, not words.** `must_call_tool` reads
  the `tool_use` blocks `ailly eval` extracts from the conversation;
  `tool_call_order` pins that `web_search` precedes `web_fetch`.
- **The fixture is the script.** Because the pre-filled fixture carries both
  `tool_use` blocks, the assertions score without a live model — the same read
  path that scores a freshly-run conversation.

## CI integration

`ci.sh` exits 0 from a clean clone with no credentials. It drives the
operator's four-subcommand journey, proving the tool-call assertions
structurally before riding the optional live half:

```sh
bash e2e/research/ci.sh
```

1. **assemble** (always): `ailly -p . assemble research` writes exactly one
   conversation skeleton under `runs/<id>/`.
2. **structural tool-call gate** (always, noop): copies
   `fixtures/web-research.yaml` into a fresh in-tree run dir, runs `ailly run`
   over it as a verified no-op (idempotent — fills no blank), then `ailly eval
   research` and reads the report JSON, asserting `passed >= 3` and
   `failed == 0`. This proves `must_call_tool: web_search`,
   `must_call_tool: web_fetch`, and `tool_call_order: [web_search, web_fetch]`
   fire on the multi-turn shape with no live API.
3. **live run** (gated): runs the assembled skeleton through a real model when
   `ANTHROPIC_API_KEY` (or a project `.env`) is present, then `eval` + `report`.
   Skipped with a clear notice otherwise.
4. **report** (always): over the structural run; asserts the single-run
   markdown report wrote.

A contributor may drop an `e2e/research/.env` with `ANTHROPIC_API_KEY` instead
of exporting it in the shell; `ailly run` and `ailly eval` load it via
[src/cli/env.rs](../../src/cli/env.rs), and an exported shell var still wins
over the file.

## How the tool calls reach CI without a live API

Under a noop model the CLI cannot *produce* a tool call: `NoopEngine::auto()`
emits only `Content::Text`, and `cli/run.rs` constructs the empty
`NoopToolExecutor::default()`. The resolution is the pre-filled fixture: a
committed conversation that already carries the full `user ->
assistant(tool_use web_search) -> tool(tool_result) -> assistant(tool_use
web_fetch) -> tool(tool_result) -> assistant(text)` shape with **no blank
assistant slot**. `ailly run` over it fills nothing (the loop walks only blank
slots), so it is a byte-identical no-op; the fixture *is* the script. `ailly
eval` then scores the authored `tool_use` blocks directly. This mirrors the
synthetic-conversation convention the `e2e_delegate_52` and `e2e_patterns_eval`
Rust tests already use, adds no CLI surface, and builds no executor registry.
