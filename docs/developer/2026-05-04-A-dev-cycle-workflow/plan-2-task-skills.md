# Implementation Plan: `Task::skills`

**Feature test:** `src/workflow/runtime.rs` `mod tests` `task_skills_propagate_into_synthesized_turn_toml_in_declared_order`
**Feature test doc:** `docs/developer/2026-05-04-A-dev-cycle-workflow/feature-test-2-task-skills.md`
**User story:** A workflow task declaring `skills = [...]` produces a synthesized turn TOML carrying that field in declared order, round-trips through `Conversation::single_turn`, and surfaces unknown names as a load-time content error.

**Steps:**
- [ ] Step 1: `Task::skills` schema field
- [ ] Step 2: Per-turn TOML carries `skills` and `ConversationTurn` round-trips it
- [ ] Step 3: Walker resolves per-turn declared skills via `SkillRepository`
- [ ] Step 4: Runtime threads `task.skills` through `synthesize_turn_file`

## Step 1: `Task::skills` schema field

**Enables:** Schema construction inside the feature test (the test builds `Task { skills: vec![...], ... }` literals, so without the field the test fails to compile, which is the first form of red).

Add the field to `src/workflow/schema.rs`:

```rust
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Task {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
    pub task: TaskAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evaluation: Option<TaskAction>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub next: BTreeMap<String, String>,
}
```

Update every `Task { ... }` literal in `src/workflow/runtime.rs` `mod tests` and the `task(...)` helper at `runtime.rs:457` so existing tests still compile. Add a focused TOML round-trip test in `src/workflow/schema.rs` covering omission of the field when empty and preservation of declared order when populated.

`cargo build` and `cargo test` stay green. The feature test still fails because `synthesize_turn_file` does not yet write the skills.

## Step 2: Per-turn TOML carries `skills` and `ConversationTurn` round-trips it

**Enables:** The `assert!(!bare_text.contains("skills"))` and the substring-order assertions on `design_text`, plus the `Conversation::single_turn` round-trip assertion that returns the two names in order.

In `src/content/mod.rs`:

- Add `skills: Vec<String>` to `ConversationTurnFile` with `#[serde(default, skip_serializing_if = "Vec::is_empty")]`, placed beside `tools`.
- Add a `declared_skills: Vec<String>` field to `ConversationTurn` mirroring the `declared_tools` pattern (`mod.rs:252`). It captures the per-turn literal exactly as authored so `write` reproduces it byte-stably.
- Extend `ConversationTurn::new` (`mod.rs:329`) to accept declared skill names. Pick the simplest signature change: add a parameter so callers must pass a slice/`Vec<String>` (existing call sites already exist in `synthesize_turn_file` at `runtime.rs:411` and in tests).
- Populate `declared_skills` from `file.skills` inside `ConversationTurn::load` (`mod.rs:272`).
- Serialize `declared_skills` back into `ConversationTurnFile { skills: self.declared_skills.clone(), .. }` inside `ConversationTurn::write` (`mod.rs:387`).
- Expose a `pub fn declared_skills(&self) -> &[String]` accessor next to `tool_names`.

Update the feature-test assertion to match the chosen accessor surface. The feature-test note explicitly permits adjusting the accessor expression `turns().last().skills()`. The new expression is:

```rust
let convo = Conversation::single_turn(design_path).await.unwrap();
let resolved: Vec<String> = convo.turn(0).declared_skills().to_vec();
```

Adjust every `ConversationTurn::new` call site (test fixtures in `src/content/mod.rs` `mod tests` and `synthesize_turn_file` in `src/workflow/runtime.rs`) to pass the new slice. `synthesize_turn_file` continues to pass an empty slice in this step; that is wired up in step 4.

Add a focused content-layer test that writes a turn with declared skills, reloads it via `ConversationTurn::load`, and asserts byte-equality of the on-disk file plus order preservation through `declared_skills()`.

`cargo build` and `cargo test` stay green. The feature test still fails because the runtime does not yet pass `task.skills` into the synthesizer.

## Step 3: Walker resolves per-turn declared skills via `SkillRepository`

**Enables:** Acceptance criterion 4 ("Unknown skill names surface through the engine's existing skill-resolution failure mode"). Not exercised by the feature test's `single_turn` path but required by the slice's scope.

Inside `Conversation::load_into` (`mod.rs:740`), after `ConversationTurn::load` returns, resolve each name in the freshly loaded `turn.declared_skills` against `skills_repo` using the same `SkillName::try_from` plus `skills_repo.load(...)` pattern as `AillyRc::load` at `mod.rs:1064-1076`. Append the resolved `Skill` values to the existing `acc.skills`-derived `turn.skills`, preserving walk-then-declaration order.

Surface unknown names through `ContentError::Skill { name, source }`, the same variant `AillyRc::load` already uses, so the error path is identical between `.ailly.toml`-declared and turn-declared skills.

Add a focused test in `src/content/mod.rs` `mod tests`: a turn file declaring an unknown skill loaded via `Conversation::load` against a `FsSkillRepository` over an empty skills tree fails with `ContentError::Skill`, and the same turn loaded via `Conversation::single_turn` succeeds (single_turn does not resolve, only carries `declared_skills`).

`cargo build` and `cargo test` stay green. The feature test still fails because the runtime does not yet pass `task.skills` through.

## Step 4: Runtime threads `task.skills` through `synthesize_turn_file`

**Enables:** All four feature-test assertions: the `design_text.find("skills")` substring, the declared-order check between `developer:design` and `developer:thinking`, the `Conversation::single_turn` round-trip producing the two names, and the `!bare_text.contains("skills")` omission for the empty-slice task.

Modify `synthesize_turn_file` in `src/workflow/runtime.rs:394` to accept a `skills: &[String]` slice and forward it into `ConversationTurn::new`. Update both call sites inside `Runtime::run`:

- `runtime.rs:138` (main task turn): pass `&task.skills`.
- `runtime.rs:204` (evaluation turn): pass `&[]`. The evaluation prompt is a separate synthesized file; the slice intentionally does not propagate to it because the evaluator is a self-contained `fs.absent` style check, not a development-cycle step in its own right.

The feature test now passes. `cargo build` and `cargo test` stay green; `cargo test --features bedrock` and `./e2e/e2e.sh` stay green.

## Out of scope for this slice

- Wiring per-turn skills into the runtime's direct `engine.stream` evaluation path. The eval-via-`Generator::run` migration is already a tracked TASKS entry tied to the deferred Generator overwrite filter; per-turn skill resolution will travel with that migration.
- Cache-control routing for `PreambleBlock::Skill` in `RigEngine` (already on `TASKS.md`).
- User-global skill search paths and multi-ancestor dedup (already on `TASKS.md`).
