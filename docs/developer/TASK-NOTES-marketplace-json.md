# TASK-NOTES: marketplace.json multi-plugin knowledge loading

## Goal

Load skills, tools, and workflows (collectively, knowledge) from a single
`marketplace.json` manifest that aggregates several plugins in one declaration,
so a project can consume a curated bundle without configuring each
`KnowledgeRoot` by hand.

## Why

Today a user composes knowledge by pointing `--knowledge` at one directory
per source (project root, `~/.claude`, `~/.ailly`, an org-shared bundle).
A plugin is just a directory laid out as `skills/<name>/SKILL.md`,
`workflows/<name>.toml`, etc. There is no way to say "load these five
plugins from this one descriptor file."

A `marketplace.json` manifest names plugins by source (filesystem path,
git URL, tarball URL, packaged bundle) so a project's `.ailly.toml` can
reference a single manifest and the runtime resolves every plugin behind
it at startup.

## Open design questions

- Manifest schema. Minimum fields: a list of plugins, each with a name,
  source URI, optional version constraint, and optional fetch policy
  (refresh interval, integrity hash). What is the smallest viable v1?
- Plugin source kinds. Filesystem (relative or absolute), git, tarball,
  HTTP-served pack, in-tree dev override. Pick one or two for the first
  slice; the rest land behind a `PluginSource` trait once the first is real.
- Cache and resolution. Where does a remote plugin land on disk?
  `~/.ailly/cache/marketplace/<hash>/`? Co-design with the user-global
  knowledge roots task (`~/.ailly/`, `~/.claude/`) already on the SKILLS
  list.
- Layering with `--knowledge`. Does a marketplace plugin become a
  synthetic `KnowledgeRoot` appended to `project.knowledge`, or a new
  `KnowledgeBase` impl that wraps several roots? The trait surface
  shaped in 2026-05-06-A-project-roots admits both.
- First-wins precedence. Today: project root > each `--knowledge` in
  argument order. A marketplace plugin is also a knowledge source.
  Where does it sit in the precedence order? Likely after `--knowledge`
  flags so explicit operator wiring still wins, but confirm with the
  `Conversation::load` exclusion-list-as-configuration task.
- Trust and integrity. Tarball/git fetches need integrity verification
  (sha256 hash in the manifest). Untrusted code should not run on first
  install; the manifest declares the trust posture.
- Lockfile vs manifest. Does the manifest pin exact versions, or do we
  need a sibling `marketplace.lock`? Mirrors the npm/cargo split.
- CLI surface. `ailly --refresh-marketplace`? `ailly marketplace pull`?
  Co-design with the subcommand-split deferred decision from the
  2026-05-06 listing slice.

## Touch points (file-level guesses, verify when implementing)

- `src/project.rs` — `Project::knowledge` already typed as
  `Vec<KnowledgeRoot>`. A marketplace plugin can register additional
  `KnowledgeRoot`s.
- `src/knowledge/base.rs` — `FsKnowledgeBase` already iterates roots.
  The marketplace would feed it more roots.
- `src/ailly/args.rs` — new `--marketplace <PATH>` flag and
  `.ailly.toml [marketplace]` table.
- New module `src/marketplace/` — manifest parsing, plugin resolution,
  cache management.

## Why deferred

This is a substantial direction (manifest design, fetch policy, trust
model) that should not be wedged into the listing slice. Land it after:

1. The 2026-05-06-A-project-roots slice's `~/.ailly/` and `~/.claude/`
   user-global knowledge roots ship (TASKS.md line ~95). Marketplace
   plugins extend that surface; build it after the surface exists.
2. The `KnowledgeBase` consolidation slice in
   `TASK-NOTES-knowledge-base-consolidation.md` lands. Marketplace is a
   second `KnowledgeBase` impl; the consolidated trait should be in
   place first.
