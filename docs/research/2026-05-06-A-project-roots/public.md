# Project root, session root, and knowledge root across coding agents

Date: 2026-05-06. Survey of how popular coding agents separate (or conflate) three distinct concepts: the **project root** (where edits are scoped), the **session root** (where transcripts persist), and the **knowledge root(s)** (where reusable rules, skills, prompts, agents, and MCP configs are discovered).

## Comparison table

| Tool | Project root | Session storage | Knowledge dirs | Precedence on conflict |
|---|---|---|---|---|
| Claude Code | cwd / git root | Global `~/.claude/projects/<hash-of-path>/` | `.claude/` + `~/.claude/` (memory, skills, commands, agents, hooks, settings) | Managed > CLI flags > project > user; skill beats command of the same name; project CLAUDE.md beats user CLAUDE.md |
| Cursor | Workspace folder(s) | Global SQLite under app support, keyed by workspace path | `.cursor/rules/` + user rules in settings + team rules from dashboard; `.cursor/mcp.json` + `~/.cursor/mcp.json` | Team > Project > User for rules; Project wins for MCP server name conflicts; nested `AGENTS.md` more-specific wins |
| Aider | Git repo root | `.aider.chat.history.md` in repo root (project-local) | `.aider.conf.yml` (cwd, git root, home), `.aiderignore`, conventions files via `--read` | Home -> git root -> cwd, with later overriding earlier for config |
| Continue.dev | VS Code workspace | IDE-managed (extension storage) | `.continue/` project + `~/.continue/` global (rules, prompts, models, MCP) | Global merges with project; rule files load lexicographically; YAML beats JSON when both exist |
| GitHub Copilot coding agent | Repository (cloud sandbox) | Server-side session, resumable via `/resume` or `--resume` | `.github/copilot-instructions.md`, `.github/instructions/*.instructions.md`, `AGENTS.md` | Path-specific instructions augment repo-wide; AGENTS.md adopted as fallback |
| Codex CLI | Git root, walking down to cwd | `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl` | `~/.codex/AGENTS.md` (or `AGENTS.override.md`) plus per-directory `AGENTS.md` from project root to cwd | Override file wins at each level; first non-empty file at the global level wins; deeper directories augment shallower |
| Cline / Roo Code | Primary workspace folder | VS Code extension storage | `.clinerules` file or `.clinerules/` directory in primary workspace; user-level rules via settings | Primary workspace only; non-primary roots in a multi-root workspace are silently ignored |
| Goose | cwd | Session files under config dir | `~/.config/goose/` (config, recipes) + `.goose/recipes/` project; `GOOSE_PATH_ROOT` overrides config dir | Project recipes coexist with global; explicit recipe path on invocation wins |

The recurring pattern is: **project root = cwd or git root**, **session root = global per-user store keyed by project**, **knowledge root = layered project + global with project winning on name collisions**. Cursor and Claude Code add a third tier (team or managed enterprise settings) that wins over both.

## Per-tool detail

### 1. Claude Code

Project root is the cwd at launch, with file-edit safety scoped to it. Session transcripts live under `~/.claude/projects/<hash-of-absolute-cwd>/sessions/<uuid>.jsonl`, never inside the project tree. Knowledge is split across two directories with the same shape: project `.claude/` and user `~/.claude/`, each containing `CLAUDE.md`, `agents/`, `commands/`, `skills/`, `hooks/`, `plugins/`, and `settings.json`. CLAUDE.md uses an additive hierarchy: enterprise managed > CLI flags > project root CLAUDE.md > nested directory CLAUDE.md (loaded when work touches that directory) > `~/.claude/CLAUDE.md`. For named artifacts such as `/deploy`, a skill at `.claude/skills/deploy/SKILL.md` beats a command file at `.claude/commands/deploy.md`. Project root, session root, and knowledge root are therefore three different paths, with knowledge layered project-then-user. ([Claude Code docs](https://code.claude.com/docs/en/claude-directory), [CLAUDE.md hierarchy](https://www.anthropiccertifications.com/learn/claude-code-workflows/claude-md-hierarchy), [Session storage](https://claude-world.com/tutorials/s16-session-storage/))

### 2. Cursor

Project root is the VS Code workspace folder (or folders, in multi-root). Chat history lives in SQLite databases inside Cursor's application support directory keyed by workspace path, not in the project, so renaming or moving the project orphans the history. Knowledge has three tiers: Team Rules (admin dashboard, enterprise plans), Project Rules (`.cursor/rules/*.mdc`, with nested subdirectories supported), and User Rules (Cursor Settings -> Rules). Documented precedence is **Team > Project > User**, applied as a merge with earlier sources winning on conflict. `AGENTS.md` is supported as a single-file alternative, and nested `AGENTS.md` files combine with parent directories using more-specific-wins. MCP servers come from `.cursor/mcp.json` (project) and `~/.cursor/mcp.json` (global); when the same server name appears in both, the project-level entry wins. ([Cursor Rules](https://cursor.com/docs/rules), [Rules hierarchy thread](https://forum.cursor.com/t/rules-hierarchy-in-cursor/108589), [Cursor MCP setup](https://www.morphllm.com/cursor-mcp-server), [Chat storage](https://forum.cursor.com/t/where-are-cursor-chats-stored/77295))

### 3. Aider

Project root is the git repo root. Aider is the rare tool that **persists session transcripts inside the project tree**: `.aider.chat.history.md` (and `.aider.input.history`) sit at the repo root by default. The path is overridable through `--chat-history-file` or `AIDER_CHAT_HISTORY_FILE`. Configuration loads from `.aider.conf.yml` searched in home, git root, then cwd, with later files overriding earlier values. Knowledge artifacts are minimal: `.aiderignore` for scope control and convention markdown files supplied via `--read`. The repo map is computed from the git repo root and bounded by the active token budget. Project root, session root, and config root effectively converge on the same git tree, which is unusual in this comparison. ([Aider options](https://aider.chat/docs/config/options.html), [YAML config](https://aider.chat/docs/config/aider_conf.html), [Repo map](https://aider.chat/docs/repomap.html))

### 4. Continue.dev

Project root is the IDE workspace. Session state is held in extension storage (not user-visible). Knowledge lives in two parallel directories with identical shape: project `.continue/` (rules, prompts, models, MCP, agents) and user `~/.continue/`. `config.yaml` takes precedence over `config.json` when both are present. Rules under `.continue/rules/` are loaded in lexicographic order, so numeric prefixes are the convention for ordering. Project and global configs are merged automatically rather than one strictly overriding the other, which differs from Cursor's "earlier wins" merge. ([Continue config reference](https://docs.continue.dev/reference), [Rules](https://docs.continue.dev/customize/deep-dives/rules), [Configuration deep-dive](https://docs.continue.dev/customize/deep-dives/configuration))

### 5. GitHub Copilot coding agent

The "workspace" is the repository the cloud agent is dispatched into; sessions are server-side and resumable through `/resume` (CLI) or the agents dashboard. `COPILOT_HOME` overrides the CLI home directory. Knowledge is in-repo: `.github/copilot-instructions.md` (repo-wide), `.github/instructions/*.instructions.md` (path-globbed), and `AGENTS.md` (added as a supported fallback in 2025). Path-specific instructions stack with the repo-wide file rather than replacing it. There is no documented user-global knowledge dir at the same tier as Claude Code or Cursor. ([Custom instructions](https://docs.github.com/copilot/customizing-copilot/adding-custom-instructions-for-github-copilot), [AGENTS.md support](https://github.blog/changelog/2025-08-28-copilot-coding-agent-now-supports-agents-md-custom-instructions/), [Copilot CLI sessions](https://code.visualstudio.com/docs/copilot/agents/copilot-cli))

### 6. Codex CLI (OpenAI)

Project root is the git root, with a walk down to cwd; if no git root exists, only cwd is consulted. Sessions are global: `~/.codex/sessions/YYYY/MM/DD/rollout-<uuid>.jsonl`. `codex resume --last` is scoped to the current working directory by default, which couples session lookup to project root for ergonomics without storing sessions in the project. Knowledge is the cleanest layered model in the survey. At the **global** level (`$CODEX_HOME`, default `~/.codex`), Codex picks the first non-empty file from `AGENTS.override.md` then `AGENTS.md`. At the **project** level, it walks from git root down to cwd, and at each directory chooses `AGENTS.override.md` then `AGENTS.md` then any `project_doc_fallback_filenames`. The override-file pattern provides explicit, name-preserving precedence rather than implicit merge order. ([Codex AGENTS.md](https://developers.openai.com/codex/guides/agents-md), [Config reference](https://developers.openai.com/codex/config-reference), [Sessions feature](https://developers.openai.com/codex/cli/features))

### 7. Cline / Roo Code

Project root is the **primary workspace** (first folder) in a VS Code multi-root workspace. This is the survey's biggest gotcha: `.clinerules` files or `.clinerules/` directories in non-primary workspace folders are silently ignored. There is an open enhancement request to make `.clinerules/` discovery per-folder, particularly for monorepos. Knowledge is the rules dir plus user-level rules in extension settings. Session state is held in VS Code extension storage. Roo Code (a Cline fork) takes the same architectural shape but biases toward developer autonomy over institutional rule enforcement. ([Cline multi-root issue 4642](https://github.com/cline/cline/issues/4642), [Cline multi-root docs](https://docs.cline.bot/features/multiroot-workspace), [Roo vs Cline](https://www.qodo.ai/blog/roo-code-vs-cline/))

### 8. Goose (Block)

Project root is cwd. Configuration lives at `~/.config/goose/config.yaml` (overridable via `GOOSE_PATH_ROOT`). Recipes (Goose's reusable workflow unit, equivalent to skills) are discovered in two places: `{config_dir}/recipes/` globally and `.goose/recipes/` in the project. Both sources coexist; an explicit path on invocation wins. Permissions and secrets each have their own files (`permission.yaml`, `secrets.yaml`, `permissions/tool_permissions.json`), keeping policy separate from knowledge. ([Goose configuration files](https://goose-docs.ai/docs/guides/config-files/), [Recipe reference](https://block.github.io/goose/docs/guides/recipes/recipe-reference/))

## Patterns and surprises

**Common precedence rule.** Every tool that supports project + global knowledge resolves conflicts as **project wins over global** for same-named artifacts. The variation is on what happens above project: Claude Code and Cursor add managed/team tiers that **outrank** project, while Continue.dev and Goose merge without an enterprise override.

**Session storage diverges from project root.** Claude Code, Codex, Cursor, Cline, Continue, and Copilot all keep transcripts outside the project tree. Aider is the outlier, storing `.aider.chat.history.md` at the repo root. The Claude Code community has open feature requests for project-local session storage (issues 9306, 12646), suggesting this is contested design territory.

**Override files instead of merge order.** Codex's `AGENTS.override.md` is the cleanest precedence primitive in the survey: an explicit "this beats the unsuffixed file at this level" marker, applied at both global and per-directory tiers. Most other tools rely on directory order or load-time lexicographic sort, which is more fragile.

**Multi-root is largely unsolved.** Cline only honors the primary workspace folder. Continue and Cursor support nested rule directories but practical multi-root semantics are sparsely documented. This matters for monorepos that mix backend and frontend conventions.

**Knowledge "shape" is converging.** AGENTS.md is now supported by Cursor, Codex, and Copilot as a portable cross-tool convention, even though each tool also retains its native dot-directory format. The shared shape: a project root with a dot-directory of layered sub-resources (rules, prompts, agents, MCP, settings), plus a parallel user-global directory of identical shape.
