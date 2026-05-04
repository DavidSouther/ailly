# Feature Test: Knowledge Skills, Conversation-Level Skill Loading

## User Story

**Given** a project root that contains a Skill on disk at
`<root>/.ailly/skills/foo/SKILL.md` (with frontmatter `name: foo`,
`description: a foo skill`, and body `FOO BODY`), an ancestor
`.ailly.toml` at `<root>/parent/.ailly.toml` whose `system = "INHERITED"`,
and a turn directory `<root>/parent/child/` whose `.ailly.toml` declares
both `system = "LOCAL"` and `skills = ["foo"]`,

**When** the operator loads the conversation rooted at `<root>` against an
`FsSkillRepository` rooted at `<root>` and asks for the preamble of the
single resulting turn,

**Then** the preamble's blocks are exactly three, ordered
`InheritedSystem("INHERITED")`, `Skill(foo, body="FOO BODY")`,
`LocalSystem("LOCAL")`. The skill body sits between the ancestor system
text and the turn-directory system text, and the `Skill` carries its
typed `name` and `body` accessors so a downstream Engine can route it
into a cacheable system block.

## Why this story

This is the headline acceptance criterion enumerated by the design's
Test Plan as `skill_body_is_injected_between_inherited_and_local_system`.
It exercises the load path end-to-end (filesystem walk, `.ailly.toml`
parsing, `SkillRepository::get`, `SKILL.md` frontmatter parsing, body
extraction) and the new preamble assembly contract that replaces
`Conversation::history_for`. Every later test in the design's plan
narrows in on a sub-rule (multi-skill ordering, parent-mode behaviors,
laziness, error provenance, cache-control routing) that this single
feature test does not by itself prove.

## Test Location

The user story is encoded by two artifacts that pin the same behavior
at two layers:

- **Crate-level feature test.** Lives in the existing tests module of
  [src/content/mod.rs](../../../src/content/mod.rs), beside the
  `history_for_*` unit tests, under the name
  `skill_body_is_injected_between_inherited_and_local_system`. It
  exercises the `Conversation::load` plus `preamble_for` contract
  directly against an in-memory VFS and asserts the three-block order.
- **CLI e2e fixture.** Lives at
  [e2e/07_skills/](../../../e2e/07_skills/) and runs as part of
  `e2e/e2e.sh` against the noop engine. Phase 1 declares
  `skills = ["echo"]` plus a `SKILL.md` carrying the unique marker
  `SKILL_BODY_MARKER_ECHO_42`, runs `ailly --root .`, and greps the
  recorded `01_skill.toml` for both the inherited system text and the
  skill body marker, proving the skill body reached the engine
  alongside the inherited system block. Phase 2 swaps in
  `skills = ["does-not-exist"]` and asserts the run exits non-zero
  with both the missing skill name and the `.ailly/skills` search
  path present in stderr, exercising the
  `ContentError::Skill { name, source }` provenance metric.

Phase 1 of the e2e relies on the noop engine surfacing preamble blocks
in its rendered envelope the same way it surfaces inline system
messages today. The slice's implementation must preserve that
visibility so the existing
[e2e/05_conversation/](../../../e2e/05_conversation/) assertion
(`'You are running an integration test.'` in the noop output) does
not regress when system text moves from `history` into `Preamble`.

## Expected Failure Modes Today

The test is intended to fail at compile time before any implementation
exists, because the following symbols are not yet present in the crate:

- `crate::knowledge::skills` module, and within it `FsSkillRepository`,
  `SkillRepository`, `Skill`, `SkillName`, `SkillBody`.
- `crate::content::Preamble` and `crate::content::PreambleBlock` (with
  variants `InheritedSystem { text }`, `Skill(Skill)`, and
  `LocalSystem { text }`).
- `Conversation::load(VfsPath, &dyn SkillRepository)` (the second
  parameter is new).
- `Conversation::preamble_for(&ConversationTurn) -> Preamble`.
- Accessor methods `SkillName::as_str` and `SkillBody::as_str` on the
  validated newtypes.

## Out of Scope For This Test

- Cache-control routing into the Anthropic `CompletionRequest` (covered
  by the engine integration tests in the design).
- Multi-skill ordering, parent-mode interactions, laziness counters,
  and error provenance assertions (each is a focused unit test in the
  design's Test Plan).
- The `messages_for` half of the new contract (asserted indirectly by
  preserving the existing `history_for_*` test set during migration).

## Follow-up Tracked

The TASKS file already carries the slice's final refactoring and review
pass entry: "Final refactoring and review pass for the Agent Skills
(Knowledge subsystem) first slice once
`docs/developer/2026-05-03-B-knowledge-skills/` reaches green." That
entry covers the verification checklist this feature test does not by
itself prove (focused unit tests, VFS read counters, error message
provenance, dependency budget).
