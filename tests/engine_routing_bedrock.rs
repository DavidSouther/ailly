//! Feature test for Feature B — Bedrock engine implementation.
//!
//! End-to-end user story: a developer points a conversation's `meta.model`
//! at a Bedrock-hosted model using an Ailly-side `"bedrock:"` prefix (e.g.
//! `bedrock:meta.llama3-3-70b-instruct-v1:0`, one of the four Bedrock models
//! named in `domain-driven-design#29`'s runner matrix), and
//! `open_engine_for_model` — the same router `ailly run` calls for every
//! provider — resolves it to a real engine backed by
//! `rig_bedrock::completion::CompletionModel`, instead of the pre-feature
//! dead end where every `"bedrock:"` id fell through the prefix table to
//! `ModelNotFound`.
//!
//! Bedrock's own credential model is why this test's assertion shape
//! differs from `tests/engine_routing.rs`'s "recognised but keyless" pattern
//! for `gpt-*`/`gemini-*`. Anthropic/OpenAI/Gemini's `*_from_env`
//! constructors read one required API-key env var *before* building any
//! client, so a missing key deterministically fails with `EngineError::Auth`
//! offline. Bedrock resolves credentials through the AWS SDK's own chain
//! (a Bedrock API key via `AWS_BEARER_TOKEN_BEDROCK`, preferred automatically
//! when set, falling back to standard `SigV4` credentials — env vars, shared
//! profile, SSO, IMDS — otherwise), and `rig_bedrock`'s `Client::from_env()`
//! never inspects either mechanism at construction time — it always
//! succeeds, deferring resolution to the first live call. So a
//! keyless Bedrock id cannot be told apart from a well-configured one until
//! a real network call is attempted, which this test deliberately does not
//! do (no AWS credentials are available in this environment). Instead this
//! test proves the *routing and construction* seam is closed: recognised
//! `"bedrock:"` ids now resolve to `Ok`, where they used to resolve to
//! `Err(ModelNotFound)`.
//!
//! Running today (before Feature B lands) this test is red for two
//! independent reasons, exercised together:
//!   1. `open_engine_for_model` has no `"bedrock:"` branch at all, so every
//!      case below currently falls through to `ModelNotFound`.
//!   2. `bedrock_from_env` (only reachable with the `bedrock` Cargo feature,
//!      not yet on by default) unconditionally returns `EngineError::Provider {
//!      message: "rig_engine: not yet implemented" }`.
//!
//! Verify with `cargo test --test engine_routing_bedrock --all-features`
//! (matching `mise run test`'s existing `--all-features` convention) so the
//! `bedrock`-gated block below compiles; without that flag the block is
//! `cfg`'d out, but the router-level assertions above it still fail red on
//! their own.

use ailly_two::content::conversation::ModelId;
use ailly_two::engine::engine::EngineError;
use ailly_two::engine::engine::open_engine_for_model;

#[test]
fn open_engine_routes_bedrock_prefixed_models_to_a_real_constructor() {
    // Arrange / Act / Assert — a named issue-#29 Bedrock model, addressed
    // through the Ailly-side "bedrock:" prefix, must now resolve to a real
    // engine rather than the pre-feature ModelNotFound dead end.
    let named = ModelId::from("bedrock:meta.llama3-3-70b-instruct-v1:0");
    match open_engine_for_model(&named) {
        Ok(_) => {}
        Err(EngineError::ModelNotFound { model }) => panic!(
            "bedrock: prefix must route to a real engine, not ModelNotFound \
             (the pre-feature dead end); got ModelNotFound for {model:?}"
        ),
        Err(other) => panic!(
            "bedrock: prefix must resolve to Ok (AWS credential resolution is \
             deferred to the first live call, not the constructor); got {other:?}"
        ),
    }

    // An inference-profile ARN must pass through unvalidated, per the
    // research decision that the remainder after "bedrock:" is forwarded
    // verbatim to rig-bedrock as a raw AWS model id or ARN.
    let arn = ModelId::from(
        "bedrock:arn:aws:bedrock:us-east-1:123456789012:inference-profile/us.meta.llama3-3-70b-instruct-v1:0",
    );
    match open_engine_for_model(&arn) {
        Ok(_) => {}
        Err(other) => panic!(
            "an inference-profile ARN after the bedrock: prefix must resolve to Ok \
             with no format validation; got {other:?}"
        ),
    }

    // The same raw id *without* the Ailly "bedrock:" prefix must still be
    // unrecognised: this is prefix dispatch, not a substring match, and the
    // feature must not loosen routing for ids that were already unrecognised.
    let unprefixed = ModelId::from("meta.llama3-3-70b-instruct-v1:0");
    match open_engine_for_model(&unprefixed) {
        Err(EngineError::ModelNotFound { model }) => assert_eq!(model, unprefixed),
        Err(other) => panic!(
            "an id lacking the bedrock: prefix must still fail with ModelNotFound; got {other:?}"
        ),
        Ok(_) => panic!(
            "a raw AWS model id with no bedrock: prefix must not resolve to an engine \
             (that would be a substring match, not prefix dispatch)"
        ),
    }

    // The constructor itself, one layer below the router: this is the
    // load-bearing assertion that `bedrock_from_env` no longer unconditionally
    // returns the "not yet implemented" stub. Gated on the `bedrock` feature,
    // matching the function's own `#[cfg(feature = "bedrock")]`; run with
    // `--all-features` (or, once this feature-step turns `bedrock` on by
    // default, with no extra flag at all) to exercise it.
    #[cfg(feature = "bedrock")]
    {
        use ailly_two::engine::rig_engine::bedrock_from_env;

        match bedrock_from_env("meta.llama3-3-70b-instruct-v1:0") {
            Ok(_) => {}
            Err(EngineError::Provider { message }) if message.contains("not yet implemented") => {
                panic!(
                    "bedrock_from_env must no longer return the unimplemented stub; \
                     got the pre-feature placeholder error: {message}"
                );
            }
            Err(other) => panic!(
                "bedrock_from_env must construct successfully offline (AWS credential \
                 resolution is deferred to the first live call, not the constructor); \
                 got {other:?}"
            ),
        }
    }
}
