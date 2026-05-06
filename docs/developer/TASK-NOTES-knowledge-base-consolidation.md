# Knowledge primitives consolidation into a single KnowledgeBase surface

The codebase carries several "things a model or runtime asks at the edge" primitives that have grown up independently: `KnowledgeBase` (in `src/knowledge/clarify.rs`), `ToolRegistry` (in `src/engine/`), `FsSkillRepository` (in `src/knowledge/skills/`), `MapKnowledgeBase`, `RefuseKnowledgeBase`, `DefaultOnlyKnowledgeBase`, and the `HashMapRegistry` carrying `fs.absent` plus `user.clarify` that the workflow CLI assembles. They overlap in intent (look up something the model needs) but diverge in shape.

This task is the design-and-implement slice that consolidates them. It is the natural follow-on to the workflow CLI wiring slice (`docs/developer/2026-05-05-B-workflow-cli-wiring/`), which chose CLI-side registration of `user.clarify` precisely because the type system was not yet ready to express the auto-registration guarantee.

## Why now

The workflow CLI wiring slice took option two of three: every harness wanting `user.clarify` has to register it itself, and the runtime contract that "every workflow can rely on `user.clarify`" lives in CLI documentation rather than in the type system. That choice traded type-system safety for a smaller diff. This consolidation slice is the place where the type-system guarantee is paid back.

## Scope to investigate, not to assume

The shape of the consolidation is the design question, not a foregone conclusion. Candidate shapes:

- A single `KnowledgeBase` trait that subsumes tool calls, skill body lookup, clarification, and fact recall. Implementations compose by delegation.
- A `KnowledgeRepository` umbrella holding a `ToolRegistry`, a `SkillRepository`, and a clarification backend, with one constructor per harness flavor (CLI, test, future MCP).
- Leave the traits separate but introduce a `Harness` aggregate that the runtime accepts as one parameter, replacing the current `tool_registry` plus future `kb` plus future `skills` parameters.

## Acceptance shape (subject to design)

- Workflow runtime `Runtime::new` accepts at most one harness aggregate, not a growing list of side parameters.
- A workflow file's `[inputs]` declarations, `[[tasks]] skills` references, and tool calls all resolve through the same surface.
- Auto-registration of "every workflow can rely on it" tools (today: `user.clarify`) happens in the harness construction, not in CLI prose.
- The existing `MapKnowledgeBase`, `DefaultOnlyKnowledgeBase`, and `RefuseKnowledgeBase` test doubles either survive verbatim or are replaced by clearly-named successors.

## Predecessor reading

- The existing one-line follow-up at the top of the SKILLS follow-on list in `TASKS.md` ("refactor Conversation to either not rely on Knowledge at all, or combine Skills, Tools, etc into a single KnowledgeBase trait or struct"). This task supersedes that line.
- `docs/developer/2026-05-03-B-knowledge-skills/design.md` for the SkillRepository shape.
- `docs/developer/2026-05-05-A-user-clarify/design.md` for the clarify-tool surface.
- `docs/developer/2026-05-05-B-workflow-cli-wiring/design.md` for the CLI-side auto-registration policy this task replaces with a type-system guarantee.
