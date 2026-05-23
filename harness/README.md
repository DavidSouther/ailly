# Ailly 2

Capability is driven by verification, not complexity.

Ailly is a Context Window Swiss Army Knife. Ailly stores all prompts and sessions in version controlled local folder. This allows editing and reviewing entire LLM sessions offline, outside of any specific user interface or application. Context windows can be built exactly as desired, not relying on user agents' proprietary and unknown algorithms.

Ailly provides fine-grained control over context window without needing an SDK or programming. By keeping all prompting and context in files that are easily written and reviewed, teams don't need to provide their own code to run LLM experiments. With standard conventions and formats, common experimental patterns can be quickly templated, run, and evaluated.

Ailly makes it simple to prepare AI experimental setups, especially ones that allow A/B testing skill and context window contributions. Teams can prepare session templates, then instantiate prompt instances, run them in parallel, and perform automated evals with a single command. Continuous Integration can perform automated and regression testing when prompts in agented applications are updated.

## Modules

### Content

The structs Conversation, Assembly, and Evaluation provide the inner domain model and framework for Ailly. Each content type can be serialized, via serde, to YAML or Toml. See below for schema outlines.

- **Conversation** is an LLM Session Multi-document yaml, each document being one turn message. One session per file. Additionally Ailly information optionally controls how Ailly will execute the conversation. Trace information stored in-line with messages.
- **Assembly**: describes how to build a single run from a project's context. Entries refer to various files and patterns in the project, and how to coordinate them into a single conversation. Assemblies can be thought of as Conversation Templates (or meta-conversations?).
- **Evaluation**: describes how to mark whether a conversation (or final response in a conversation) did or did not achieve a goal. In the Arrange-Act-Assert testing pattern, Assemblies are what Arranges, Run is what Acts, and Evaluations are a collection of Assertions to check in the resulting run file.

### Engine

Engine is the underlying piece that calls LLM providers. It is primarily a wrapper around rig, but also handles converting from Ailly's formats to other agents', and providing a2a services. An EngineProvider trait allows swapping between specific implementations for completion providers; initially, this includes the Rig create for 3p network api calls, Noop for a deterministic / scriptable system, and Native, which runs locally (and itself will need to research between llama_cpp, candle). Emits telemetry information, both for collectors (eg lapdog) and for Conversation trace details.

### Project

Organizes an entire project into its context/, prompts/, assemblies/, runs/, and evals/ folders. These conventions streamline prompt assembly.

### CLI

```bash
ailly run conversation.yaml # Run a single conversation through LLM
ailly assemble assembly.yaml > conversation.yaml # Prepare a conversation from an assembly.
ailly eval --suite regression.yaml conversation.yaml # Run an evaluation suite on a finished conversation.

# And various aggregate forms in a project with `-p`
```

### Integrations

The [`e2e/`](e2e/) folder contains sample projects, and scripts to run said projects in a CI environment to ensure Ailly is always succeeding at its critical user journeys. Each subfolder is one end-to-end test, with its own README holding the project layout, assembly and eval files, and workflow.

#### [Classification](e2e/insurance-claim/README.md)

A worked example of a single-prompt application: an insurance claim handler that classifies claims as auto-approve, human-review, or reject. The context window is composed from system fragments, JSON Schema tool definitions, few-shot exemplars, and a retrieved knowledge corpus; the assembly is run against a user prompt, captured into a timestamped replayable run directory, and judged by a regression suite that asserts across three failure modes: behavioural (which tool was called), textual (what the response said), and efficiency (token budget per cache breakpoint).

The project doubles as a fixture for the four claims Ailly makes about itself: context windows built exactly as written rather than chosen by a proprietary agent loop, plain files in version control rather than an SDK, byte-replayable runs, and one-command A/B sweeps that report tool-call, text, and budget deltas between assembly variants. The CI step reads its regression report alongside the patterns-eval suite from a shared report format.

#### [Patterns skill eval](e2e/patterns-eval/README.md)

A regression check for the `patterns:*` plugin from [davidsouther/domain-driven-design](https://github.com/davidsouther/domain-driven-design), which provides LLM coding agent skills for software development tasks. The eval runs a minimal cross-section of three skills (`newtype`, `configuring-logging`, `emitting-logs`) across two axes: discovery confirms the model selects the right skill from its `description:` frontmatter for a given code situation, and invocation confirms the produced code structurally exhibits the pattern (graded by an LLM-as-judge plus per-skill Python checkers). The three skills are chosen because their discovery surfaces overlap: `configuring-logging` and `emitting-logs` are a paired bootstrap/per-call-site set, and `newtype` shares description vocabulary with `entities-value-objects-services`.

The report is a matrix of skill by axis by pass rate, so a regression after an edit to any `SKILL.md` surfaces both the affected skill and which axis blurred. Paired-skill cases under the discovery axis catch the failure mode that single-skill cases would miss: when a `description:` edit pulls two paired skills' triggers toward each other, both per-skill cases still pass and only the cross case shows the blur. The eval reuses the insurance-claim handler's report format, so the same CI step reads both.

#### [DELEGATE-52](e2e/delegate-52/README.md)

A scaled-down reproduction of the delegated-workflow protocol from Laban, Schnabel, and Neville (_LLMs Corrupt Your Documents When You Delegate_, [arXiv:2604.15597v1](https://arxiv.org/abs/2604.15597), Microsoft Research, April 2026), packaged as an Ailly content folder. The original paper measures silent document corruption across 52 professional domains and 19 LLMs; this e2e runs the same protocol at fixture scale (four representative domains, a six-turn workflow, three provider families) and feeds the artifacts into the paper's per-domain scorers ported from [microsoft/DELEGATE52](https://github.com/microsoft/DELEGATE52).

The integration also demonstrates three streamlining wins Ailly provides over hand-written driver code: multi-provider parity from a single source of truth (one assembly recipe, three providers swept via the `providers:` matrix), a filesystem-as-history audit trail (every turn's window, response, post-edit document, and diff land on disk in plaintext), and declarative composition of seed plus distractor context (sweeping the distractor count along the paper's documented degradation axis is one variable change, not a code edit).

## YAML Schemas

### `conversation`

meta: {model: model_id, debug}
session: Message[]
Message: SystemMessage|UserMessage|AssistantMessage etc from https://platform.claude.com/docs/en/api/messages, https://docs.rig.rs/docs/concepts/completion plus tracing spans & OTEL gen_ai.

### `assembly`

name
messages: Message[]
variables: Map<string, string>
tools: ToolList[]
out dir

### `evaluation`

name
assertions: Assertion

```
Assertion:
| { type: "judge"; prompt: string } // Sends the final response plus the prompt and asks whether the judge prompt accepts the result.
| { type: "tool"; tool_call } // Freeform tool call from existing tools, taking the final response as the input to the tool call.
| { type: "script"; runtime: Node|Python; script: {contents: string}|{path: string}} // Process execution returning 0 for success, taking the final response on stdin and writing any reasoning to stdout.
/ { type: "program"; script: string }  // Path to a validator that is executable. Takes the yaml as stdin and outputs on stdout. Exits 0 for success, any other number for failure. Script is shell interpreted, while `script` is a Node or Python literal.
// ─── Tool-call assertions
| { type: "must_call_tool"; tool: string; with_args?: Record<string, unknown> }
| { type: "must_not_call_tool"; tool: string}
| { type: "tool_call_count"; tool?: string; op: Op; value: number }
| { type: "tool_call_order"; sequence: string[] }
// ─── Text assertions
| { type: "text_contains"; value: string;case_sensitive?: boolean }
| { type: "text_not_contains"; value: string; case_sensitive?: boolean }
| { type: "text_matches";pattern: string; flags?: string }
| { type: "text_equals"; value: string }
| { type: "text_semantic_match"; value: string;threshold?: number }
// ─── Structural assertions
| { type:"json_path"; path: string; op: Op; value: unknown }
| { type: "response_field"; path: string; exists: boolean }
// ─── Performance assertions
| { type: "tokens"; metric: "total" | "input" |"output"; op: Op; value: number }
| { type: "latency_ms"; op: Op; value: number }
```
