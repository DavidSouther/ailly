//! Feature test for the judge-calibration harness (project
//! `2026-07-06-A-ailly-evals`, feature-step E).
//!
//! User story: a developer is not sure whether to trust the `judge`
//! assertion's verdicts. They run the calibration harness over a set of
//! human-labeled examples (each a candidate response with a known-correct
//! pass/fail verdict) and get back an agreement rate plus a clear signal on
//! whether it clears the industry-cited ≥90% bar before an LLM judge should
//! be trusted for release decisions.
//!
//! This test pins the harness's own correctness, not a live judge's actual
//! calibration (that is an empirical, non-deterministic outcome of a real
//! model call — see design.md "User Journey and Metrics"). It scripts a
//! `NoopEngine` with ten known `GRADE:` replies standing in for a judge
//! model, drives the existing, unmodified `evaluate()` orchestrator (exactly
//! as `tests/eval_judge.rs` does), and asserts that the new
//! `knowledge::calibration::compute_calibration` folds the resulting
//! `EvalReport` against a human-labeled set into the correct agreement rate
//! and bar verdict — including the inclusive `>=` boundary at exactly 90%.
//!
//! Currently RED: `ailly_two::knowledge::calibration` does not exist yet.

use std::collections::BTreeMap;
use std::marker::PhantomData;

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
use ailly_two::knowledge::calibration::Agreement;
use ailly_two::knowledge::calibration::HumanVerdict;
use ailly_two::knowledge::calibration::compute_calibration;
use ailly_two::knowledge::eval::EvalArgs;
use ailly_two::knowledge::eval::evaluate;

/// Ten labeled examples, one case each, each with a single `judge` rubric.
/// The rubric text is the same across cases for brevity; only the candidate
/// response (the conversation's assistant turn) and the human label vary.
const SUITE_YAML: &str = "\
name: judge-calibration
cases:
  - name: example-01
    assertions:
      - { type: judge, prompt: \"the candidate directly answers the user's question\" }
  - name: example-02
    assertions:
      - { type: judge, prompt: \"the candidate directly answers the user's question\" }
  - name: example-03
    assertions:
      - { type: judge, prompt: \"the candidate directly answers the user's question\" }
  - name: example-04
    assertions:
      - { type: judge, prompt: \"the candidate directly answers the user's question\" }
  - name: example-05
    assertions:
      - { type: judge, prompt: \"the candidate directly answers the user's question\" }
  - name: example-06
    assertions:
      - { type: judge, prompt: \"the candidate directly answers the user's question\" }
  - name: example-07
    assertions:
      - { type: judge, prompt: \"the candidate directly answers the user's question\" }
  - name: example-08
    assertions:
      - { type: judge, prompt: \"the candidate directly answers the user's question\" }
  - name: example-09
    assertions:
      - { type: judge, prompt: \"the candidate directly answers the user's question\" }
  - name: example-10
    assertions:
      - { type: judge, prompt: \"the candidate directly answers the user's question\" }
";

#[tokio::test]
async fn judge_calibration_harness_computes_agreement_rate_and_flags_bar() {
    // Arrange: ten conversations, in suite/case order. `example-07` is the
    // deliberate disagreement — the scripted judge says `GRADE: P` but the
    // human labeler says the candidate actually failed the rubric. The other
    // nine scripted replies agree with their human label. This lands the
    // fixture at exactly 9/10 = 90%, pinning the inclusive `>=` bar.
    let conversations: Vec<(std::path::PathBuf, Conversation)> = (1..=10)
        .map(|n| {
            let path = std::path::PathBuf::from(format!("example-{n:02}.yaml"));
            let conv = conversation_with(vec![
                user_text(&format!("Question {n}: what does this candidate answer?")),
                assistant_text(&format!("Candidate response body for example {n}.")),
            ]);
            (path, conv)
        })
        .collect();

    // Scripted judge replies, one per example in case order. `example-07`'s
    // scripted `GRADE: P` is the deliberate disagreement against its
    // `HumanVerdict::Fail` label below.
    let scripted_replies = [
        "The candidate answers directly.\nGRADE: P", // example-01: agree (human: pass)
        "The candidate does not address the question.\nGRADE: F", // example-02: agree (human: fail)
        "The candidate answers directly.\nGRADE: P", // example-03: agree (human: pass)
        "The candidate does not address the question.\nGRADE: F", // example-04: agree (human: fail)
        "The candidate answers directly.\nGRADE: P", // example-05: agree (human: pass)
        "The candidate does not address the question.\nGRADE: F", // example-06: agree (human: fail)
        "The candidate answers directly enough.\nGRADE: P", // example-07: DISAGREE (human: fail)
        "The candidate answers directly.\nGRADE: P", // example-08: agree (human: pass)
        "The candidate does not address the question.\nGRADE: F", // example-09: agree (human: fail)
        "The candidate answers directly.\nGRADE: P", // example-10: agree (human: pass)
    ];
    let engine = NoopEngine::from_replies(scripted_replies);

    let mut labels: BTreeMap<String, HumanVerdict> = BTreeMap::new();
    labels.insert(String::from("example-01"), HumanVerdict::Pass);
    labels.insert(String::from("example-02"), HumanVerdict::Fail);
    labels.insert(String::from("example-03"), HumanVerdict::Pass);
    labels.insert(String::from("example-04"), HumanVerdict::Fail);
    labels.insert(String::from("example-05"), HumanVerdict::Pass);
    labels.insert(String::from("example-06"), HumanVerdict::Fail);
    labels.insert(String::from("example-07"), HumanVerdict::Fail); // conflicts with scripted GRADE: P
    labels.insert(String::from("example-08"), HumanVerdict::Pass);
    labels.insert(String::from("example-09"), HumanVerdict::Fail);
    labels.insert(String::from("example-10"), HumanVerdict::Pass);

    let suite = Evaluation::from_yaml_str(SUITE_YAML).expect("judge-calibration suite parses");
    let ctx = EvaluationContext {
        engine: Some(&engine),
        script_runner: None,
        project_root: None,
    };

    // Act 1: drive the existing, unmodified orchestrator to produce a real
    // `EvalReport` — the calibration harness must sit cleanly downstream of
    // today's `evaluate()`, not replace or duplicate it.
    let report = evaluate(EvalArgs {
        suite: &suite,
        conversations: &conversations,
        ctx,
        suite_name: "judge-calibration",
        run_id: "2026-07-06T00-00-00Z-test-judge-calibration",
        judge_output_dir: None,
    })
    .await;

    // Act 2: fold the report against the human labels.
    let calibration = compute_calibration(&report, &labels);

    // Assert: exact counts and the exact agreement rate — a stubbed or
    // partially-correct implementation (e.g. one that always reports
    // `meets_bar: true`, or that miscounts `malformed`/`errored` exclusions)
    // cannot pass this without actually computing 9/10.
    assert_eq!(
        calibration.total_examples, 10,
        "expected all ten examples folded in, got {calibration:#?}"
    );
    assert_eq!(
        calibration.excluded, 0,
        "no example in this fixture is errored/deferred, got {calibration:#?}"
    );
    assert_eq!(calibration.considered, 10);
    assert_eq!(
        calibration.agreements, 9,
        "nine of ten scripted replies agree with their human label, got {calibration:#?}"
    );
    assert!(
        (calibration.agreement_rate - 0.9).abs() < 1e-9,
        "expected agreement_rate == 0.9 exactly, got {}",
        calibration.agreement_rate
    );
    assert!(
        calibration.meets_bar,
        "0.9 must clear the inclusive >= 0.90 bar, got meets_bar = {}",
        calibration.meets_bar
    );

    // Assert: the one deliberate disagreement is identified by id, not just
    // folded anonymously into the aggregate count.
    let example_07 = calibration
        .examples
        .iter()
        .find(|e| e.id == "example-07")
        .expect("example-07 must appear in the per-example breakdown");
    assert_eq!(
        example_07.agreement,
        Agreement::Disagree,
        "example-07's scripted GRADE: P must disagree with its HumanVerdict::Fail label, got {example_07:#?}"
    );

    // Assert: every other example agrees.
    let agree_count = calibration
        .examples
        .iter()
        .filter(|e| e.agreement == Agreement::Agree)
        .count();
    assert_eq!(agree_count, 9);
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
