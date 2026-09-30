# DDD & Design Patterns Skills for Agentic Development

An agent skills collection for Domain-Driven Design, software design patterns, research, and a strict test-driven developer lifecycle. The repository ships Claude Code marketplace packaging today, and the skill instructions are written so other agent harnesses such as Codex, Copilot, and Gemini can adapt the same workflows through explicit tool mappings.

## Installation & Getting Started

### Claude Code Marketplace

Claude Code uses the `.claude-plugin/` directories as its marketplace adapter.

1. Add the marketplace from GitHub: run `/plugin marketplace add davidsouther/domain-driven-design`.
   * You can also run `/plugin` to open the interactive browser and add it from the **Marketplaces** tab.
2. Install the skills with `/plugin install <name>@ailly` — install `general`, `developer`, `patterns`, and `research` (`domain` is optional). Browse and toggle them anytime from the **Installed** tab of `/plugin`.

> **Updating:** Ailly does not auto-update when new versions are pushed. Run `/plugin marketplace update ailly` to pull the latest skills.
> **Local development:** To work from a local clone instead, run `/plugin marketplace add <path-to-clone>`.

### Codex Plugin Mirrors

Codex uses the `*/.codex-plugin/plugin.json` manifests as its plugin adapter. This repository includes Codex mirrors for the skill plugins: `developer`, `domain`, `general`, `patterns`, and `research`.

Repo-local Codex marketplace metadata lives at `.agents/plugins/marketplace.json`. Its entries point back to the plugin folders in this checkout, where each `.codex-plugin/plugin.json` lives. The `characters` package remains Claude Code-specific because it is an output-style package rather than a portable skill plugin.

### Pi Package

On [pi](https://github.com/badlogic/pi-mono), Ailly's workflow is implemented as tools rather than prose. The skills describe the process; the extensions in `.pi/extensions/` carry out the mechanical parts — spawning subagents, naming folders, checking that promised files exist — so those steps don't depend on a model following instructions correctly.

Install with `pi install git:davidsouther/domain-driven-design` from any project. To work on this repo itself, `.pi/settings.json` already self-references the package; approve the project (`pi -a`) and pi discovers the five skill plugins, the `/ailly*` prompt templates in `prompts/`, and the extensions. The root `package.json`'s `pi` manifest (`pi.skills`, `pi.prompts`, `pi.extensions`) contains nothing specific to this checkout's location.

Pi has no built-in `Task` tool, so subagent dispatch is a separate `pi` subprocess per call:

- `ailly_subagent` — runs one Ailly phase or ability in an isolated subprocess that reads exactly one reference file.
- `research_dispatch` — runs one or more research skills, in parallel when combined; computes the dated notes folder and checks each skill's findings file landed on disk.
- `review_run` — general:review's dispatch and converge steps in one call, returning a verified, deduplicated, severity-ranked findings list.
- `ailly_quick_loop` — the quick loop end to end: each phase, a review of each artifact including the build diff, and each build step verified by a sentinel line in `plan.md`. Halts and reports at the first missing artifact or aborted step.
- `ailly_long_loop_start` / `_status` / `_stop` — the long loop as a detached background process, so it doesn't hold a tool call open for hours; a watcher notifies the interactive session when the run stalls, escalates, or exits.
- `clarify` — answers one ad hoc question mid-task: a subagent checks local convention, researches what research can settle, and returns either a sourced answer or a structured escalation rather than a guess.
- `todo` — Ailly's `TodoWrite` equivalent.

The dispatching tools share one subprocess module and resolve reference paths relative to their own install location, so they work wherever the package is installed. The full tool-mapping table is in `developer/skills/ailly/references/agents/pi.md`.

The slash commands are the same as described below: `/ailly [phase] <request>`, the `/ailly-research` through `/ailly-cleanup` phase shortcuts, `/research <question>`, and `/review <artifact> [specialists]`.

### Other Agent Harnesses

Install or expose the `*/skills/*/SKILL.md` trees using your harness's native skill mechanism. `developer:ailly` is the lifecycle coordinator; it consults `developer/skills/ailly/references/agents/<harness>.md` for host-specific tool names and behavior. Mappings currently exist for Claude Code, Codex, Copilot, Gemini, and pi.

The `.claude-plugin/` and `.codex-plugin/` manifests are packaging metadata only. They are not the source of truth for the skill behavior.

### Starting Work

The examples below use Ailly's slash-command surface. In another harness, invoke the same `developer:ailly` skill or command equivalent and rely on the matching harness reference for tool-name translation.

1. Start a project in a new folder - `/ailly initialize a project called [name] for [design goal] using [TypeScript, Python, or Rust] (including initializing git)`.
2. Start work on a new feature - `/ailly start work on [description]`.
3. Start a feature in an isolated worktree - `/ailly in a worktree start work on [description]`.
4. Continue work on a specific phase - `/ailly continue [phase] [task-slug]`.
5. Fast-track a simple change - `/ailly finish [task] with a quick loop`.
6. Tell Ailly to do research herself - after Ailly asks you a question, `/using-research to perform a deep dive; pay attention especially to [area of interest]`.
7. When finished with a task, `/ailly cleanup [task] with a PR`. Or use `with a squash merge` for local development.

### How Ailly Works

Ailly structures development as five sequential phases, each isolated by the active harness's best mechanism to prevent context bloat:

| Phase | What happens |
|-------|-------------|
| **Research** | Gathers reference material, produces a refined report with user review |
| **Design** | Produces a design doc with purpose, user journey, spec, and alternatives; outputs a failing feature test |
| **Plan** | Details type-first, TDD implementation steps; user reviews before build begins |
| **Build** | Executes each plan step, runs checks, commits; targets a passing feature test |
| **Cleanup** | Removes session artifacts, squash-merges the branch |

Each phase produces a draft artifact. A human must review and clear the draft marker before the next phase begins — this keeps architectural decisions collaborative rather than delegated entirely to the agent.

The **quick loop** option compresses the human-reviewed draft structure for simple, unambiguous tasks while still producing minimal research, design, plan, build, and thinking artifacts. It pauses before cleanup so those artifacts can be reviewed, unless the user starts with "no review".

### Useful Patterns

- Write a complex task description to `.ailly/prompts/[topic].md`, then run it with `/ailly run .ailly/prompts/[topic].md`.
- Work on multiple independent features at the same time using worktrees: `/ailly in a worktree start work on [feature-a]` in one terminal, the same for feature-b in another.
- Check if Ailly has what she needs for research - `/ailly check if internal, books, and papers research are set up right`.
- Ask Ailly about herself - `/ailly what skills are available?` or `/ailly how do I update a git repo?`
- Change the `.ailly/` folder by adding `For developer: tools, use .agents/ instead of .ailly/` to your AGENTS.md or equivalent.

## Skills

### General (`general:*`)

Utilities for skill authoring, parallel agent dispatch, and git worktree isolation.

| Skill | When to use |
|-------|-------------|
| `general:using-general` | Bootstrap — load before any action to find the most relevant skills |
| `general:using-git-worktrees` | Starting feature work that needs isolation from the current workspace |
| `general:dispatching-agents` | Multiple independent tasks with no shared state or sequential dependencies |
| `general:writing-skills` | Creating, editing, or verifying skills before deployment |
| `general:c3-review` | An isolated final-handoff subagent collecting Correctness, Conciseness, and Clarity feedback for `general:review` |
| `general:review` | Composing C3 and applicable specialists for a final handoff review |

### Developer (`developer:*`)

A strict three-loop development lifecycle: design → feature test → TDD implementation.

| Skill | When to use |
|-------|-------------|
| `developer:ailly` | Bootstrap and session coordinator for all developer work. The main driver and the entry point for every lifecycle phase: `/ailly research`, `/ailly design`, `/ailly plan`, `/ailly build`, `/ailly cleanup`. Each phase loads one `references/phases/<phase>.md` and runs with the isolation mechanism supported by the active harness. It also routes the coordinator's progressive abilities — thinking (stuck on a build error), refactor (clean up green code), initialize (new project/language setup), and program-management (tracker task I/O and wiring). Running `/ailly` will get it going on the next thing. |
| `developer:clean-comments-review` | Reviewing comment and DocBlock audience and longevity (a review specialist consumed by `general:review`). |
| `developer:code-review` | Optional user review, offered after `general:review` and before the change is public. Launches [diffx](https://github.com/wong2/diffx) to collects the user's comments on the current diff. |

The developer lifecycle uses draft gates: each artifact (design doc, feature test, plan) must be human-reviewed and cleared before the next loop begins.

### Domain-Driven Design (`domain:*`)

Guides architectural decisions and domain modeling through the full DDD lifecycle.
The plugin exposes a single skill, `domain:using-domain`, whose body is the routing
surface. Each ability's full guidance lives in a reference under
`domain/skills/using-domain/references/<name>.md`, loaded on demand.

`domain:using-domain` is the bootstrap and router: it names the domain ability that fits
the situation and points at its `references/<name>.md`, with a glossary-first gate on any
new term.

The abilities it routes to (each a `references/<name>.md` reference): glossary,
ubiquitous-language, domain-model, contracts-and-invariants, arrow-of-maturity.

The Arrow of Maturity describes six architectural stages a DDD project grows through, from prototype scripts to event-sourced microservices. Each ability knows which stage is appropriate and what signal justifies advancing.

### Design Patterns (`patterns:*`)

Provides structured guidance for applying common software patterns at the right time.
The plugin exposes a single skill, `patterns:using-patterns`, whose body is the routing
surface. Each pattern's full guidance lives in a reference under
`patterns/skills/using-patterns/references/patterns/<name>.md`, loaded on demand.

`patterns:using-patterns` is the bootstrap and router: it names the pattern that fits a
design pressure and points at its `references/patterns/<name>.md`.

The patterns it routes to (each a `references/patterns/<name>.md` reference): aggregate,
arrange-act-assert, bifurcate, bootstrap-and-service, builder, configuring-feature-flags,
configuring-logging, domain-objects, emitting-logs, errors-typed-untyped, newtype,
parse-dont-validate, repository, triangulate, type-conversion, type-states,
unit-of-work, using-feature-flags, visibility.

### Research (`research:*`)

Selects the right research strategy based on what kind of question is being asked.

| Skill | When to use |
|-------|-------------|
| `research:using-research` | Bootstrap — establishes when to invoke each research skill |
| `research:archaeology` | Why did this code change? Who introduced it? When was it added or removed? |
| `research:codebase` | What does this code do right now? Where is a symbol defined or used? |
| `research:dependencies` | What version of a dependency is used? What does a package provide or require? |
| `research:domain` | What does this concept mean in the domain? How are domain terms defined? |
| `research:internal` | What do internal documents (Slack, Confluence, Linear, Notion) say about this? |
| `research:public` | What does the public internet say? What do official docs or forums say? |


## License

MIT
