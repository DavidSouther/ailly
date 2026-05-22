# Ailly 2

Capability is driven by verification, not complexity.

Ailly is a Context Window Swiss Army Knife. Ailly stores all prompts and sessions in version controlled local folder. This allows editing and reviewing entire LLM sessions offline, outside of any specific user interface or application. Context windows can be built exactly as desired, not relying on user agents' proprietary and unknown algorithms.

Ailly provides fine-grained control over context window without needing an SDK or programming. By keeping all prompting and context in files that are easily written and reviewed, teams don't need to provide their own code to run LLM experiments. With standard conventions and formats, common experimental patterns can be quickly templated, run, and evaluated.

Ailly makes it simple to prepare AI experimental setups, especially ones that allow A/B testing skill and context window contributions. Teams can prepare session templates, then instantiate prompt instances, run them in parallel, and perform automated evals with a single command. Continuous Integration can perform automated and regression testing when prompts in agented applications are updated.

## Modules

### Content
The structs Conversation, Assembly, and Evaluation provide the inner domain model and framework for Ailly. Each content type can be serialized, via serde, to YAML or Toml. See below for schema outlines.

* **Conversation** is an LLM Session Multi-document yaml, each document being one turn message. One session per file. Additionally Ailly information optionally controls how Ailly will execute the conversation. Trace information stored in-line with messages.
* **Assembly**: describes how to build a single run from a project's context. Entries refer to various files and patterns in the project, and how to coordinate them into a single conversation. Assemblies can be thought of as Conversation Templates (or meta-conversations?).
* **Evaluation**: describes how to mark whether a conversation (or final response in a conversation) did or did not achieve a goal. In the Arrange-Act-Assert testing pattern, Assemblies are what Arranges, Run is what Acts, and Evaluations are a collection of Assertions to check in the resulting run file.

### Engine
Engine is the underlying piece that calls LLM providers. It is primarily a wrapper around rig, but also handles converting from Ailly's formats to other agents', and providing a2a services. An EngineProvider trait allows swapping between specific implementations for completion providers; initially, this includes the Rig create for 3p network api calls, Noop for a deterministic / scriptable system, and Native, which runs locally (and itself will need to research between llama_cpp, candle). Emits telemetry information, both for collectors (eg lapdog) and for Conversation trace details.

### Knowledge
An agentic knowledge system, which serves to manage parts of the available context including skills, MCP tools, and knowledge base components. Also has "Thinking Fast and Slow" components to assist in creating a context window, given a prompt, without necessarily needing to invoke a high-cost thinking LLM. Provides MCP Tool implementations (including guards) for FileSystem (Glob, Read, Write), Shell (Bash, Python, JavaScript), Web (Search, Fetch), and Clarify (Knowledge, Ask). Provides knowledge graph SDK.

### Workflow
LLM as Step, rather than LLM As Driver, workflow engine. Workflows are a collection of (Name, Task, Transition) tuples, where Task defines an Assembly to run, and Transition describes an Evaluation that results in a Name within the workflow to execute next.

### Project
Organizes an entire project into its context/, prompts/, assemblies/, runs/, and evals/ folders.

### CLI

```bash
run conversation.yaml # Run a single conversation through LLM
assemble assembly.yaml > conversation.yaml # Prepare a conversation from an assembly.
eval --suite regression.yaml conversation.yaml # Run an evaluation suite on a finished conversation.

# And various aggregate forms in a project with `-p`
```

### Integrations

The `e2e` folder contains several sample projects, and scripts to run said projects in a CI environment to ensure Ailly is always succeeding at its critical user journeys.

#### Cooking Claude

> Claude hallucinated this one during initial design. It looks feasible but needs refinement.

```
project/
├── AGENT.md                     # Project constitution. Pinned, cached.
├── context/
│   ├── system/                  # System prompt fragments
│   │   ├── 00-persona.md
│   │   ├── 10-constraints.md
│   │   └── 20-tools-policy.md
│   ├── tools/                   # JSON Schema tool definitions
│   │   ├── search.json
│   │   └── calc.json
│   ├── examples/                # Few-shot exemplars
│   │   └── classification/
│   └── knowledge/               # RAG corpus
│       └── docs/
├── prompts/                     # User-prompt templates and test inputs
│   ├── default.md
│   └── edge-cases/
├── assemblies/                  # Recipes that compose windows
│   └── claim-handler.yaml
├── runs/                        # Timestamped, replayable outputs
│   └── 2026-05-20T14-32-claim-handler/
│       ├── window.txt           # The assembled context
│       ├── response.json        # Model output
│       ├── trace.json           # Tool calls, tokens, timing
│       └── meta.yaml            # Recipe SHA, git SHA, model
└── evals/
    ├── suites/regression.yaml   # Assertions over runs
    └── reports/                 # Pass/fail history
```

CLI Example:

```
# Assemble, run, eval, in one chain
ailly assemble claim-handler \
  | ailly run --user-prompt prompts/edge-cases/missing-fields.md \
  | ailly eval --suite regression

# Sweep two assembly variants over the same input
for v in v1 v2; do
  ailly assemble "$v" \
    | ailly run --user-prompt prompts/test.md
done
ailly diff runs/v1-* runs/v2-*
```

`assemblies/claim-handler.yaml`:

```
agent_md: ./AGENT.md
system:
  - context/system/00-persona.md
  - context/system/10-constraints.md
  - context/system/20-tools-policy.md
tools:
  - context/tools/search.json
  - context/tools/calc.json
examples:
  - context/examples/classification/*.md
retrieval:
  source: context/knowledge/docs/
  query: "{{ user_prompt }}"
  top_k: 5
user_prompt: prompts/default.md
model: claude-opus-4-7
cache_breakpoints: [after_system, after_tools]
```

`evals/suites/regression.yaml`:

```
cases:
  - name: missing-policy-number
    input: prompts/edge-cases/missing-fields.md
    assertions:
      - response.must_call_tool: lookup_policy
      - response.text.contains: "policy number required"
      - response.tool_calls.length: "<= 2"
      - trace.tokens.total: "< 8000"

  - name: ambiguous-claim
    input: prompts/edge-cases/ambiguous.md
    assertions:
      - response.must_not_call_tool: auto_approve
      - response.text.matches: "/clarif|specif/i"
```

Workflow:

	1.	Edit context/system/10-constraints.md.
	2.	`ailly -p . assemble claim-handler`. The window rebuilds.
	3.	`ailly -p . eval claim-handler --suite regression`.
	4.	Read evals/reports/<ts>.json. Fourteen of fifteen pass. The one failure is the new behavior intended.
	5.	Commit. Next change is measured against this baseline.


#### DELEGATE-52

Reproduce a scaled-down version of the DELEGATE-52 round-trip relay experiment from Laban, Schnabel, and Neville (*LLMs Corrupt Your Documents When You Delegate*, arXiv:2604.15597v1) as an Ailly content folder. The folder must:

1. Run the same protocol against ChatGPT (OpenAI), Gemini (Google), and Claude (Anthropic), producing artifacts the paper's per-domain scorers can ingest.
2. Demonstrate three streamlining wins that Ailly provides over hand-written driver code: multi-provider parity from a single source of truth, filesystem-as-history audit trail, and declarative composition of seed plus distractor context.

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

