# Engine slice refactor plan

Files in scope: `src/engine/{mod.rs, generator.rs, noop.rs, feature_test.rs}` and the `history_for` and turn-accessor additions in `src/content/mod.rs`.

Metric checks before changes:

- Engine independence: only `generator.rs` and `feature_test.rs` import `Conversation`. `mod.rs` and `noop.rs` do not. ✓
- No metadata leakage: grep `ContentMeta`/`Conversation` in `src/engine/` returns only `Generator`/`feature_test` references. ✓
- Deterministic Noop: covered by `two_runs_produce_byte_equal_text_payloads`. ✓
- Sub-second test runtime: full `cargo test --lib` finishes in 0.00s. ✓
- Cancellation: deferred per the design; `cancel_token` exists on the Generator and is unused in Step 3. ✓

## Refactorings

- [x] **Dead-temporary**: `src/engine/generator.rs:69` allocates `let request_label = path.as_str().to_string();` only to pass `&request_label` into `engine.stream`. Pass `path.as_str()` directly so the temporary `String` goes away.
- [x] **Magic constant**: `src/engine/noop.rs:15` and `src/engine/mod.rs:26` use bare literals `32` and `5` for the Noop default chunk size and Settings default request limit. Extract module-level `pub const DEFAULT_CHUNK_BYTES: usize = 32;` in `noop.rs` and a `const DEFAULT_REQUEST_LIMIT: usize = 5;` near `Settings::default` in `mod.rs`. Use them from `default()` and from the `assert!(chunk.len() <= 32, ...)` in the noop test.

## Deferred

- **Duplication of `message_text`**: same helper exists in `src/content/mod.rs:1081` (test-only) and `src/engine/noop.rs:72` (production). Two strikes is not a smell yet; revisit on the third occurrence (likely arrives with `RigEngine` or a second engine impl).
- **`Noop` field exposure**: both fields are `pub`, which is convenient for tests and the design's existing examples. Builders or `with_*` constructors would close the surface, but the public shape is documented in the design and changing it now is scope-creep.
- **`Generator::run` length**: the body is one `async_stream::stream!` block. Extracting per-turn helpers does not compose cleanly with `yield`. Revisit when more behavior (skip filter, isolation, cancellation) is added.
- **`request_label` newtype**: `Engine::stream` takes `&str`. Wrapping it would echo the design's primitive-obsession defense, but the rig adapter and consumers do not yet need a stricter type.
