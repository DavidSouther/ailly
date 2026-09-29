# Ailly 2

Capability is driven by verification, not complexity.

Ailly is a Context Window Swiss Army Knife. Ailly stores all prompts and sessions in a version-controlled local folder. Entire LLM sessions can be edited and reviewed offline, outside any specific user interface or application. Context windows are built exactly as written, not chosen by a proprietary agent loop. Because every prompt and every fragment of context lives in a file, teams run LLM experiments without an SDK and without bespoke driver code.

Ailly makes it simple to prepare AI experimental setups, especially ones that A/B test skill and context window contributions. Teams prepare session templates, instantiate prompt instances, run them in parallel, and perform automated evaluations with a single command. Continuous Integration performs automated regression testing when prompts in agented applications are updated.

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

Ailly's CLI provides four commands: `assemble` to prepare a Conversation file from an Assembly; `run` to run a Conversation through an LLM inference provider; `eval` to perform automated evaluations on those runs; and `report` to summarize one run or compare two.

```bash
ailly assemble <name>                       # Expand matrix; write N skeleton conversations to runs/<id>/
ailly run <conversation.yaml | run-dir>     # Fill blank assistant turns by calling the model
ailly eval <suite> --over <run-dir>         # Score conversations against assertions
ailly report <run-id>                       # Summarize one run; `report <id-a> <id-b>` compares two

# Project form: `-p <project-dir>` resolves assemblies/<name>.yaml and evals/<name>.yaml by convention.
ailly -p e2e/insurance-claim assemble claim-handler
ailly -p e2e/insurance-claim run runs/2026-05-23T14-32-claim-handler/
ailly -p e2e/insurance-claim eval regression --over runs/2026-05-23T14-32-claim-handler/
ailly -p e2e/insurance-claim report 2026-05-23T14-32-claim-handler
```

`assemble` expands the assembly's `matrix:` into one conversation file per binding, with user turns filled and assistant turns blank. `run` walks each conversation and asks the model to fill any blank assistant turn. The conversation file is the run artifact; there is no parallel `window.txt`, `response.json`, or `meta.yaml`. Trace lives inline per message, per the conversation schema. `eval` scores the filled conversations against a suite's assertions, and `report` renders those scores — a single-run summary, or a two-run comparison that sorts each assertion into improved / regressed / unchanged.

> A `diff` of the conversation files themselves comes from source control or native diff tools; `report` compares the *scored* outcomes.

### Integrations & Examples

The [`e2e/`](e2e/) folder contains sample projects, and scripts to run said projects in a CI environment to ensure Ailly is always succeeding at its critical user journeys. Each subfolder is one end-to-end test, with its own README holding the project layout, assembly and eval files, and workflow.

#### [Classification](e2e/insurance-claim/README.md)

A worked example of a single-prompt application: an insurance claim handler that classifies claims as auto-approve, human-review, or reject. The assembly's prefix composes system fragments, JSON Schema tool definitions, few-shot exemplars, and a retrieved knowledge corpus. The matrix sweeps four edge-case prompts, and each binding is written to one conversation file. Evaluations are judged by a regression suite that asserts across three failure modes: behavioural (which tool was called), textual (what the response said), and efficiency (token budget per cache breakpoint).

The project doubles as a fixture for the four claims Ailly makes about itself: context windows built exactly as written rather than chosen by a proprietary agent loop, plain files in version control rather than an SDK, conversation-as-run-artifact replay, and one-command A/B sweeps that report tool-call, text, and budget deltas between assembly variants. The CI step reads its regression report alongside the patterns-eval suite from a shared report format.

#### [Patterns skill eval](e2e/patterns-eval/README.md)

A regression check for the `patterns:*` plugin from [davidsouther/domain-driven-design](https://github.com/davidsouther/domain-driven-design), which provides LLM coding agent skills for software development tasks. The eval runs a minimal cross-section of three skills (`newtype`, `configuring-logging`, `emitting-logs`) across two axes: discovery confirms the model selects the right skill from its `description:` frontmatter for a given code situation, and invocation confirms the produced code structurally exhibits the pattern (graded by an LLM-as-judge plus per-skill Python checkers). The three skills are chosen because their discovery surfaces overlap: `configuring-logging` and `emitting-logs` are a paired bootstrap/per-call-site set, and `newtype` shares description vocabulary with `entities-value-objects-services`.

The report is a matrix of skill by axis by pass rate, so a regression after an edit to any `SKILL.md` surfaces both the affected skill and which axis blurred. Paired-skill cases under the discovery axis catch the failure mode that single-skill cases would miss: when a `description:` edit pulls two paired skills' triggers toward each other, both per-skill cases still pass and only the cross case shows the blur. The eval reuses the insurance-claim handler's report format, so the same CI step reads both.

#### [DELEGATE-52](e2e/delegate-52/README.md)

A scaled-down reproduction of the delegated-workflow protocol from Laban, Schnabel, and Neville (_LLMs Corrupt Your Documents When You Delegate_, [arXiv:2604.15597v1](https://arxiv.org/abs/2604.15597), Microsoft Research, April 2026), packaged as an Ailly content folder. See the e2e README for the fixture-scale matrix (four domains, six turns, three provider families), the scorers ported from [microsoft/DELEGATE52](https://github.com/microsoft/DELEGATE52), and the three streamlining wins it demonstrates over hand-written driver code.

## Schemas

Schemas for three serialization formats are detailed in DESIGN.md. The formats are Assemblies, which describe how to create conversations from content libraries; Conversations, which describe the shape of a conversation both before and after running through inference (and including trace and metadata details); and Evaluations, which describe how to rate and review completed conversations.