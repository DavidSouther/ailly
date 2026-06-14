//! Feature test for the `eval: program_outputs` slice.
//!
//! User story: an eval-suite author writes a regression suite whose first case
//! runs a `script`/`program` scorer that prints a result on stdout, and whose
//! final case is a no-`name:`/no-`when:` fanout `judge` that must reason over
//! those scorer outputs instead of re-scoring. When the suite runs, each scorer
//! exits 0 and prints its result, and the orchestrator hands that captured
//! stdout to the fanout judge under a `PROGRAM_OUTPUTS:` heading in the judge's
//! user message — so the judge sees what the earlier scorers printed against
//! the same conversation.
//!
//! This test drives the data path end-to-end through a real `python3` scorer
//! and a request-capturing engine:
//!
//!   - A `script` (Python) scorer exits 0 and prints a known marker on stdout.
//!     The scorer's `Pass` no longer discards that stdout (design metric 2); it
//!     is captured as a side-channel keyed by the conversation it ran on.
//!   - A later no-`when:` fanout `judge` case runs against the SAME
//!     conversation. The orchestrator looks up the accumulated outputs and
//!     passes them into the judge prompt. The text the judge engine receives
//!     contains the marker under a `PROGRAM_OUTPUTS:` heading (design metric
//!     1).
//!
//! The engine here is a local request-capturing `EngineProvider`: the shipped
//! `NoopEngine` discards its request, and capturing the request is exactly what
//! this metric must observe, so the test defines its own recorder (the
//! orchestrator tests already define their own engine/runner helpers). It
//! serves a single `GRADE: P` reply so the judge tallies a pass while its
//! request is recorded for inspection.
//!
//! Empty-accumulator byte-identity (metric 3), per-conversation isolation
//! (metric 4), and the `CheckerCall` capture/redaction unit behaviour (metrics
//! 2/5/7) are covered by red-green-refactor unit tests in
//! `src/knowledge/assertions.rs` and `src/knowledge/eval.rs` once this path is
//! green.

use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::Mutex;

use ailly_two::content::conversation::BindingMap;
use ailly_two::content::conversation::Content;
use ailly_two::content::conversation::Conversation;
use ailly_two::content::conversation::Message;
use ailly_two::content::conversation::Meta;
use ailly_two::content::conversation::ModelId;
use ailly_two::content::conversation::Role;
use ailly_two::content::conversation::SpanId;
use ailly_two::content::conversation::TokenCounts;
use ailly_two::content::conversation::Trace;
use ailly_two::content::evaluation::Evaluation;
use ailly_two::engine::engine::CompletionRequest;
use ailly_two::engine::engine::CompletionResponse;
use ailly_two::engine::engine::EngineError;
use ailly_two::engine::engine::EngineProvider;
use ailly_two::knowledge::assertions::EvaluationContext;
use ailly_two::knowledge::eval::EvalArgs;
use ailly_two::knowledge::eval::evaluate;
use ailly_two::knowledge::script_runner::TokioScriptRunner;

/// Marker the Python scorer prints on stdout. Distinctive so its presence in
/// the judge's recorded request is unambiguous proof the capture flowed
/// through.
const SCORER_MARKER: &str = "CORRUPTION-MARKER-7Q";

/// A scorer `script` that exits 0 and prints the marker on stdout, followed by
/// a no-`when:` fanout `judge` case. The judge fans out onto the same single
/// conversation the scorer ran against, so it must receive the scorer's output.
const SUITE_YAML: &str = "\
name: regression
cases:
  - name: score
    assertions:
      - type: script
        runtime: python
        script:
          contents: |
            import sys
            sys.stdin.read()
            sys.stdout.write(\"CORRUPTION-MARKER-7Q corruption=0\")
            sys.exit(0)
  - assertions:
      - type: judge
        prompt: \"the per-domain scorer reports no corruption; consume program_outputs, do not re-score\"
";

/// A `GRADE: P` reply so the fanout judge tallies a pass; the test inspects the
/// recorded request, not the verdict path.
const JUDGE_REPLY: &str =
    "The attached scorer output reports corruption=0, so the rubric is satisfied.\nGRADE: P";

/// Request-capturing engine. Records the flattened text of every user message
/// it is asked to complete, then serves scripted replies in call order. The
/// shipped `NoopEngine` ignores its request, so this test defines its own
/// recorder — the one observation this metric requires.
struct RecordingEngine {
    replies: Mutex<std::collections::VecDeque<String>>,
    seen_user_text: Mutex<Vec<String>>,
}

impl RecordingEngine {
    fn new<I, S>(replies: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            replies: Mutex::new(replies.into_iter().map(Into::into).collect()),
            seen_user_text: Mutex::new(Vec::new()),
        }
    }

    /// Every user-role message text the engine was handed, in call order.
    fn recorded_user_text(&self) -> Vec<String> {
        self.seen_user_text.lock().expect("recorder mutex").clone()
    }
}

#[async_trait::async_trait]
impl EngineProvider for RecordingEngine {
    async fn complete(
        &self,
        request: CompletionRequest<'_>,
    ) -> Result<CompletionResponse, EngineError> {
        for message in request.messages {
            if message.role != Role::User {
                continue;
            }
            if let Some(Content::Text(text)) = message.body.as_ref() {
                self.seen_user_text
                    .lock()
                    .expect("recorder mutex")
                    .push(text.clone());
            }
        }
        let reply = self
            .replies
            .lock()
            .expect("reply mutex")
            .pop_front()
            .expect("RecordingEngine ran out of scripted replies");
        Ok(CompletionResponse {
            content: Content::Text(reply),
            trace: noop_trace(),
        })
    }
}

#[tokio::test]
async fn fanout_judge_receives_prior_program_output_under_program_outputs_heading() {
    // Arrange: one conversation, a suite whose scorer prints a marker on stdout
    // and whose fanout judge runs afterward against the same conversation, a real
    // python3 runner, and a request-capturing engine.
    let suite = Evaluation::from_yaml_str(SUITE_YAML).expect("regression suite parses");
    let conv_path = PathBuf::from("score.yaml");
    let conv = conversation_with(vec![
        user_text("claim narrative: policy C-1, requested payout 12,500 USD"),
        assistant_text(
            "Per policy C-1 the requested payout is within the documented threshold. \
             Auto-approving this claim.",
        ),
    ]);

    let runner = TokioScriptRunner;
    let project_root = tempfile::tempdir().expect("tempdir for project root");
    let engine = RecordingEngine::new([JUDGE_REPLY]);
    let ctx = EvaluationContext {
        engine: Some(&engine),
        script_runner: Some(&runner),
        project_root: Some(project_root.path()),
    };

    // Act: run the orchestrator. The scorer runs first (suite order), capturing
    // its stdout against `score.yaml`; the fanout judge then runs against the
    // same conversation and must receive that capture.
    let report = evaluate(EvalArgs {
        suite: &suite,
        conversations: &[(conv_path, conv)],
        ctx,
        suite_name: "regression",
        run_id: "2026-06-04T14-00-handler",
        judge_output_dir: None,
    })
    .await;

    // Assert: the scorer ran and passed; the judge ran and passed. Neither
    // deferred, proving the engine and runner were both wired.
    assert_eq!(
        report.totals.assertions.deferred, 0,
        "nothing may defer when engine and runner are wired; totals {:#?}",
        report.totals.assertions,
    );
    assert_eq!(
        report.totals.assertions.passed, 2,
        "the scorer (exit 0) and the judge (GRADE: P) must both pass; totals {:#?}",
        report.totals.assertions,
    );

    // Assert: the judge's user message carried the scorer's stdout under a
    // `PROGRAM_OUTPUTS:` heading. This is the feature: a fanout judge reads the
    // prior case's captured program output for the same conversation.
    let judge_user_text = engine
        .recorded_user_text()
        .into_iter()
        .next()
        .expect("the fanout judge must have issued exactly one engine request");
    assert!(
        judge_user_text.contains("PROGRAM_OUTPUTS:"),
        "judge prompt must carry a PROGRAM_OUTPUTS heading once a prior scorer ran, got:\n{judge_user_text}",
    );
    assert!(
        judge_user_text.contains(SCORER_MARKER),
        "judge prompt must replay the scorer's captured stdout marker {SCORER_MARKER:?}, got:\n{judge_user_text}",
    );
}

fn conversation_with(session: Vec<Message>) -> Conversation {
    Conversation {
        meta: Meta {
            model: ModelId::from("noop"),
            debug: false,
            assembly: None,
            binding: BindingMap::new(),
            tools: Vec::new(),
        },
        session,
    }
}

fn user_text(text: &str) -> Message {
    Message {
        role: Role::User,
        body: Some(Content::Text(String::from(text))),
        cache: false,
        trace: None,
        _phase: PhantomData,
    }
}

fn assistant_text(text: &str) -> Message {
    Message {
        role: Role::Assistant,
        body: Some(Content::Text(String::from(text))),
        cache: false,
        trace: None,
        _phase: PhantomData,
    }
}

fn noop_trace() -> Trace {
    Trace {
        span_id: SpanId::from("recording-0"),
        model: ModelId::from("noop"),
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
