#![cfg(feature = "bedrock")]
//! Feature test for the Bedrock engine.
//!
//! User story: when the `bedrock` Cargo feature is enabled, a developer
//! calls `bedrock_from_env(model)` and gets back a real `Engine` wired
//! through the shared `RigEngine<M>` adapter. Construction is synchronous
//! and never touches the network. The resulting engine satisfies the same
//! input contract as the existing Anthropic and OpenAI rig engines, which
//! proves the new constructor is genuinely plugged into the shared adapter
//! and not a placeholder.

use ailly::engine::{Engine, EngineInput, Settings, bedrock_from_env};

#[test]
fn bedrock_from_env_returns_a_rig_engine_without_network() {
    let engine = bedrock_from_env("us.anthropic.claude-sonnet-4-5-20250929-v1:0")
        .expect("bedrock_from_env should construct without network access");

    let result = engine.stream(EngineInput::default(), &Settings::default(), "feature-test");
    let err = match result {
        Ok(_) => panic!("empty history must error synchronously"),
        Err(e) => e,
    };

    assert!(
        err.to_string().contains("history is empty"),
        "expected the shared rig-engine empty-history error, got: {err}",
    );
}
