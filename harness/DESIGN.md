# DESIGN

This document specifies Ailly's three YAML schemas: `conversation`, `assembly`, and `evaluation`.

## YAML Schemas

### `conversation`

A conversation file is multi-document YAML. The first document is the `meta` header; every subsequent document is one message in `session` order. Trace lives inline on each message; there is no sidecar `trace.json` or `response.json`.

```
meta:
  model: model_id                              # default model for the conversation
  debug?: bool                                 # emit extra trace events
  assembly?: string                            # name of the source assembly (when produced by `ailly assemble`)
  binding?: Map<string, value>                 # matrix binding values that produced this file
session: Message[]                             # ordered turn messages; one per YAML document after meta

Message:                                       # one per YAML document (--- separated)
  role: system | user | assistant | tool
  content?: string | ContentBlock[]            # text, tool_use, tool_result; absent on blank assistant turns
  cache?: bool                                 # mark end-of-message as a cache breakpoint
  trace?: Trace                                # inline per-message trace; populated after `ailly run`

ContentBlock:                                  # shape mirrors https://platform.claude.com/docs/en/api/messages
  type: text | tool_use | tool_result | thinking | image
  ...type-specific fields

Trace:                                         # OTEL gen_ai conventions (https://docs.rig.rs/docs/concepts/completion)
  span_id: string
  model: model_id                              # actual model called (may differ from meta.model under a provider matrix)
  tokens: { input: int, output: int, cache_hit?: int, cache_write?: int }
  latency_ms: int
  events: OTELEvent[]                          # gen_ai.* events, tool calls, errors
```

A message with `role: assistant` and no `content` is a blank slot left by `ailly assemble`; `ailly run` walks the session in order and fills any blank assistant message by calling the model with the prior messages as input. After running, the assistant message has content and trace populated; the user, system, and tool messages from the source assembly are byte-identical to their pre-run state.

Cache markers on messages serve the same role as cache markers on `assembly.prefix` blocks: they declare where a prompt-cache breakpoint should land. The trace records whether the breakpoint hit on each subsequent call.

### `assembly`

```
name: string
model: model_id                              # default model; can be overridden per matrix binding
matrix: Map<string, list<value>>             # cross-product over named axes; one conversation per binding
prefix: PrefixBlock[]                        # ordered content rendered into the cached prefix
conversation: TurnTemplate[]                 # ordered user/assistant turns; blank assistant turns are filled by `ailly run`

PrefixBlock:
  kind: file | system | tools | examples | context
  path?: string                              # file or glob; templated with matrix variables
  source?: string                            # directory (kind: context)
  glob?: string                              # filter inside source (kind: context)
  count?: number                             # cap on selected items (kind: context)
  cache: bool                                # mark end-of-block as a cache breakpoint

TurnTemplate:
  role: user | assistant
  path?: string                              # for role=user; templated with matrix variables. Omitted for blank assistant turns.
  cache: bool                                # mark end-of-turn as a cache breakpoint
```

The `kind:` tag on a prefix block is a documentation/convention label; the engine treats every block as ordered text concatenated into the window. Cache breakpoints ride on the blocks and turns they cache; there is no separate `cache_breakpoints:` list.

No content is included implicitly. `AGENTS.md` only appears in the window if the assembly names it (typically `{kind: file, path: ./AGENTS.md}` as the first prefix block).

`matrix:` expands into the cross-product of its axes. `ailly assemble` writes one conversation file per binding into `runs/<id>/`, named by the binding values. Single-axis matrices produce flat filenames; multi-axis matrices join with `-`.

The CLI flag `--case <name>` restricts `assemble`, `run`, and `eval` to the named case(s) after matrix expansion, rather than the whole matrix or run directory. It is repeatable (`--case a --case b` selects both) and matches by exact string against the case/conversation name — the binding's derived filename stem for `assemble`, the conversation key's name for `run`, and both the conversation key and any suite case's `name:` for `eval`. Omitted (the default) means no filter: every binding, every resolved conversation, and every suite case is processed, identical to today's behavior. A `--case` value matching nothing in the target is a hard error naming the value(s) that failed to match and the case names that were actually available; this fires per-value, so one correct name mixed with one typo still errors rather than silently running a smaller, unintended set.

### `evaluation`

An eval suite is a list of cases. Each case binds to zero, one, or many conversation files in a run directory and applies its assertions to each match. Matching is by `name` (the conversation filename stem), by `when` (a filter over the matrix binding values recorded in `meta.binding`), or by neither (which matches every conversation in the run directory).

```
name: string
cases: Case[]

Case:
  name?: string                                # matches `<name>.yaml` in the run directory
  when?: Map<string, value>                    # matches conversations whose meta.binding is a superset of these key/value pairs
  assertions: Assertion[]                      # applied to every matched conversation

Assertion:
  # ─── Open-ended assertions (delegate to a model, tool, or external process)
  | { type: "judge";       prompt: string }                                                # sends the final assistant turn plus prompt; passes if the judge accepts
  | { type: "tool";        tool_call: ToolCallSpec }                                       # invokes an existing tool with the final response as input
  | { type: "script";   runtime: Node | Python; script: { contents | path: string }; pass_env?: string[] }   # runtime process; cleared env + base allowlist + any pass_env names; exit 0 passes; candidate response on stdin, question in AILLY_USER_QUESTION; non-zero exit ⇒ Fail (stdout as reason), or Errored when stdout empty and stderr present
  | { type: "program";  script: string; pass_env?: string[] }                               # path or PATH-looked-up executable; spawned directly (no shell); cleared env + base allowlist + pass_env; same stdin / env / exit contract as script

  # ─── Tool-call assertions (over assistant tool_use blocks across the session)
  | { type: "must_call_tool";     tool: string; with_args?: Record<string, unknown> }
  | { type: "must_not_call_tool"; tool: string }
  | { type: "tool_call_count";    tool?: string; op: Op; value: number }
  | { type: "tool_call_order";    sequence: string[] }
  | { type: "tool_call_collection"; tools: string[] }   # order-insensitive multiset: each name must appear at least as many times as listed; extra/intervening calls are ignored

  # ─── Text assertions (over the final assistant turn's text content)
  | { type: "text_contains";       value: string; case_sensitive?: bool }
  | { type: "text_not_contains";   value: string; case_sensitive?: bool }
  | { type: "text_matches";        pattern: string; flags?: string }
  | { type: "text_equals";         value: string }
  | { type: "text_semantic_match"; value: string; threshold?: number }

  # ─── Structural assertions (walk the conversation as JSON)
  | { type: "json_path";      path: string; op: Op; value: unknown }
  | { type: "response_field"; path: string; exists: bool }

  # ─── Performance assertions (read inline trace on session messages)
  | { type: "tokens";     metric: "total" | "input" | "output"; op: Op; value: number }
  | { type: "latency_ms"; op: Op; value: number }

Op: "==" | "!=" | "<" | "<=" | ">" | ">="
```

`ailly eval <suite> --over <run-dir>` walks each conversation in the run directory, finds the cases that match it, and applies the assertions. Assertions consume the conversation file directly: text assertions read the final assistant turn's text, tool-call assertions inspect tool_use blocks across the session, structural assertions walk the conversation as JSON, and performance assertions sum the inline trace across messages.

A case with no `name:` and no `when:` matches every conversation in the run directory and runs its assertions once per match. This is how cross-binding rollups (e.g. a single judge prompt over every provider's output for a domain) are written: omit the filter, let the case fan out, and have the judge read `program_outputs` from the prior cases' assertion results.

`when:` is a subset match against `meta.binding`. A case with `when: { domain: prose-bio }` matches every conversation whose binding includes that key/value, regardless of the other axes; a case with `when: { domain: prose-bio, provider: anthropic }` narrows further. This is how one assertion template covers a whole matrix axis without enumerating the cross-product by hand.

The suite produces one report per run, written to `evals/reports/<run-id>.json`. The report records each case, each match, each assertion result, and aggregate pass rates per assertion class so a regression surfaces both the failing case and the failing assertion type.