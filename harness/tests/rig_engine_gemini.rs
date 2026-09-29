//! Feature test for the Rig-backed `EngineProvider` adapter's Gemini leg.
//!
//! End-to-end user story: a caller has a `GEMINI_API_KEY` exported in their
//! environment. They construct a `RigEngine` via `gemini_from_env` for a
//! stable Gemini model, build an Ailly `CompletionRequest` that contains a
//! system instruction plus a user turn, and hand the request to the engine
//! through the `EngineProvider::complete` port. The engine translates the
//! Ailly messages into the Rig request shape, calls Gemini's completion API,
//! and returns one `CompletionResponse` whose `content` carries the model's
//! reply and whose `Trace` carries the provider's token usage, the actual
//! model called, a non-empty `span_id`, a measured `latency_ms`, and at
//! minimum one `gen_ai.completion` event whose attributes follow the
//! OpenTelemetry `gen_ai.*` semantic conventions with `gen_ai.system` set
//! to `"gemini"`.
//!
//! When `GEMINI_API_KEY` is present, this confirms the Gemini leg of the
//! ailly-evals project's Feature C (multi-provider live confirmation). The
//! code path already existed and was unit-tested; this is the live round-trip
//! harness for it. It focuses on the basic-completion path and the
//! Gemini-specific telemetry rough edge the parent project's research flagged.
//!
//! This is a live integration test: it issues a real HTTPS call to Gemini.
//! It is gated on the `GEMINI_API_KEY` environment variable and is silently
//! skipped when the variable is absent so `cargo test` stays green in
//! environments without credentials.

use ailly_two::content::conversation::Content;
use ailly_two::content::conversation::Message;
use ailly_two::content::conversation::ModelId;
use ailly_two::content::conversation::Role;
use ailly_two::engine::engine::CompletionRequest;
use ailly_two::engine::engine::EngineProvider;
use ailly_two::engine::rig_engine::gemini_from_env;

const LIVE_API_KEY_ENV: &str = "GEMINI_API_KEY";
const MODEL: &str = "gemini-3.5-flash";

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
async fn rig_engine_complete_against_live_gemini_populates_content_and_trace() {
    // Arrange: load a `.env` from the workspace root if present, then skip
    // silently when the live API key is still absent, mirroring the Anthropic
    // and OpenAI live tests so this stays green in keyless environments.
    let _ = dotenvy::dotenv();
    let Ok(api_key) = std::env::var(LIVE_API_KEY_ENV) else {
        eprintln!(
            "skipping rig_engine_gemini live test: {LIVE_API_KEY_ENV} not found in environment or .env"
        );
        return;
    };
    assert!(!api_key.is_empty(), "{LIVE_API_KEY_ENV} must not be empty");

    let engine = gemini_from_env(MODEL).expect(
        "gemini_from_env must succeed when GEMINI_API_KEY is set and the model id is known",
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

    // Act: issue the live call. This proves the Gemini basic-completion path
    // still round-trips through Rig and Ailly's provider adapter.
    let response = engine
        .complete(request)
        .await
        .expect("live Gemini completion succeeds with a valid API key and a known model");

    // Assert: body lowered from the response.
    match response.content {
        Content::Text(ref text) => {
            assert!(
                !text.trim().is_empty(),
                "Gemini reply must be a non-empty string for this prompt; got {text:?}"
            );
        }
        Content::Blocks(_) => {
            panic!(
                "single-text response from Gemini must lower to Content::Text, not Content::Blocks"
            );
        }
    }

    // Assert: trace carries the actual model called, a non-empty span id, a
    // measured latency, and non-zero token counts in both directions.
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
        "Gemini must report non-zero input tokens; got {}",
        trace.tokens.input
    );
    assert!(
        trace.tokens.output > 0,
        "Gemini must report non-zero output tokens for a non-empty reply; got {}",
        trace.tokens.output
    );

    // Assert: at least one gen_ai.completion event, with the OpenTelemetry
    // semantic-convention attributes present, and `gen_ai.system` reading
    // "gemini".
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
        system_attr, "gemini",
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
