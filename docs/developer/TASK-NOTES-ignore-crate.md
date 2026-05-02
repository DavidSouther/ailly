# TASK-NOTES: Evaluate the `ignore` crate for `gitignore_fs`

## Context

The current implementation ships an in-tree gitignore matcher.

- Location: `src/content/gitignore_fs.rs`, items `GitignoreParser`, `Rule`, `glob_match`.
- Scope: per-line parse, `!` negation, trailing `/` for directory-only, `*` and `?` glob. No `**`, no character classes, no anchored-path semantics beyond stripping leading `/`.
- Call site: `GitignoreFs::keep_entry` passes the basename (with a trailing `/` for directories) to `GitignoreParser::accepts`. Ancestor `.gitignore` files are collected by `GitignoreFs::collect_gitignores`.

The TypeScript original used the `gitignore-parser` npm package, called the same way, basename in. Source: `ailly_typescript/core/src/content/gitignore_fs.ts`.

## Question

Should the in-tree matcher be replaced by the `ignore` crate?

Reference: https://docs.rs/ignore/latest/ignore/ (crate documentation, not yet read in full).

## What to investigate

1. **API fit.** `ignore::gitignore::Gitignore` and `GitignoreBuilder` expect a real-filesystem root path and match against paths relative to that root. Confirm whether they can be driven with synthetic `&str` paths from the VFS, or whether the crate insists on `std::path::Path` against the live filesystem.
2. **Bare-pattern semantics.** Verify that a pattern like `skip` in `/.gitignore` matches `dir/skip` when matched at the root, since the existing tests rely on that behavior.
3. **Per-directory composition.** The current code stacks parsers from `/`, `/dir`, `/dir/deep`. Confirm the crate composes that way without re-walking the disk.
4. **Dependency weight.** `ignore` pulls `regex`, `globset`, `walkdir`, `crossbeam-*`. Compare the binary-size and compile-time delta against keeping a hand-rolled matcher.
5. **Behavioral differences.** Run the existing three tests in `gitignore_fs.rs::tests` against an `ignore`-backed implementation; note any divergence.

## Decision criteria

Adopt `ignore` if:
- The three existing tests pass without contortion.
- The crate accepts synthetic paths, or there is a clean adapter.
- The added compile-time cost is not material against the existing dependency tree (`rig`, `tokio`, `vfs`).

Otherwise, document the gaps in this file and keep the in-tree matcher.

## Out of scope

- Reworking the broader `GitignoreFs` composition shape.
- Async variants of the matcher.
