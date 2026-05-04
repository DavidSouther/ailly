# Implementation Plan: Knowledge Skills, Conversation-Level Skill Loading

**Feature test:** [src/content/mod.rs](../../../src/content/mod.rs) `skill_body_is_injected_between_inherited_and_local_system`, plus [e2e/07_skills/skills.sh](../../../e2e/07_skills/skills.sh).

**User story:** An operator declares `skills = ["foo"]` in a `.ailly.toml`, drops `SKILL.md` under `<root>/.ailly/skills/foo/`, and the resulting `ConversationTurn` exposes a preamble whose blocks are ordered `InheritedSystem`, `Skill`, `LocalSystem`.

**Steps:**

- [ ] Step 0: Domain model (skill newtypes, `Skill` struct, `SkillError`, parser)
- [ ] Step 1: `SkillRepository` port and `FsSkillRepository` adapter
- [ ] Step 2: Thread skills through `.ailly.toml`, `AillyRc`, `ConversationTurn`, and `Conversation::load`
- [ ] Step 3: `Preamble` assembly via `preamble_for` and `messages_for`
- [ ] Step 4: `Engine::stream` consumes `EngineInput`; `Noop` renders preamble blocks

The crate-level feature test passes at the end of Step 3. The e2e fixture passes at the end of Step 4.

## Step 0: Domain model

**Enables:** Compilation of the test imports `crate::knowledge::skills::FsSkillRepository`, `PreambleBlock::Skill(skill)`, and the assertions `skill.name.as_str() == "foo"` and `skill.body.as_str() == "FOO BODY"`.

Introduce a new module `src/content/skills/` containing parser-validated newtypes plus the `Skill` aggregate. Construction is the proof of validity.

Files:

- `src/content/skills/mod.rs` (re-exports)
- `src/content/skills/types.rs`
- `src/content/skills/errors.rs`

Types:

- `SkillName(String)` — newtype, lowercase, 1..=64 chars; `try_from(&str) -> Result<Self, SkillError>`; `as_str(&self) -> &str`.
- `SkillDescription(String)` — newtype, 1..=1024 chars; same parser shape.
- `SkillBody(String)` — newtype wrapping the post-frontmatter body; `as_str(&self) -> &str`.
- `SkillSource(VfsPath)` — newtype; carried only for error provenance and future tier-3 work.
- `Skill { name, description, body, source }` — aggregate; constructed only by `Skill::parse(source: SkillSource, raw: &str, expected_name: &SkillName) -> Result<Skill, SkillError>`.
- `SkillError` — `thiserror` enum with the variants enumerated in the design (`Missing`, `NameMismatch`, `FrontmatterMissing`, `FrontmatterInvalid`, `FrontmatterParse`, `Read`, `InvalidName`, `InvalidDescription`).

Parser sketch:

```rust
impl Skill {
    pub fn parse(
        source: SkillSource,
        raw: &str,
        expected_name: &SkillName,
    ) -> Result<Skill, SkillError>;
}
```

Add `serde_yml` to `Cargo.toml` as the only new direct dependency.

No callers of these types exist yet. The crate compiles and all existing tests pass.

## Step 1: `SkillRepository` port and `FsSkillRepository` adapter

**Enables:** Compilation of `let skills = FsSkillRepository::new(&project_root);` in the feature test, and Phase 2 of the e2e fixture (the missing-skill error includes both the skill name and the `.ailly/skills` search path in stderr).

Files:

- `src/content/skills/repository.rs`
- `src/content/mod.rs` (extend `ContentError`)

Trait shape:

```rust
pub trait SkillRepository: Send + Sync {
    fn get(&self, name: &SkillName) -> Result<Skill, SkillError>;
}

pub struct FsSkillRepository { root: VfsPath }

impl FsSkillRepository {
    pub fn new(project_root: &VfsPath) -> Self;
}
```

`FsSkillRepository::new` is infallible. Absence of `<root>/.ailly/skills/` is tolerated until first `get`. `get` reads `<root>/.ailly/skills/<name>/SKILL.md`, builds a `SkillSource`, and delegates to `Skill::parse`.

Extend `ContentError` with:

```rust
#[error("failed to load skill `{name}` referenced in {origin}")]
Skill { name: SkillName, origin: String, #[source] source: SkillError },
```

The repository has no callers yet. Existing tests pass; the crate compiles.

## Step 2: Thread skills through the load pipeline

**Enables:** `Conversation::load(project_root, &skills).await.unwrap()` compiles with the new second parameter, and the `foo` skill is resolved from disk and stored on the child turn.

Files:

- `src/content/mod.rs` (`AillyRcFile`, `AillyRc`, `ConversationTurn`, `Conversation::load`, `Conversation::load_into`)
- `src/ailly/mod.rs` (Composition Root in `build_engine`/load path)
- All in-tree call sites of `Conversation::load` (test harness + `Generator` test fixtures)

`AillyRcFile` gains `#[serde(default)] skills: Option<Vec<String>>`.

`AillyRc` and `ConversationTurn` each gain `skills: Vec<Skill>`. `ConversationTurn::load` takes the skills vector alongside the existing system vector.

`Conversation::load` signature becomes:

```rust
pub async fn load(
    path: VfsPath,
    skills: &dyn SkillRepository,
) -> Result<Self, ContentError>;
```

`load_into` and `AillyRc::load` thread the borrowed `&dyn SkillRepository` down. `AillyRc::load` resolves each entry in the local `skills =` list via `skills.get(...)`, mapping `SkillError` into `ContentError::Skill { name, origin: <.ailly.toml path>, source }`. Inheritance follows the same `parent` mode rules already implemented for `system`.

Bootstrap wiring in `src/ailly/mod.rs` constructs `FsSkillRepository::new(&vfs_root)` immediately before `Conversation::load`, then borrows it across the load call.

Skills are stored on the turn but not yet rendered. `history_for` remains untouched and continues to satisfy the existing `history_for_*` test set, so the crate stays runnable.

## Step 3: `Preamble` assembly via `preamble_for` and `messages_for`

**Enables:** The crate-level feature test `skill_body_is_injected_between_inherited_and_local_system` reaches its three `assert_eq!` blocks and passes. The kinds vector equals `["inherited", "skill", "local"]`; `text == "INHERITED"`, `skill.name.as_str() == "foo"`, `skill.body.as_str() == "FOO BODY"`, `text == "LOCAL"`.

Files:

- `src/content/mod.rs` (add `Preamble`, `PreambleBlock`, `Conversation::preamble_for`, `Conversation::messages_for`)

Types:

```rust
pub struct Preamble { pub blocks: Vec<PreambleBlock> }

pub enum PreambleBlock {
    InheritedSystem { text: String },
    Skill(Skill),
    LocalSystem { text: String },
}
```

Assembly rule (matches the design's spec):

1. Each ancestor `Message::System` becomes one `PreambleBlock::InheritedSystem { text }`, in walk order. Inherited blocks are not collapsed.
2. All accumulated `Skill` values become `PreambleBlock::Skill(skill)`, in walk-then-declaration order.
3. The local `Message::System` (the one originating in the turn's own directory) becomes `PreambleBlock::LocalSystem { text }`.

Empty blocks are elided. `meta.skip_head` suppresses the entire preamble exactly as it suppresses inherited system text today.

`messages_for(&turn) -> Vec<Message>` returns prior turns' prompts and responses (gated by `meta.isolated`) plus the current turn's prompt, applying the trailing-assistant-drop rule from `meta.continue`. This is the moral half of `history_for` minus the system entries.

`history_for` stays in place during this step so the existing `history_for_*` tests keep passing. (Removing it is a follow-up tracked in the design's migration section, not blocking the feature test.)

## Step 4: Engine consumes `EngineInput`; `Noop` renders preamble blocks

**Enables:** The e2e fixture `e2e/07_skills/skills.sh` Phase 1 assertion `assert_grep_q 'SKILL_BODY_MARKER_ECHO_42' 01_skill.toml` passes alongside the inherited-system marker.

Files:

- `src/engine/mod.rs` (`Engine` trait, `EngineInput`)
- `src/engine/noop.rs`
- `src/engine/rig_engine.rs`
- `src/engine/generator.rs` (caller; switch from `history_for` to `preamble_for` + `messages_for`)

Trait shape:

```rust
pub struct EngineInput {
    pub preamble: Preamble,
    pub history: Vec<Message>,
}

pub trait Engine: Send + Sync {
    fn name(&self) -> &'static str;
    fn stream(
        &self,
        input: EngineInput,
        settings: &Settings,
        request_label: &str,
    ) -> anyhow::Result<EngineStream>;
}
```

`Generator` now calls `let preamble = self.conversation.preamble_for(turn);` and `let history = self.conversation.messages_for(turn);` and passes both via `EngineInput`.

`Noop` flattens `input.preamble` into the same rendered envelope it already emits for system text, so the e2e fixture observes both `'You are running the skills integration test.'` and `SKILL_BODY_MARKER_ECHO_42` in the recorded response. The exact rendering shape is whatever keeps `e2e/05_conversation/` green; the constraint is that every preamble block's text appears in the noop output, in order.

`RigEngine` flattens `input.preamble` into a single string preamble passed to `AgentBuilder.preamble(...)` for every provider in this step. `cache_control: ephemeral` routing for the Anthropic path is a separate slice; this step only restores parity.

After this step, `cargo test`, `cargo test --features bedrock`, and `./e2e/e2e.sh` are all green. The feature test is fully passing across both artifacts.
