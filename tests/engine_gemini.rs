//! Feature test for the Gemini engine.
//!
//! User story: a user holds a `GEMINI_API_KEY` and runs `ailly --engine
//! gemini`. The CLI calls `gemini_from_env(model)`, which constructs a real
//! `Engine` wired through the shared `RigEngine<M>` adapter without any
//! Cargo feature gate. From that moment on, every invariant of the shared
//! adapter applies to the Gemini path identically to Anthropic and OpenAI,
//! proving the new constructor is a true slot-in and not a placeholder.

use ailly::engine::{Engine, Settings, gemini_from_env};

#[test]
fn gemini_from_env_returns_a_rig_engine_without_network() {
    // SAFETY: each integration test file is its own process binary, and
    // this is the only test in this file. The env var is process-local
    // for the test's full lifetime and is never observed by another test.
    unsafe { std::env::set_var("GEMINI_API_KEY", "dummy-feature-test-key") };

    let engine = gemini_from_env("gemini-2.5-flash")
        .expect("gemini_from_env should construct from a populated env var");

    let result = engine.stream(Vec::new(), &Settings::default(), "feature-test");
    let err = match result {
        Ok(_) => panic!("empty history must error synchronously through the shared adapter"),
        Err(e) => e,
    };

    assert!(
        err.to_string().contains("history is empty"),
        "expected the shared rig-engine empty-history error, got: {err}",
    );
}
