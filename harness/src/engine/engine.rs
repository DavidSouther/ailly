//! `EngineProvider` trait and the deterministic `NoopEngine` adapter.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use crate::content::conversation::Content;
use crate::content::conversation::Message;
use crate::content::conversation::ModelId;
use crate::content::conversation::SpanId;
use crate::content::conversation::TokenCounts;
use crate::content::conversation::Trace;

/// Identity literal stamped on every `NoopEngine` response: appears in
/// `Trace.model` and as the prefix of every generated `SpanId`.
const NOOP_MODEL: &str = "noop";

/// A request to produce the next assistant turn given prior messages.
///
/// Borrowed: the caller lends the messages slice for one engine call; the
/// engine must not retain it.
#[derive(Debug)]
pub struct CompletionRequest<'a> {
    pub model: ModelId,
    pub messages: &'a [Message],
    pub debug: bool,
}

/// Output of a single engine call: the content body that fills the blank
/// assistant slot, and the inline trace that documents how it was produced.
#[derive(Debug)]
pub struct CompletionResponse {
    pub content: Content,
    pub trace: Trace,
}

/// Structural failure modes for any `EngineProvider`.
///
/// Closed set: downstream call sites (run handler, eval judge) `match`
/// exhaustively. Adding a variant is a deliberate contract change; no
/// `#[non_exhaustive]` marker.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// Scripted `NoopEngine` ran out of entries.
    #[error("noop engine has no script entry for call #{call_index}")]
    NoopExhausted { call_index: usize },
    /// 401/403 from the provider, or any other authentication failure
    /// (missing key, malformed key). The secret value itself never appears
    /// here.
    #[error("engine authentication failed: {message}")]
    Auth {
        message: std::borrow::Cow<'static, str>,
    },
    /// 429 from the provider. `retry_after` mirrors the `Retry-After` header
    /// when present; absent when the provider did not advise a wait.
    #[error("engine rate limited (retry_after: {retry_after:?})")]
    RateLimited {
        retry_after: Option<std::time::Duration>,
    },
    /// Transport-level timeout. Distinct from `RateLimited` so callers can
    /// retry with backoff vs. surface a saturation alert.
    #[error("engine call timed out")]
    Timeout,
    /// 404, or a provider error envelope that names the model as unknown.
    /// Carries the `ModelId` actually requested so the message is actionable.
    #[error("engine model not found: {model}")]
    ModelNotFound {
        model: crate::content::conversation::ModelId,
    },
    /// Response did not parse against the provider's documented envelope.
    /// Covers Rig `JsonError(_)`, schema drift, and assertion violations
    /// when lowering Rig's typed response into Ailly's `Content`.
    #[error("engine returned a malformed response: {message}")]
    MalformedResponse { message: String },
    /// Residual for genuinely opaque transport failures. `Provider` stays as
    /// the last-resort variant so the closed set still covers everything Rig
    /// can emit; the typed variants peel cases off the front.
    #[error("engine call failed: {message}")]
    Provider { message: String },
}

/// Produce the next assistant turn for a partially-filled conversation.
#[async_trait::async_trait]
pub trait EngineProvider: Send + Sync {
    /// Produce the next assistant turn given prior messages.
    ///
    /// # Errors
    /// Returns [`EngineError`] when the request cannot be served.
    async fn complete(
        &self,
        request: CompletionRequest<'_>,
    ) -> Result<CompletionResponse, EngineError>;
}

/// Pick the engine adapter for one conversation's `meta.model`.
///
/// The id's prefix names the provider family, and each family resolves through
/// its own `*_from_env` constructor, which reads that provider's key:
///
/// - `"claude-"` resolves via `rig_engine::anthropic_from_env`
///   (`ANTHROPIC_API_KEY`).
/// - `"gpt-"` resolves via `rig_engine::openai_from_env` (`OPENAI_API_KEY`).
/// - `"gemini-"` resolves via `rig_engine::gemini_from_env` (`GEMINI_API_KEY`).
/// - Any other id returns [`EngineError::ModelNotFound`]: an unrecognised
///   prefix is the only `ModelNotFound` path from routing. A provider's own 404
///   for a recognised family is a separate live concern mapped by
///   `engine_error_from_rig`, not by this function.
///
/// Recognition (prefix) and authorisation (key) are distinct failures: a
/// recognised prefix with a missing key reaches its `*_from_env` constructor
/// and fails with [`EngineError::Auth`], never `ModelNotFound`.
///
/// Instantiated per conversation so a future heterogeneous run-dir
/// (multiple models across bindings) needs no further refactoring.
///
/// # Errors
/// [`EngineError::Auth`] when a recognised family is requested without its
/// provider key; [`EngineError::ModelNotFound`] for any unrecognised prefix.
pub fn open_engine_for_model(model: &ModelId) -> Result<Box<dyn EngineProvider>, EngineError> {
    let id = model.as_ref();
    if id == NOOP_MODEL {
        return Ok(Box::new(NoopEngine::auto()));
    }
    if id.starts_with("claude-") {
        let engine = crate::engine::rig_engine::anthropic_from_env(id)?;
        return Ok(Box::new(engine));
    }
    if id.starts_with("gpt-") {
        let engine = crate::engine::rig_engine::openai_from_env(id)?;
        return Ok(Box::new(engine));
    }
    if id.starts_with("gemini-") {
        let engine = crate::engine::rig_engine::gemini_from_env(id)?;
        return Ok(Box::new(engine));
    }
    Err(EngineError::ModelNotFound {
        model: model.clone(),
    })
}

/// `true` when `model` names the deterministic Noop adapter.
///
/// The eval handler uses this to decline a Noop *grader*: an
/// [`NoopEngine::auto`] cannot produce a `GRADE:` line, so a judge assertion
/// run against it would malform rather than report a meaningful verdict. A
/// `noop` run therefore resolves to no judge engine and judge assertions defer,
/// matching every synthetic e2e suite. A scriptable Noop judge engine — feeding
/// `from_scripts` grades so a judge can pass deterministically offline — would
/// lift this and is tracked in `TASKS.md`.
#[must_use]
pub fn is_noop_model(model: &ModelId) -> bool {
    model.as_ref() == NOOP_MODEL
}

/// Distinguishes scripts whose `Trace` is owned by the caller from scripts
/// that the engine fills with its default trace at completion time.
enum ScriptEntry {
    /// Caller-supplied response; the full `Trace` is preserved verbatim.
    Fixed(CompletionResponse),
    /// `from_replies` body; the engine builds a fresh default trace with a
    /// `noop-{call_index}` span id at completion time.
    AutoStamp(Content),
}

/// Default `Trace` stamped on every `from_replies` completion: zero tokens,
/// zero latency, no events, and a per-call span id.
fn noop_trace(call_index: usize) -> Trace {
    Trace {
        span_id: SpanId::from(format!("{NOOP_MODEL}-{call_index}")),
        model: ModelId::from(NOOP_MODEL),
        tokens: TokenCounts {
            input: 0,
            output: 0,
            cache_hit: None,
            cache_write: None,
        },
        latency_ms: 0,
        events: Vec::new(),
    }
}

/// Deterministic, scriptable adapter for tests and any consumer that needs a
/// byte-stable conversation file across repeated runs.
///
/// Scripts are consumed in call order regardless of input contents; the
/// caller controls determinism through script construction, not through input
/// matching.
///
/// When `auto_fill` is true (see [`Self::auto`]) and the scripted queue is
/// empty, the engine generates `"noop-{call_index}"` text indefinitely rather
/// than returning [`EngineError::NoopExhausted`].
pub struct NoopEngine {
    scripts: Mutex<VecDeque<ScriptEntry>>,
    call_count: AtomicUsize,
    auto_fill: bool,
}

impl NoopEngine {
    /// Build from a fully-formed response queue. Use this when a test must
    /// assert against `Content::Blocks` bodies, populated `Trace.events`, or
    /// `Trace.model == request.model`. The caller-supplied `trace.span_id` is
    /// preserved verbatim.
    #[must_use]
    pub fn from_scripts(scripts: Vec<CompletionResponse>) -> Self {
        Self {
            scripts: Mutex::new(scripts.into_iter().map(ScriptEntry::Fixed).collect()),
            call_count: AtomicUsize::new(0),
            auto_fill: false,
        }
    }

    /// Each input string becomes one served response with `Content::Text`
    /// body and the [`noop_trace`] defaults — `ModelId::from("noop")`, zero
    /// tokens, zero latency, no events, and a span id of the form
    /// `noop-{call_index}` stamped at completion time so distinct calls
    /// always yield distinct span ids.
    ///
    /// Total conversion. Tests that require `Trace.model == request.model`
    /// must use [`Self::from_scripts`].
    #[must_use]
    pub fn from_replies<I, S>(replies: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let scripts = replies
            .into_iter()
            .map(|reply| ScriptEntry::AutoStamp(Content::Text(reply.into())))
            .collect();
        Self {
            scripts: Mutex::new(scripts),
            call_count: AtomicUsize::new(0),
            auto_fill: false,
        }
    }

    /// Generate `"noop-{call_index}"` replies indefinitely. Used by
    /// [`open_engine_for_model`] when the conversation's model id is `"noop"`,
    /// so integration tests can call `run` without injecting an engine.
    #[must_use]
    pub fn auto() -> Self {
        Self {
            scripts: Mutex::new(VecDeque::new()),
            call_count: AtomicUsize::new(0),
            auto_fill: true,
        }
    }
}

#[async_trait::async_trait]
impl EngineProvider for NoopEngine {
    async fn complete(
        &self,
        _request: CompletionRequest<'_>,
    ) -> Result<CompletionResponse, EngineError> {
        let index = self.call_count.load(Ordering::Relaxed);
        let popped = {
            let mut guard = self
                .scripts
                .lock()
                .expect("NoopEngine script queue mutex poisoned");
            guard.pop_front()
        };
        let entry = match popped {
            Some(entry) => entry,
            None if self.auto_fill => {
                ScriptEntry::AutoStamp(Content::Text(format!("{NOOP_MODEL}-{index}")))
            }
            None => return Err(EngineError::NoopExhausted { call_index: index }),
        };
        let response = match entry {
            ScriptEntry::Fixed(response) => response,
            ScriptEntry::AutoStamp(content) => CompletionResponse {
                content,
                trace: noop_trace(index),
            },
        };
        self.call_count.fetch_add(1, Ordering::Relaxed);
        Ok(response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Compile-time: `EngineProvider` is dyn-compatible.
    const _: fn(&dyn EngineProvider) = |_| {};

    // Compile-time: `NoopEngine` is `Send + Sync`.
    const _: fn() = || {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<NoopEngine>();
    };

    fn request() -> CompletionRequest<'static> {
        CompletionRequest {
            model: ModelId::from("claude-opus-4-7"),
            messages: &[],
            debug: false,
        }
    }

    #[tokio::test]
    async fn from_replies_serves_scripts_in_order_with_per_call_span_ids() {
        // Arrange
        let engine = NoopEngine::from_replies(["a", "b"]);

        // Act
        let first = engine.complete(request()).await.expect("first call serves");
        let second = engine
            .complete(request())
            .await
            .expect("second call serves");

        // Assert
        match first.content {
            Content::Text(ref text) => assert_eq!(text, "a"),
            Content::Blocks(_) => panic!("from_replies must produce Content::Text"),
        }
        match second.content {
            Content::Text(ref text) => assert_eq!(text, "b"),
            Content::Blocks(_) => panic!("from_replies must produce Content::Text"),
        }
        assert_eq!(first.trace.span_id, SpanId::from("noop-0"));
        assert_eq!(second.trace.span_id, SpanId::from("noop-1"));
        assert_eq!(first.trace.model, ModelId::from("noop"));
        assert_eq!(first.trace.tokens.input, 0);
        assert_eq!(first.trace.tokens.output, 0);
        assert_eq!(first.trace.latency_ms, 0);
        assert!(first.trace.events.is_empty());
    }

    #[tokio::test]
    async fn exhausted_script_returns_call_index_without_advancing_counter() {
        // Arrange
        let engine = NoopEngine::from_replies(["only"]);
        let _ = engine.complete(request()).await.expect("first call serves");

        // Act
        let err_first = engine
            .complete(request())
            .await
            .expect_err("queue empty on second call");
        let err_second = engine
            .complete(request())
            .await
            .expect_err("queue still empty on third call");

        // Assert
        match err_first {
            EngineError::NoopExhausted { call_index } => assert_eq!(call_index, 1),
            EngineError::Auth { .. }
            | EngineError::RateLimited { .. }
            | EngineError::Timeout
            | EngineError::ModelNotFound { .. }
            | EngineError::MalformedResponse { .. }
            | EngineError::Provider { .. } => {
                panic!("expected NoopExhausted, got non-noop variant")
            }
        }
        match err_second {
            EngineError::NoopExhausted { call_index } => assert_eq!(call_index, 1),
            EngineError::Auth { .. }
            | EngineError::RateLimited { .. }
            | EngineError::Timeout
            | EngineError::ModelNotFound { .. }
            | EngineError::MalformedResponse { .. }
            | EngineError::Provider { .. } => {
                panic!("expected NoopExhausted, got non-noop variant")
            }
        }
    }

    #[tokio::test]
    async fn from_scripts_preserves_caller_supplied_blocks_and_trace_fields() {
        // Arrange
        let span = SpanId::from("caller-span-xyz");
        let model = ModelId::from("claude-sonnet-4-6");
        let blocks = Content::Blocks(Vec::new());
        let trace = Trace {
            span_id: span.clone(),
            model: model.clone(),
            tokens: TokenCounts {
                input: 17,
                output: 42,
                cache_hit: Some(3),
                cache_write: None,
            },
            latency_ms: 1234,
            events: vec![crate::content::conversation::TraceEvent {
                name: String::from("gen_ai.completion"),
                attributes: std::collections::BTreeMap::new(),
            }],
        };
        let engine = NoopEngine::from_scripts(vec![CompletionResponse {
            content: blocks,
            trace,
        }]);

        // Act
        let response = engine
            .complete(request())
            .await
            .expect("scripted call serves");

        // Assert
        assert!(matches!(response.content, Content::Blocks(_)));
        assert_eq!(response.trace.span_id, span);
        assert_eq!(response.trace.model, model);
        assert_eq!(response.trace.tokens.output, 42);
        assert_eq!(response.trace.tokens.cache_hit, Some(3));
        assert_eq!(response.trace.latency_ms, 1234);
        assert_eq!(response.trace.events.len(), 1);
    }

    #[test]
    fn open_engine_for_model_claude_prefix_returns_anthropic_engine_or_auth_error() {
        // SAFETY: tests run single-threaded against env vars by convention; the
        // factory's claude-* branch is exercised by observing Auth or success.
        // Clear the key to force the deterministic Auth path; restore after.
        let saved = std::env::var("ANTHROPIC_API_KEY").ok();
        // SAFETY: setting/removing env vars; documented unsafe in 2024 edition.
        unsafe {
            std::env::remove_var("ANTHROPIC_API_KEY");
        }

        let result = open_engine_for_model(&ModelId::from("claude-opus-4-7"));

        // SAFETY: restoring previous value.
        unsafe {
            match saved {
                Some(v) => std::env::set_var("ANTHROPIC_API_KEY", v),
                None => std::env::remove_var("ANTHROPIC_API_KEY"),
            }
        }

        match result {
            Err(EngineError::Auth { .. }) => {}
            Err(other) => panic!("expected Auth, got {other:?}"),
            Ok(_) => panic!("expected Auth with no key in environment"),
        }
    }

    #[test]
    fn is_noop_model_matches_only_the_noop_literal() {
        assert!(is_noop_model(&ModelId::from("noop")));
        assert!(!is_noop_model(&ModelId::from("claude-opus-4-7")));
        assert!(!is_noop_model(&ModelId::from("noop-extra")));
    }

    #[test]
    fn open_engine_for_model_unrecognised_prefix_returns_model_not_found() {
        // `mistral-large` matches no wired provider family (claude-/gpt-/gemini-),
        // so it pins the `else -> ModelNotFound` fallthrough. A recognised-but-
        // keyless id like `gpt-5-turbo` instead reaches its constructor and
        // fails with `Auth`; that distinction is covered by the routing feature
        // test and the constructor-boundary unit tests.
        let requested = ModelId::from("mistral-large");
        match open_engine_for_model(&requested) {
            Err(EngineError::ModelNotFound { model }) => assert_eq!(model, requested),
            Err(other) => panic!("expected ModelNotFound, got {other:?}"),
            Ok(_) => panic!("an unrecognised prefix must not resolve to an engine"),
        }
    }
}
