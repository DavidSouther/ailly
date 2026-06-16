# TASK-NOTES: project-layout deferred decisions

Five revisit-when-consumer-arrives items carried over from the project-layout design doc (`docs/developer/2026-05-25-A-project-layout/design.md`, "Deferred decisions" section).

## 1. Recursive globs (`**` patterns)

**Status:** Out of scope. Today's call sites in `Context::glob_concat` use only single-segment globs (`*.md`, `*.json`); the `read_dir`-driven walker that replaced `glob::glob` covers the production shape with a single segment match.

**Trigger to revisit:** An assembly or eval recipe needs cross-directory matching (e.g., `context/**/*.md` to pull every markdown file under a tree, or a retrieval block whose `source` walks subdirectories). When that call site lands, extend the walker to handle multi-segment patterns; do not add `**` support speculatively.

## 2. Per-prompt cache strategy

**Status:** `Prompts<'_>` exists as a separate sub-handle on `Project` so that prompt-side divergence is cheap (e.g., file-level content addressing for prompt deduplication, hashed cache keys). No divergence in the current slice — `Prompts` is structurally identical to `Context`.

**Trigger to revisit:** A consumer wants prompt-level caching that differs from `context/` (e.g., a large-prompt dedup pass, or a content-hash key for cache-breakpoint stability across runs). When that consumer lands, the sub-handle is already in place to grow its own behavior without touching `Context`.

## 3. `RemoteAssemblyRepository`

**Status:** The `AssemblyRepository`, `EvaluationRepository`, `ContextRepository`, `PromptsRepository`, and `ConversationRepository` traits remain open to network-backed adapters, but only `vfs`-backed implementations land in this slice. The Repository pattern is named in `TASKS.md` for exactly this future shape.

**Trigger to revisit:** The agent-based-eval workflow (or any other) needs a project served from a remote URL (HTTP-backed assemblies, S3-hosted context, etc.). When that consumer lands, implement the repository trait against the network adapter and inject it via `Project::open` (or a new `Project::open_remote` constructor); the sub-handle interface should not need to change.

## 4. Project schema validation at open

**Status:** `Project::open` only checks that the root exists and is a directory. It does not check that any subfolder exists or that any file parses. The first call to `assemblies().get(...)` is the first parse. This matches the YAGNI principle: parse errors surface where the parse happens.

**Trigger to revisit:** Early-fail behavior becomes a UX requirement (e.g., users want `ailly assemble` to fail fast on a typo'd assembly name before doing any rendering work, with a "did you mean X?" suggestion). At that point, add a schema-validation pass to `Project::open` or a separate `Project::validate` method; do not put it in the hot path unless explicitly requested.

## 5. Atomic `save` on `MemoryFS`

**Status:** `VfsPath::move_file` is atomic per backend, not across backends. The "same-file collision" caveat from `FsConversationRepository::save` documentation carries forward to the `vfs`-backed adapter; cross-process atomicity guarantees are unchanged. Concurrent writers within a single process against the same `MemoryFS` mount are not protected; tests are single-threaded against each `Project::open_memory` value. A multi-threaded test that shares a `Project` is undefined behavior.

**Trigger to revisit:** A concurrent-`ailly run` use case appears (e.g., a parallel matrix-binding execution that writes to the same `runs/` directory from multiple threads, or a multi-process orchestration). At that point, decide whether to add per-`RunTx` locking, switch to a content-addressed run-id scheme that makes collisions impossible by construction, or document the constraint as a hard precondition of the CLI shape.
