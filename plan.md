# Plan: Bedrock Engine Implementation

**Feature test:** `tests/engine_routing_bedrock.rs` — `open_engine_routes_bedrock_prefixed_models_to_a_real_constructor`

**User story:** A developer addresses a Bedrock-hosted model with an Ailly-side `bedrock:` prefix in `meta.model`, and `open_engine_for_model` — the same router every other provider goes through — resolves it to a real `RigEngine<rig_bedrock::completion::CompletionModel>` instead of the pre-feature `ModelNotFound` dead end, with no AWS credentials required to complete construction.

**Design:** `.ailly/developer/2026-07-06-A-ailly-evals/feature-b-bedrock-engine/design.md` (cleared; Open Artifact Decisions resolved by the long-loop reviewer, 2026-07-06).

## Steps checklist

- [ ] Step 0 — API surface: Cargo feature default-on, dependency comments, cfg-placement decision (no function bodies)
- [ ] Step 1 — Implement `bedrock_from_env`'s body
- [ ] Step 2 — Add the `"bedrock:"` router branch in `open_engine_for_model`
- [ ] Step 3 — Lock non-regression: unprefixed ids and disabled-feature builds still `ModelNotFound`
- [ ] Step 4 — Doc-comment polish and default-feature build confirmation

---

## Step 0 — API surface stubs (no function bodies)

No new public types, enums, or function signatures are introduced. `RigEngine<M>`'s existing generic `EngineProvider` impl and `bedrock_from_env`'s existing signature already cover everything this feature-step needs (Specification item 4). Step 0 fixes only the declarative/build-graph surface and the one mechanical shape decision the design deferred to Plan:

- **`Cargo.toml`:**
  - `[features]` gains `default = ["bedrock"]`, alongside the existing `bedrock = ["dep:rig-bedrock"]`.
  - `rig-bedrock` stays a named `optional = true` dependency (not folded into `[dependencies]`) — resolved Decision 1.
  - A one-line source comment added directly above the `rig` (`rig-core`) version line and the `[dependencies.rig-bedrock]` stanza, stating: bumping `rig`'s version requirement past `0.37` may make it unsatisfiable alongside `rig-bedrock = "0.4"` (which caps before `rig-bedrock-v0.4.7`'s `rig-core = "0.38.0"` requirement) — resolved Decision 3, comment-only guard.
- **`src/engine/rig_engine.rs`:** `bedrock_from_env`'s signature is unchanged from what already exists:
  ```
  #[cfg(feature = "bedrock")]
  pub fn bedrock_from_env(model: &str) -> Result<RigEngine<rig_bedrock::completion::CompletionModel>, EngineError>
  ```
  Only its body (Step 1) and doc comment (Step 4) change.
- **`src/engine/engine.rs`:** `open_engine_for_model`'s signature is unchanged:
  ```
  pub fn open_engine_for_model(model: &ModelId) -> Result<Box<dyn EngineProvider>, EngineError>
  ```
  Fixed shape decision (resolved Decision 4): the new branch is a single `#[cfg(feature = "bedrock")]`-gated `if let Some(remainder) = id.strip_prefix("bedrock:") { ... return ...; }` block, placed immediately before the function's final `Err(EngineError::ModelNotFound { .. })` fallthrough — mirroring the existing `starts_with("claude-")`/`"gpt-"`/`"gemini-"` guards' shape, not two `cfg`-gated function variants. No new helper function is introduced for this block.

This step is committed once `cargo check --all-features --all-targets` and `cargo check --no-default-features --all-targets` both compile cleanly with the Cargo.toml change alone (the router body and constructor body still unchanged, so the feature test remains red for the same reason as today — nothing here unblocks an assertion by itself).

## Step 1 — Implement `bedrock_from_env`'s body

**Assertion unblocked:** the feature test's fourth, `#[cfg(feature = "bedrock")]`-gated assertion (`tests/engine_routing_bedrock.rs:101-119`) — a direct call to `bedrock_from_env` must no longer return the hardcoded `"not yet implemented"` `Provider` error.

**Happy-path test sketch:** a focused unit test alongside the existing `openai_from_env_without_key_fails_with_auth`/`gemini_from_env_without_key_fails_with_auth` tests in `src/engine/rig_engine.rs`'s `#[cfg(test)] mod tests`, gated `#[cfg(feature = "bedrock")]`: call `bedrock_from_env("meta.llama3-3-70b-instruct-v1:0")` with no AWS environment variables required, assert the result is `Ok(_)` — no network call, no async runtime needed, since construction never awaits `get_inner()`.

**Implementation outline:**
- Bring `rig::client::ProviderClient` into scope alongside the already-imported `rig::client::CompletionClient` on the same `use` line inside the function body.
- Call `rig_bedrock::client::Client::from_env()`; map its `Err` arm to `EngineError::Provider` (unreachable today per Prior Art, but required for type correctness since the trait method returns a `Result`).
- Call `client.completion_model(model)` to get the `rig_bedrock::completion::CompletionModel` (infallible).
- Return `Ok(RigEngine::new(completion_model, "bedrock", model))`, matching the `engine_name` convention (`"anthropic"`, `"openai"`, `"gemini"`) already used by the other three constructors.

## Step 2 — Add the `"bedrock:"` router branch in `open_engine_for_model`

**Assertion unblocked:** the feature test's first and second assertions (`tests/engine_routing_bedrock.rs:53-64` and `:66-78`) — a named Bedrock model id, and an inference-profile ARN passed after the `bedrock:` prefix, both resolve to `Ok(_)` through the router, not `ModelNotFound`.

**Happy-path test sketch:** extend `src/engine/engine.rs`'s own `#[cfg(test)] mod tests` with a case (gated `#[cfg(feature = "bedrock")]`) that calls `open_engine_for_model(&ModelId::from("bedrock:meta.llama3-3-70b-instruct-v1:0"))` and asserts `Ok(_)`, and a second case with the inference-profile ARN string after the prefix asserting `Ok(_)` too, proving no shape validation is applied to the remainder.

**Implementation outline:**
- Insert the `#[cfg(feature = "bedrock")]`-gated `if let Some(remainder) = id.strip_prefix("bedrock:")` block fixed in Step 0, positioned after the existing `"gemini-"` branch and before the final `ModelNotFound` fallthrough.
- Inside the block, call `crate::engine::rig_engine::bedrock_from_env(remainder)`, propagate its `Err` with `?`, and box the resulting `RigEngine<rig_bedrock::completion::CompletionModel>` into `Ok(Box::new(engine))`, exactly matching the existing `claude-`/`gpt-`/`gemini-` branches' shape.
- Pass `remainder` straight through unvalidated (no format check on raw AWS model id vs. ARN), per the parent research's decision and Specification item 3.

## Step 3 — Lock non-regression: unprefixed ids and disabled-feature builds still `ModelNotFound`

**Assertion unblocked:** the feature test's third assertion (`tests/engine_routing_bedrock.rs:80-93`) — the same raw AWS model id *without* the `bedrock:` prefix must still resolve to `Err(ModelNotFound)`. This assertion does not require new production code (prefix dispatch via `strip_prefix` already excludes non-matching ids by construction), but it is the regression this step exists to pin down explicitly, alongside the resolved `--no-default-features` behavior (Decision 2) that the feature test itself does not exercise (it always runs `--all-features` per its own doc comment).

**Happy-path test sketch:**
- In `src/engine/engine.rs`'s test module, a case calling `open_engine_for_model(&ModelId::from("meta.llama3-3-70b-instruct-v1:0"))` (no prefix) and asserting `Err(EngineError::ModelNotFound { model }) if model == requested` — mirroring the existing `open_engine_for_model_unrecognised_prefix_returns_model_not_found` test's shape.
- A second case, *not* gated on `#[cfg(feature = "bedrock")]` (i.e., compiled and run in a `--no-default-features` build), calling `open_engine_for_model(&ModelId::from("bedrock:meta.llama3-3-70b-instruct-v1:0"))` and asserting `Err(EngineError::ModelNotFound { .. })` — proving the resolved fallthrough behavior for a disabled feature.

**Implementation outline:** no production code change; this step is verification-only, run once via `cargo test --no-default-features` (confirming the second case above) and once via the project's default `cargo test`/`mise run test` (confirming the first case and the feature test's own third assertion together with the now-passing first two).

## Step 4 — Doc-comment polish and default-feature build confirmation

**Assertion unblocked:** none new (all four feature-test assertions already pass after Step 3); this step closes the design's remaining Specification/Metrics items that are not test-visible but are part of "done": the `bedrock` feature is on by default so a plain `cargo build`/`cargo test` includes Bedrock support, and the router's and constructor's doc comments accurately describe the new behavior for a future reader.

**Happy-path test sketch:** none new; re-run the full existing suite (`mise run test`, which already passes `--all-features`) plus a plain `cargo test` (no flags) to confirm the feature test's first three assertions pass by default now that `bedrock` is default-on, and `cargo test --features bedrock` / the project's default invocation both exercise the fourth, cfg-gated assertion without an explicit `--features` flag.

**Implementation outline:**
- Update `bedrock_from_env`'s doc comment (Specification item 2) to state plainly that the constructor supports and automatically prefers `AWS_BEARER_TOKEN_BEDROCK` over the standard SigV4 credential chain when both are present, per the design's Prior Art finding — a fact true of the exact call the constructor makes but invisible from the code alone.
- Extend `open_engine_for_model`'s existing per-branch doc-comment bullets (Specification item 3) with a `"bedrock:"` bullet calling out the asymmetry: unlike the other three providers, a recognized-but-credential-less Bedrock id resolves to `Ok`, not `Err(Auth)`, because credential resolution is deferred to the first live call.
- Run `mise run check`, `mise run lint`, and `mise run test` (all already `--all-features`) plus a plain `cargo build`/`cargo test` with no flags to confirm the default-on feature closes the gap described in the design's Metrics section.

## Resolved by the long-loop reviewer (2026-07-06)

**1. Plan self-review against the checked-out worktree. Decided: the plan is accurate as written; no step re-shaping needed.** Read cold against `Cargo.toml`, `src/engine/rig_engine.rs`, `src/engine/engine.rs`, and `tests/engine_routing_bedrock.rs` on disk: every signature, doc-comment, and code-shape claim in Steps 0-3 matches the actual code (the three existing `if id.starts_with(prefix) { ...; return ...; }` router branches, `bedrock_from_env`'s existing `#[cfg(feature = "bedrock")]` stub signature, and the feature test's four ordered assertions). Five steps (0-4) sits inside the required 3-7 range and each step unblocks a distinct, named assertion or closes a distinct non-test-visible Specification item, so no merge/split was warranted.

**2. Design's four Open Artifact Decisions. Decided: already resolved correctly by the prior planning pass; nothing left open.** Re-read `design.md`'s "Open Artifact Decisions" subsection: all four adopt the design's own recommended defaults exactly as instructed (toggleable default-on feature; `ModelNotFound` fallthrough when the feature is disabled; comment-only version-pin guard with the CI check noted as a deferred nice-to-have; single cfg-gated `if let` router block, with the exact placement fixed in this plan's own Step 0). No escalation trigger (irreversible / out of recorded scope / underdetermined) applies to any of the four — each has a documented conservative rationale already in `design.md`. Plan's own Draft marker removed; proceeding to build.
