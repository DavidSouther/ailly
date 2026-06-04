//! Feature test for `open_engine_for_model`'s provider-family routing.
//!
//! End-to-end user story: a run reads a conversation's `meta.model` and asks
//! `open_engine_for_model` for the engine that serves it. The id's prefix
//! names the provider family — `claude-*` is Anthropic, `gpt-*` is `OpenAI`,
//! `gemini-*` is Gemini — and each family resolves through its own
//! `*_from_env` constructor, which reads that provider's key. A model id whose
//! prefix matches no known family is genuinely unserviceable and returns
//! `ModelNotFound`.
//!
//! The load-bearing distinction is between *recognition* and *authorisation*.
//! A `gpt-5-turbo` binding with no `OPENAI_API_KEY` exported is a recognised
//! family that lacks a key: it must fail at the key check with
//! `EngineError::Auth`, NOT fall through the routing table to `ModelNotFound`.
//! Before this feature, only `claude-*` was wired, so every `gpt-*` /
//! `gemini-*` id produced the wrong failure (`ModelNotFound`) regardless of
//! whether a key was present.
//!
//! This test runs fully offline. It clears each provider key to force the
//! deterministic Auth path (the key read precedes any network or client
//! construction), and asserts a genuinely unknown prefix still routes to
//! `ModelNotFound`. The model ids `gpt-5-turbo` and `gemini-3-pro` are
//! fictional/future ids used only to exercise routing; no live call is made.
//!
//! All four routing arms are asserted inside one test function so the env-var
//! save/clear/restore dance runs single-threaded, matching the convention the
//! `open_engine_for_model` unit tests already document.

use ailly_two::content::conversation::ModelId;
use ailly_two::engine::engine::EngineError;
use ailly_two::engine::engine::open_engine_for_model;

/// Run `body` with `var` removed from the environment, restoring its prior
/// value afterward. Mirrors the save/clear/restore dance in the
/// `open_engine_for_model` unit tests (src/engine/engine.rs).
fn with_key_cleared<T>(var: &str, body: impl FnOnce() -> T) -> T {
    let saved = std::env::var(var).ok();
    // SAFETY: integration test functions in this file run single-threaded
    // (all routing arms are in one test), matching the env-var convention the
    // engine unit tests document; removing/restoring an env var is the
    // documented unsafe operation under the 2024 edition.
    unsafe {
        std::env::remove_var(var);
    }
    let result = body();
    // SAFETY: restoring the previously observed value (or its absence).
    unsafe {
        match saved {
            Some(value) => std::env::set_var(var, value),
            None => std::env::remove_var(var),
        }
    }
    result
}

#[test]
fn open_engine_routes_by_prefix_and_keyless_recognised_model_fails_with_auth() {
    // Arrange / Act / Assert — claude-*: recognised Anthropic family with no
    // ANTHROPIC_API_KEY fails at the key check with Auth, not ModelNotFound.
    let claude = with_key_cleared("ANTHROPIC_API_KEY", || {
        open_engine_for_model(&ModelId::from("claude-opus-4-7"))
    });
    match claude {
        Err(EngineError::Auth { .. }) => {}
        Err(other) => panic!(
            "claude-* with no ANTHROPIC_API_KEY must route to Anthropic and fail with Auth; \
             got {other:?}"
        ),
        Ok(_) => panic!("claude-* with no ANTHROPIC_API_KEY must not resolve to an engine"),
    }

    // gpt-*: recognised OpenAI family with no OPENAI_API_KEY fails with Auth.
    // This is the load-bearing arm — before the feature it was ModelNotFound.
    let gpt = with_key_cleared("OPENAI_API_KEY", || {
        open_engine_for_model(&ModelId::from("gpt-5-turbo"))
    });
    match gpt {
        Err(EngineError::Auth { .. }) => {}
        Err(EngineError::ModelNotFound { .. }) => panic!(
            "gpt-5-turbo is a recognised family lacking a key: it must fail with Auth, \
             not ModelNotFound (the pre-feature bug)"
        ),
        Err(other) => panic!("gpt-* with no OPENAI_API_KEY must fail with Auth; got {other:?}"),
        Ok(_) => panic!("gpt-* with no OPENAI_API_KEY must not resolve to an engine"),
    }

    // gemini-*: recognised Gemini family with no GEMINI_API_KEY fails with Auth.
    let gemini = with_key_cleared("GEMINI_API_KEY", || {
        open_engine_for_model(&ModelId::from("gemini-3-pro"))
    });
    match gemini {
        Err(EngineError::Auth { .. }) => {}
        Err(EngineError::ModelNotFound { .. }) => panic!(
            "gemini-3-pro is a recognised family lacking a key: it must fail with Auth, \
             not ModelNotFound (the pre-feature bug)"
        ),
        Err(other) => panic!("gemini-* with no GEMINI_API_KEY must fail with Auth; got {other:?}"),
        Ok(_) => panic!("gemini-* with no GEMINI_API_KEY must not resolve to an engine"),
    }

    // Unknown prefix: a genuinely unserviceable id still returns ModelNotFound
    // carrying the requested id, regardless of which provider keys are set.
    let unknown = ModelId::from("mistral-large");
    match open_engine_for_model(&unknown) {
        Err(EngineError::ModelNotFound { model }) => assert_eq!(
            model, unknown,
            "ModelNotFound must carry the unrecognised id that was requested"
        ),
        Err(other) => panic!(
            "an unrecognised model prefix must route to ModelNotFound, not a provider arm; \
             got {other:?}"
        ),
        Ok(_) => panic!("an unrecognised model prefix must not resolve to an engine"),
    }
}
