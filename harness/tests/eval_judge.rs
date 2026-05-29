//! Feature test for the `eval-judge` slice.
//!
//! User story: an eval-suite author declares an `Assertion::Judge` with a
//! rubric and runs `ailly eval` against a run whose conversation is already
//! filled. With an engine wired into the `EvaluationContext`, the judge call
//! goes out to the model, the verdict is parsed from the reply, and the
//! report tallies the judge in the `passed` bucket rather than deferring.
//! The judge call is also persisted as a regular Ailly conversation file so
//! it can be diff-reviewed across runs.
//!
//! This test drives the happy-path end of the `Judge` contract:
//!
//!   - Engine present in the context. The judge dispatch must not be the
//!     blanket `Deferred` arm any longer; a real `complete` call goes out.
//!   - The reply ends `GRADE: P`. The greedy-last regex pulls the `P` verdict
//!     and the orchestrator records `AssertionOutcome::Pass`.
//!   - The per-class rollup under `"judge"` shows `passed: 1`, the top-level
//!     totals show `passed: 1` and `deferred: 0`.
//!   - The judge transcript lands under the supplied judge output directory as
//!     `<conv-stem>.<case-tag>.assertion-<M>.yaml`. The file parses as a normal
//!     `Conversation`, carries the rubric verbatim, and carries the scripted
//!     reply text on the assistant turn so an operator can read what the judge
//!     actually said.
//!
//! Failure-mode variations (no GRADE, inconclusive, engine error, fan-out
//! tagging, reserved-name validation) are covered by red-green-refactor unit
//! tests inside `src/knowledge/assertions.rs` and `src/content/evaluation.rs`
//! once this happy path is green.

use std::marker::PhantomData;
use std::path::PathBuf;

use ailly_two::content::conversation::BindingMap;
use ailly_two::content::conversation::Content;
use ailly_two::content::conversation::Conversation;
use ailly_two::content::conversation::Message;
use ailly_two::content::conversation::Meta;
use ailly_two::content::conversation::ModelId;
use ailly_two::content::conversation::Role;
use ailly_two::content::evaluation::Evaluation;
use ailly_two::engine::engine::NoopEngine;
use ailly_two::knowledge::assertions::EvaluationContext;
use ailly_two::knowledge::eval::EvalArgs;
use ailly_two::knowledge::eval::evaluate;

/// One-case regression suite carrying a single `judge` rubric. The rubric
/// text is asserted-on by the file-persistence check below, so the verbatim
/// string matters.
const SUITE_YAML: &str = "\
name: regression
cases:
  - name: over-limit
    assertions:
      - type: judge
        prompt: \"the candidate response cites policy number C-1\"
";

/// Scripted judge reply. The leading chain-of-thought is the kind of prefix a
/// real frontier model would emit; the binding `GRADE: P` line is the last
/// `GRADE:` occurrence and so wins under the greedy-last regex documented in
/// the design.
const JUDGE_REPLY: &str = "The candidate cites policy C-1 in its second sentence and routes the claim to human-review, satisfying the rubric.\nGRADE: P";

#[tokio::test]
async fn judge_assertion_with_grade_p_reply_tallies_pass_and_writes_judge_transcript() {
    // Arrange: suite, the conversation under judgement, and the engine.
    // The conversation's assistant turn is the *candidate response* the
    // judge will be asked to grade; the user turn provides the question.
    let suite = Evaluation::from_yaml_str(SUITE_YAML).expect("regression suite parses");
    let conv_path = PathBuf::from("over-limit.yaml");
    let conv = conversation_with(vec![
        user_text("claim narrative: policy C-1, requested payout 12,500 USD"),
        assistant_text(
            "Per policy C-1 the requested payout exceeds the auto-approve threshold. \
             Routing this claim to human-review.",
        ),
    ]);

    let judge_dir = tempfile::tempdir().expect("tempdir for judge transcripts");
    let engine = NoopEngine::from_replies([JUDGE_REPLY]);
    let ctx = EvaluationContext {
        engine: Some(&engine),
    };

    // Act: run the orchestrator with the judge output directory wired in.
    // `judge_output_dir` is the new field this slice introduces on
    // `EvalArgs`; the CLI populates it with `<project>/evals/judges/<run-id>/`.
    let report = evaluate(EvalArgs {
        suite: &suite,
        conversations: &[(conv_path, conv)],
        ctx,
        suite_name: "regression",
        run_id: "2026-05-29T14-00-handler",
        judge_output_dir: Some(judge_dir.path()),
    })
    .await;

    // Assert: the judge no longer defers — it passes.
    assert_eq!(
        report.totals.assertions.passed, 1,
        "expected one judge pass, observed totals {:#?}",
        report.totals.assertions,
    );
    assert_eq!(
        report.totals.assertions.deferred, 0,
        "no judge may defer when an engine is wired",
    );
    let judge_bucket = report
        .per_class
        .get("judge")
        .expect("per-class rollup must include the `judge` class once a judge is wired");
    assert_eq!(
        judge_bucket.passed, 1,
        "judge per-class bucket expected one pass, got {judge_bucket:#?}",
    );
    assert_eq!(judge_bucket.deferred, 0);

    // Assert: the judge transcript exists at the per-design path
    //   `<judge_output_dir>/<conv-stem>.<case-tag>.assertion-<M>.yaml`.
    // The conversation stem is `over-limit`, the case name is `over-limit`,
    // and the judge is the zero-indexed assertion in the case.
    let judge_file = judge_dir
        .path()
        .join("over-limit.over-limit.assertion-0.yaml");
    assert!(
        judge_file.exists(),
        "judge transcript missing at {}",
        judge_file.display(),
    );

    // Assert: the transcript is a real Ailly conversation file and carries
    // the rubric verbatim plus the scripted reply on the assistant turn.
    let raw = std::fs::read_to_string(&judge_file).expect("read persisted judge transcript");
    assert!(
        raw.contains("the candidate response cites policy number C-1"),
        "judge transcript must replay the rubric verbatim, got:\n{raw}",
    );
    assert!(
        raw.contains("GRADE: P"),
        "judge transcript must record the model's reply on the assistant turn, got:\n{raw}",
    );

    let parsed = Conversation::from_yaml_str(&raw)
        .expect("persisted judge transcript must round-trip as a Conversation");
    assert!(
        parsed
            .session
            .iter()
            .any(|m| matches!(m.role, Role::System)),
        "judge transcript must begin with the judge system message",
    );
    let has_filled_assistant = parsed.session.iter().any(|m| {
        matches!(m.role, Role::Assistant)
            && matches!(
                m.body.as_ref(),
                Some(Content::Text(t)) if t.contains("GRADE: P")
            )
    });
    assert!(
        has_filled_assistant,
        "judge transcript must carry the model reply on a filled assistant turn",
    );
}

fn conversation_with(session: Vec<Message>) -> Conversation {
    Conversation {
        meta: Meta {
            model: ModelId::from("noop"),
            debug: false,
            assembly: None,
            binding: BindingMap::new(),
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
