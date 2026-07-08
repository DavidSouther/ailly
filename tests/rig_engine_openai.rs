//! Feature test for the Rig-backed `EngineProvider` adapter's `OpenAI` leg.
//!
//! End-to-end user story: a caller has an `OPENAI_API_KEY` exported in
//! their environment. They construct a `RigEngine` via `openai_from_env`
//! for `gpt-4o-mini`, build an Ailly `CompletionRequest` that contains a
//! system instruction plus a user turn, and hand the request to the engine
//! through the `EngineProvider::complete` port. The engine translates the
//! Ailly messages into the Rig request shape, calls `OpenAI`'s completion
//! API, and returns one `CompletionResponse` whose `content` carries the
//! model's reply and whose `Trace` carries the provider's token usage, the
//! actual model called, a non-empty `span_id`, a measured `latency_ms`, and
//! at minimum one `gen_ai.completion` event whose attributes follow the
//! OpenTelemetry `gen_ai.*` semantic conventions with `gen_ai.system` set
//! to `"openai"`.
//!
//! This confirms the `OpenAI` leg of the ailly-evals project's Feature C
//! (multi-provider live confirmation) — the code path already existed and
//! was unit-tested; this is the first *live* round trip against it. It
//! deliberately does not exercise tool-calling: `CompletionRequest` has no
//! `tools` field on any provider today (tracked separately in
//! `docs/developer/TASKS.md`, "live tool-definition wiring is completely
//! missing"), so this test's only job is the basic-completion round trip
//! and the OpenAI-specific system-prompt-placement rough edge the parent
//! project's research flagged.
//!
//! This is a live integration test: it issues a real HTTPS call to `OpenAI`.
//! It is gated on the `OPENAI_API_KEY` environment variable and is silently
//! skipped when the variable is absent so `cargo test` stays green in
//! environments without credentials.

use ailly_two::content::conversation::Content;
use ailly_two::content::conversation::Message;
use ailly_two::content::conversation::ModelId;
use ailly_two::content::conversation::Role;
use ailly_two::engine::engine::CompletionRequest;
use ailly_two::engine::engine::EngineProvider;
use ailly_two::engine::rig_engine::openai_from_env;

const LIVE_API_KEY_ENV: &str = "OPENAI_API_KEY";
const MODEL: &str = "gpt-4o-mini";

fn rendered_system(text: &str) -> Message {
    Message {
        role: Role::System,
        body: Some(Content::Text(text.to_owned())),
        cache: true,
        trace: None,
        _phase: std::marker::PhantomData,
    }
}

fn rendered_user(text: &str) -> Message {
    Message {
        role: Role::User,
        body: Some(Content::Text(text.to_owned())),
        cache: false,
        trace: None,
        _phase: std::marker::PhantomData,
    }
}

#[tokio::test]
async fn rig_engine_complete_against_live_openai_populates_content_and_trace() {
    // Arrange — load a `.env` from the workspace root if present, then skip
    // silently when the live API key is still absent, mirroring
    // `tests/rig_engine.rs`'s Anthropic gating so this test stays green in
    // any environment without OpenAI credentials.
    let _ = dotenvy::dotenv();
    let Ok(api_key) = std::env::var(LIVE_API_KEY_ENV) else {
        eprintln!(
            "skipping rig_engine_openai live test: {LIVE_API_KEY_ENV} not found in environment or .env"
        );
        return;
    };
    assert!(!api_key.is_empty(), "{LIVE_API_KEY_ENV} must not be empty");

    let engine = openai_from_env(MODEL).expect(
        "openai_from_env must succeed when OPENAI_API_KEY is set and the model id is known",
    );

    let messages = vec![
        rendered_system(
            "You answer concisely. Reply with exactly the single word 'pong' and nothing else.",
        ),
        rendered_user("ping"),
    ];
    let request = CompletionRequest {
        model: ModelId::from(MODEL),
        messages: &messages,
        debug: false,
    };

    // Act — issue the live call. A system turn placed ahead of the user
    // turn is exactly the shape the parent research flagged as an OpenAI
    // rough edge (system-prompt placement under Responses API churn); this
    // call succeeding is the confirmation that shape still round-trips.
    let response = engine
        .complete(request)
        .await
        .expect("live OpenAI completion succeeds with a valid API key and a known model");

    // Assert — body lowered from the response.
    match response.content {
        Content::Text(ref text) => {
            assert!(
                !text.trim().is_empty(),
                "OpenAI reply must be a non-empty string for this prompt; got {text:?}"
            );
        }
        Content::Blocks(_) => {
            panic!(
                "single-text response from OpenAI must lower to Content::Text, not Content::Blocks"
            );
        }
    }

    // Assert — trace carries the actual model called, a non-empty span id,
    // a measured latency, and non-zero token counts in both directions.
    let trace = &response.trace;
    assert_eq!(
        trace.model.as_ref(),
        MODEL,
        "trace.model records the model actually called"
    );
    assert!(
        !trace.span_id.as_ref().is_empty(),
        "trace.span_id must be populated"
    );
    assert!(
        trace.latency_ms > 0,
        "trace.latency_ms must reflect a measured wall-clock duration; got 0"
    );
    assert!(
        trace.tokens.input > 0,
        "OpenAI always reports non-zero input tokens; got {}",
        trace.tokens.input
    );
    assert!(
        trace.tokens.output > 0,
        "OpenAI always reports non-zero output tokens for a non-empty reply; got {}",
        trace.tokens.output
    );

    // Assert — at least one gen_ai.completion event, with the OpenTelemetry
    // semantic-convention attributes present, and `gen_ai.system` reading
    // "openai" (not "anthropic" — this is the discriminating assertion that
    // proves the provider-identifier plumbing is correct end to end, not
    // just for the Anthropic leg).
    let completion_event = trace
        .events
        .iter()
        .find(|event| event.name == "gen_ai.completion")
        .expect("at least one gen_ai.completion TraceEvent must be emitted on success");
    let attrs = &completion_event.attributes;

    let system_attr = attrs
        .get("gen_ai.system")
        .and_then(serde_yaml_ng::Value::as_str)
        .expect("gen_ai.system attribute is required by the OpenTelemetry semconv");
    assert_eq!(
        system_attr, "openai",
        "gen_ai.system identifies the provider family"
    );
    let request_model = attrs
        .get("gen_ai.request.model")
        .and_then(serde_yaml_ng::Value::as_str)
        .expect("gen_ai.request.model attribute is required by the OpenTelemetry semconv");
    assert_eq!(request_model, MODEL);
    let event_input_tokens = attrs
        .get("gen_ai.usage.input_tokens")
        .and_then(serde_yaml_ng::Value::as_u64)
        .expect("gen_ai.usage.input_tokens attribute is required");
    assert_eq!(
        event_input_tokens, trace.tokens.input,
        "gen_ai.usage.input_tokens must agree with trace.tokens.input"
    );
    let event_output_tokens = attrs
        .get("gen_ai.usage.output_tokens")
        .and_then(serde_yaml_ng::Value::as_u64)
        .expect("gen_ai.usage.output_tokens attribute is required");
    assert_eq!(
        event_output_tokens, trace.tokens.output,
        "gen_ai.usage.output_tokens must agree with trace.tokens.output"
    );
}
