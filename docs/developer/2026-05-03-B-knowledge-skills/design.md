# Knowledge Skills: Conversation-Level Skill Loading

## Problem Statement

Ailly's Conversation today threads a chain of plain-text system prompts down the directory hierarchy via `.ailly.toml` files. Skills, the unit of reusable agent guidance defined by [agentskills.io](https://agentskills.io/specification) v1.0, have no representation in the Conversation. Operators cannot say "this directory uses the `code-review` skill" and have its `SKILL.md` body reach the model.

This component adds Skill loading to the Conversation and routes Skill bodies into the Anthropic `system:` array as separate cacheable blocks. Concretely:

- A `.ailly.toml` may declare `skills = ["name-1", "name-2"]`.
- Each named Skill is loaded once from disk, parsed into a typed `Skill`, and attached to every `ConversationTurn` rooted in or below that directory.
- The Engine carries Skill bodies in a structured preamble alongside the inherited system text, not inline in `history`.
- The Anthropic Rig adapter bypasses `AgentBuilder` to construct `CompletionRequest` directly, so each Skill block can carry `cache_control: ephemeral`.

Anthropic's prefix cache is what makes Skill content economical at scale; injecting Skill bodies as plain `Message::System` entries (the path Ailly uses today) reaches the wire `system:` array but loses cache-block control because Rig 0.36 hard-codes `cache_control: None` on every `SystemContent::Text` it produces. This component is the Conversation half of fixing that.

Out of scope for this slice: model-driven activation, tier-1 discovery summaries, tier-3 bundled assets (scripts, references, additional markdown), user-global search paths, and `allowed-tools` enforcement. These are listed in Deferred Decisions.

## Prior Art

- **Anthropic Skills overview** ([platform.claude.com/docs/en/agents-and-tools/agent-skills/overview](https://platform.claude.com/docs/en/agents-and-tools/agent-skills/overview)) describes three-tier progressive disclosure. Tier 1 (frontmatter) lives in `system`; tier 2 (body) is loaded on demand by tool call in their reference orchestrator. This slice loads tier 2 unconditionally when a Skill is named, leaving tier-1-only and tier-3 paths for later.
- **Managed Agents Skills API** ([platform.claude.com/docs/en/managed-agents/skills.md](https://platform.claude.com/docs/en/managed-agents/skills.md)) attaches Skills at agent-create time and mounts them on the session container. Ailly is not a Managed Agent; it talks to `/v1/messages`, so the orchestration burden is application-side.
- **anthropics/skills reference bundles** ([github.com/anthropics/skills](https://github.com/anthropics/skills)) define the on-disk shape this component reads.
- **`docs/research/2026-05-03-A-skills-in-completions/`** contains the load-bearing finding: Rig's Anthropic provider extracts inline `Message::System` entries via `split_system_messages_from_history` and merges them into the wire `system:` array, but `cache_control` is hard-coded to `None` in both the `req.preamble` and `history_system` paths at version 0.36. To get per-block cache breakpoints, this design bypasses `AgentBuilder` for the Anthropic provider and constructs `CompletionRequest` directly.
- **In-tree precedent.** `AillyRc::load` ([src/content/mod.rs:594-661](../../../src/content/mod.rs#L594-L661)) already accumulates state down the directory walk with three parent modes (`Root`, `Always`, `Never`). Skill resolution piggybacks on the same walk.

## Metrics

- **Correctness.** A `.ailly.toml` with `skills = ["foo"]` produces a `ConversationTurn` whose preamble includes the body of `<root>/.ailly/skills/foo/SKILL.md`, placed between the inherited and local system text.
- **Laziness.** A `Conversation::load` over a tree with no `skills =` declarations performs zero `SKILL.md` reads. Verified via a VFS read counter or trace assertion.
- **Cache reach.** For the Anthropic engine, every Skill block in the assembled `CompletionRequest` carries `cache_control: ephemeral`. Unit test inspects the request before send.
- **Provenance in errors.** A missing or malformed Skill produces a `ContentError::Skill { name, source }` whose Display includes the search path attempted. Asserted in tests.
- **Dependency budget.** One new direct dependency (`serde_yml`) and one new internal crate-relative module (`src/content/skills/`). No `walkdir`. The `vfs` traversal already used by `Conversation::load` is reused.

## Specification

### Glossary additions

These terms are introduced by this slice and SHOULD be added to a project glossary when one exists. Definitions:

- **Skill.** A directory whose `SKILL.md` carries YAML frontmatter (`name`, `description`, optional fields) and a Markdown body, conforming to agentskills.io v1.0.
- **SkillName.** A validated newtype over `String`. Lowercase, 1-64 characters, equal to the directory name from which the Skill was loaded.
- **SkillDescription.** A validated newtype over `String`. 1-1024 characters.
- **SkillBody.** The Markdown body of a `SKILL.md`, after frontmatter is stripped.
- **SkillSource.** The absolute on-disk path of the loaded `SKILL.md`. Carried for error messages and for future tier-3 resolution of sibling files.
- **SkillRepository.** A port that resolves a `SkillName` to a parsed `Skill`. The first-slice implementation reads from `<project>/.ailly/skills/`.
- **PreambleBlock.** A typed entry in the Engine's structured preamble: either inherited or local system text, or a Skill body. Each block can carry an optional cache hint.

### Domain types (parse, don't validate)

```rust
// src/content/skills/types.rs

/// Validated Skill name. Lowercase, 1..=64 chars, equal to the directory name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SkillName(String);

/// Validated Skill description. 1..=1024 chars.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDescription(String);

/// The Markdown body of a SKILL.md, frontmatter stripped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillBody(String);

/// Absolute on-disk path of the loaded SKILL.md.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillSource(VfsPath);

/// A loaded, validated Skill. Construction is the proof of validity.
#[derive(Debug, Clone)]
pub struct Skill {
    pub name: SkillName,
    pub description: SkillDescription,
    pub body: SkillBody,
    pub source: SkillSource,
}
```

`SkillName::try_from(&str)`, `SkillDescription::try_from(&str)`, and `Skill::parse(source: SkillSource, raw: &str, expected_name: &SkillName) -> Result<Skill, SkillError>` are the parsers. They are the only sanctioned constructors. Domain code never sees an unvalidated `String` in any of these positions.

### SkillRepository (port)

```rust
// src/content/skills/repository.rs

pub trait SkillRepository: Send + Sync {
    fn get(&self, name: &SkillName) -> Result<Skill, SkillError>;
}

pub struct FsSkillRepository {
    root: VfsPath,  // <project>/.ailly/skills/
}

impl FsSkillRepository {
    /// Infallible. Stores the project root and the `.ailly/skills/` subdirectory
    /// path as a `VfsPath`. Absence of the directory is tolerated and surfaces
    /// only at the first `get` call.
    pub fn new(project_root: &VfsPath) -> Self {
        let root = project_root.join(".ailly/skills/").expect("static suffix");
        Self { root }
    }
}

impl SkillRepository for FsSkillRepository {
    fn get(&self, name: &SkillName) -> Result<Skill, SkillError> {
        let dir = self.root.join(name.as_str())?;
        if !dir.exists()? {
            return Err(SkillError::Missing {
                name: name.clone(),
                search_path: self.root.as_str().to_string(),
            });
        }
        let skill_md = dir.join("SKILL.md")?;
        let raw = skill_md.read_to_string()?;
        let source = SkillSource(skill_md);
        Skill::parse(source, &raw, name)  // expected_name = name
    }
}
```

The trait shape is deliberately narrow: `get` only. A `SkillCatalog` (list, search, prefix-match) is deferred until a consumer needs it.

### Errors

```rust
// src/content/skills/errors.rs

#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("skill `{name}` not found under {search_path}")]
    Missing { name: SkillName, search_path: String },

    #[error("skill `{expected}` declares name `{found}` in its frontmatter")]
    NameMismatch { expected: SkillName, found: String },

    #[error("skill `{name}` SKILL.md is missing required frontmatter field `{field}`")]
    FrontmatterMissing { name: SkillName, field: &'static str },

    #[error("skill `{name}` SKILL.md frontmatter field `{field}` is invalid: {reason}")]
    FrontmatterInvalid { name: SkillName, field: &'static str, reason: String },

    #[error("skill `{name}` SKILL.md failed to parse YAML frontmatter")]
    FrontmatterParse { name: SkillName, source: serde_yml::Error },

    #[error("skill `{name}` SKILL.md could not be read at {path}")]
    Read { name: SkillName, path: String, source: VfsError },

    #[error("invalid skill name `{raw}`: {reason}")]
    InvalidName { raw: String, reason: String },

    #[error("invalid skill description: {reason}")]
    InvalidDescription { reason: String },
}
```

`ContentError` gains:

```rust
#[error("failed to load skill `{name}` referenced in {origin}")]
Skill {
    name: SkillName,
    origin: String,           // the .ailly.toml path that referenced it
    #[source]
    source: SkillError,
},
```

### `.ailly.toml` activation

`AillyRcFile` gains an optional `skills` field:

```rust
struct AillyRcFile {
    // ...existing fields...
    #[serde(default)]
    skills: Option<Vec<String>>,
}
```

Activation rules:

- Absent or empty list: zero `SkillRepository::get` calls, zero `SKILL.md` reads.
- Non-empty list: each entry is parsed into a `SkillName` (failures bubble as `ContentError::Skill { ... source: SkillError::InvalidName }`), then resolved via `SkillRepository::get`.
- Inheritance follows the same `parent` mode the system chain uses. `parent = root` resets the inherited skills, `parent = always` keeps them and appends, and `parent = never` clears them.
- When a descendant `.ailly.toml` omits `skills =` entirely, the inherited skills set is preserved unchanged, gated by the same `parent` mode that gates `system` inheritance.
- Within one `.ailly.toml`, declaration order is preserved.

### `AillyRc` and `ConversationTurn` shape

```rust
pub struct AillyRc {
    pub system: Vec<Message>,
    pub skills: Vec<Skill>,        // NEW
    pub meta: ContentMeta,
}

pub struct ConversationTurn {
    path: VfsPath,
    meta: ContentMeta,
    system: Vec<Message>,
    skills: Vec<Skill>,            // NEW
    prompt: OneOrMany<Message>,
    response: Vec<TurnMessage>,
}
```

`ConversationTurn::load` gains a `skills: Vec<Skill>` parameter, threaded from `AillyRc::load`. The skills slice is frozen on the turn at load time, just like `system`.

### Preamble assembly: the new contract

`Conversation::history_for` is replaced by two callers-of-the-engine methods:

```rust
impl Conversation {
    pub fn preamble_for(&self, turn: &ConversationTurn) -> Preamble;
    pub fn messages_for(&self, turn: &ConversationTurn) -> Vec<Message>;
}

pub struct Preamble {
    pub blocks: Vec<PreambleBlock>,
}

pub enum PreambleBlock {
    InheritedSystem { text: String },
    Skill(Skill),
    LocalSystem { text: String },
}
```

Order of `Preamble.blocks` for a given turn (unless `meta.skip_head`):

1. Each inherited `Message::System` entry from ancestors above the turn's directory produces one `PreambleBlock::InheritedSystem { text }` block, in walk order. The collection is not collapsed into a single block.
2. All `Skill` blocks accumulated through the chain, in walk order then declaration order.
3. The `LocalSystem` text from the turn's own directory, if any.

This is the literal operational meaning of "skill body is injected between inherited and local system." Empty blocks are elided before emission.

`messages_for` returns the prior turns' prompts and responses (gated by `meta.isolated`) and the current turn's prompt. The trailing-assistant-drop rule (driven by `meta.continue`) stays as it is today.

### Engine trait change

```rust
pub trait Engine: Send + Sync {
    fn name(&self) -> &'static str;
    fn stream(
        &self,
        input: EngineInput,
        settings: &Settings,
        request_label: &str,
    ) -> anyhow::Result<EngineStream>;
}

pub struct EngineInput {
    pub preamble: Preamble,
    pub history: Vec<Message>,
}
```

Each engine flattens `preamble` according to what its provider can express:

- **`RigEngine` against Anthropic.** Bypasses `AgentBuilder`. Constructs `CompletionRequest` directly. Each `PreambleBlock` becomes a `SystemContent::Text { text, cache_control }` entry. `Skill` blocks (and the last `InheritedSystem` block, to anchor the prefix) carry `cache_control: ephemeral`. `LocalSystem` blocks carry `None`. The Anthropic Messages API allows up to four `cache_control: ephemeral` breakpoints per request (see [Anthropic prompt caching docs](https://docs.anthropic.com/en/docs/build-with-claude/prompt-caching)). The Anthropic adapter assigns `cache_control: ephemeral` to at most four blocks per request, sourced from the Anthropic per-request cap. Priority is: the final `InheritedSystem` block first, then `Skill` blocks in walk-then-declaration order until the cap is reached. Skill blocks beyond the cap, and all `LocalSystem` blocks, carry `cache_control: None`. Overflow handling is a unit test, not a runtime warning.
- **`RigEngine` against OpenAI/Bedrock.** Flattens all preamble blocks to a single string preamble passed to `AgentBuilder.preamble(...)`. `cache_control` is Anthropic-specific; for these providers the flag is dropped without warning. The Anthropic-specific code path lives behind a small trait the rig adapters share.
- **`Noop`.** Ignores `preamble.blocks`; uses only `history` and the noop's existing canned response.

This is the "parallel engine path" referenced in the option summary. It is parallel only inside the Anthropic-specific section of the rig adapter; the trait shape stays uniform.

### Bootstrap wiring

`build_engine` ([src/ailly/mod.rs](../../../src/ailly/mod.rs)) becomes the Composition Root for the SkillRepository as well. It constructs an `FsSkillRepository` rooted at `<project>/.ailly/skills/` (creating no directory; absence is tolerated and surfaces only when a Skill is referenced) and passes it to `Conversation::load`:

```rust
pub async fn load(
    path: VfsPath,
    skills: &dyn SkillRepository,
) -> Result<Self, ContentError> {
    // implementation threads `skills` through `load_into` and `AillyRc::load`.
}
```

#### Threading the repository

The repository is threaded read-only at every level: `Conversation::load(path, skills) -> load_into(path, prior, turns, skills) -> AillyRc::load(dir, prior, skills)`. `AillyRc::load` calls `skills.get(&name)` for each entry in the local `.ailly.toml`'s `skills =` list and accumulates the resulting `Skill` values into the `AillyRc.skills` field per the inheritance rules already specified.

The repository is borrowed for the duration of the load and dropped before the engine runs. Skills are owned by `ConversationTurn` thereafter.

### Frontmatter parsing

`Skill::parse(source, raw, expected_name)` does the following:

1. Split `raw` into a YAML frontmatter section (between `---` lines at the start) and a Markdown body. Absent frontmatter is `FrontmatterMissing { field: "name" }`.
2. `serde_yml::from_str::<RawFrontmatter>(yaml)` produces a deserializable struct with `name: String`, `description: String`, and an `#[serde(flatten)] extras: serde_yml::Value` for unrecognized fields.
3. `SkillName::try_from(&raw.name)` validates lowercase + length.
4. Compare the parsed `SkillName` to `expected_name`. Mismatch is `NameMismatch`.
5. `SkillDescription::try_from(&raw.description)` validates length.
6. The body is whatever follows the second `---`, with one leading newline trimmed. No further validation.
7. `extras` is dropped in this slice. (Future: `allowed-tools`, `license`, `compatibility`, `metadata`.)

### File and module layout

```
src/content/
  mod.rs                   (existing; AillyRc/ConversationTurn extended)
  skills/
    mod.rs                 (re-exports)
    types.rs               (SkillName, SkillDescription, SkillBody, SkillSource, Skill, parsers)
    repository.rs          (SkillRepository trait, FsSkillRepository)
    errors.rs              (SkillError)
src/engine/
  mod.rs                   (Engine trait gains EngineInput; Preamble re-exported)
  rig_engine.rs            (Anthropic path constructs CompletionRequest directly)
  noop.rs                  (consumes EngineInput, ignores preamble)
```

### Test plan

Each test name maps to one observable behavior. Failures should localise.

**Loader unit tests (`src/content/skills/types.rs`, `repository.rs`):**

- `skill_name_accepts_lowercase_alphanumeric_dash`
- `skill_name_rejects_uppercase`
- `skill_name_rejects_empty`
- `skill_name_rejects_over_64_chars`
- `skill_description_rejects_empty`
- `skill_description_rejects_over_1024_chars`
- `parse_strips_frontmatter_and_returns_body`
- `parse_rejects_missing_name_field`
- `parse_rejects_missing_description_field`
- `parse_rejects_name_mismatch_with_directory`
- `parse_rejects_invalid_yaml`
- `repository_get_returns_skill_for_existing_directory`
- `repository_get_missing_includes_search_path_in_error`

**Conversation integration tests (`src/content/mod.rs`):**

- `skill_body_is_injected_between_inherited_and_local_system` (the named-in-TASKS test, kept verbatim)
- `multi_skill_ordering_preserves_declaration_then_walk_order`
- `parent_root_resets_inherited_skills`
- `parent_never_clears_skills`
- `parent_always_appends_to_inherited_skills`
- `no_skills_declared_performs_zero_skill_md_reads` (VFS read counter)
- `missing_skill_surfaces_origin_aillyrc_path_in_error`

**Engine integration tests (`src/engine/rig_engine.rs`):**

- `anthropic_request_carries_cache_control_ephemeral_on_skill_blocks`
- `anthropic_request_carries_cache_control_on_at_most_four_blocks`
- `openai_request_flattens_preamble_to_single_string`
- `noop_engine_ignores_preamble`

The engine tests inspect the constructed `CompletionRequest` before send. They do not require a network call.

### Migration

This slice changes two public surfaces inside the crate:

1. `Conversation::history_for` is removed. Callers move to `preamble_for` + `messages_for`. The single in-tree caller is `Generator` ([src/engine/generator.rs](../../../src/engine/generator.rs)); it must be updated in the same change.
2. `Engine::stream` signature changes from `(history, settings, label)` to `(input, settings, label)` where `input.history` is the messages and `input.preamble` is the structured preamble.

`Noop` and `RigEngine` are updated. The CLI surface and `.aillyrc` files of existing projects are unaffected unless they add `skills =`.

## Alternatives

### Keep the existing inline `Message::System` route (Option 1 from the brainstorm)

Skill bodies become extra `Message::System` entries spliced into the `Vec<Message>` between inherited and local system text. Rig's Anthropic provider lifts them into `system:` automatically.

- Pros: minimal diff; no engine signature change; no parallel engine path.
- Cons: `cache_control` is `None` on every block. For Skill content, which is exactly the kind of stable, repeated-across-turns text Anthropic's prefix cache is designed for, this is the load-bearing missing feature. Rejected on the grounds that it defers the only thing that makes Skill loading economical.

### Synthetic ancestor `AillyRc` per Skill (Option 3 from the brainstorm)

Each declared Skill is treated as if it were a synthetic `.ailly.toml` ancestor with `parent = always` and `system = <body>`.

- Pros: zero new fields on `ConversationTurn`; reuses the inheritance machinery wholesale.
- Cons: erases Skill provenance. Errors degrade from "skill `foo` failed to load" to "system text at offset N failed to load." Forecloses tiered loading (the future tier-1-only summary case has no place to live). Rejected on the grounds that identity is what makes the next slice possible.

### Patch or fork Rig to expose `cache_control`

Submit upstream patches to `rig-core` exposing `cache_control` on `SystemContent::Text` through the `AgentBuilder` API.

- Pros: removes the rationale for bypassing `AgentBuilder` at all; aligns this design with how the rest of the codebase uses Rig.
- Cons: depends on upstream merge cadence; even if accepted, it ships in a future Rig version, and Ailly is currently pinned to 0.36. The bypass is mechanical (CompletionRequest is a public struct) and contained to one file. Not rejected outright; this design adopts the bypass now and recommends an upstream patch as a parallel non-blocking effort tracked elsewhere.

### Off-the-shelf Skills loader crate

There is no `agent-skills` crate at the moment. The Anthropic SDK Python bindings include orchestration helpers but not a Rust loader. A hand-rolled loader on `vfs` plus `serde_yml` is the simplest path.

## Summary

This slice gives the Conversation a typed, lazily-loaded representation of agentskills.io v1.0 Skills, threads them through the directory walk alongside system text, and lets the Engine emit them as Anthropic system blocks with `cache_control: ephemeral`. The new types are narrow and parser-validated; the engine trait change is small but unavoidable for cache-control reach.

### Deferred Decisions

The following questions are out of scope for this slice and should re-enter as their own TASKS entries when their motivating slice ships:

1. **User-global search paths.** `~/.ailly/skills/` and `~/.claude/skills/` (in that precedence order under `<project>/.ailly/skills/`) are deferred. Multi-source resolution rules and conflict handling open with that work.
2. **Multi-ancestor dedup.** When the same Skill name appears in `parent = always` chains at multiple levels, this slice loads it once per `.ailly.toml` declaration. A canonicalising pass that dedupes by `SkillName` is deferred.
3. **Body token cap.** agentskills.io recommends a 5000-token tier-2 cap; this slice loads any size. A loader-time warning or hard cap is deferred.
4. **`serde_yml` vs `serde_yaml_ng`.** This slice picks `serde_yml`. A formal evaluation against `serde_yaml_ng` (maintenance health, surface area, license) is deferred.
5. **`SkillCatalog`.** A list/search/prefix surface on top of `SkillRepository` is deferred until a consumer (CLI listing, completion-by-name UI, model-driven activation) exists.
6. **Model-driven activation.** Letting the model emit a tool call to load a Skill body on demand is deferred. This slice loads tier 2 unconditionally on declaration.
7. **Tier 1 discovery summary.** Folding `name + description` of every Skill in the search paths into the system chain (regardless of whether a `.ailly.toml` references it) is deferred.
8. **Tier 3 bundled assets.** `scripts/`, `references/`, `assets/` resolution and access policy is deferred. The `SkillSource` field is preserved precisely to make this addable without changing the type.
9. **`allowed-tools` enforcement.** The frontmatter field is parsed but ignored in this slice. Composition with the Engine's "reads safe, writes dangerous" permission model is deferred.
10. **Optional frontmatter fields.** `license`, `compatibility`, `metadata` are dropped in this slice. They will reappear when a consumer requires them.
11. **Smarter cache-block budget heuristic.** The in-slice heuristic is the simple priority rule specified above (final `InheritedSystem` block first, then `Skill` blocks in walk-then-declaration order until the four-breakpoint cap is reached). Deferred work is to evaluate alternatives such as LRU by hit-rate, longest-skill-first selection, and hit-rate telemetry to inform the choice.
12. **Upstream Rig patch.** Submitting `cache_control` exposure to `rig-core` is a parallel effort tracked outside this slice; the Anthropic-bypass code path remains the in-tree fix.
