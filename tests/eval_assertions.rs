//! Feature test for `eval-assertions-core`.
//!
//! User story: an eval-suite author writes `evals/regression.yaml` with
//! assertions spanning the four no-LLM families (text, tool-call,
//! structural, performance) and one LLM-backed family (judge). After
//! `ailly run` fills a conversation, the author asks each assertion in
//! the suite to check the conversation. Every in-scope assertion returns
//! a per-assertion verdict (`Pass`, or `Fail` with a reason string that
//! names the expectation) without contacting any engine, tool registry,
//! or script runtime. The `judge` variant returns `Deferred` because no
//! engine collaborator was supplied. A deliberately invalid regex returns
//! `Malformed`, distinct from `Fail`, so a typo in the suite surfaces as
//! an authoring bug rather than a regression.
//!
//! This test drives the four-outcome public contract end-to-end:
//! `Pass`, `Fail { reason }`, `Deferred`, `Malformed { reason }`. Per-
//! variant pass/fail coverage for every one of the twelve in-scope
//! variants lives in the unit tests inside `src/knowledge/assertions.rs`
//! per the design doc.

use std::marker::PhantomData;

use ailly_two::content::conversation::BindingMap;
use ailly_two::content::conversation::Content;
use ailly_two::content::conversation::ContentBlock;
use ailly_two::content::conversation::Conversation;
use ailly_two::content::conversation::Message;
use ailly_two::content::conversation::Meta;
use ailly_two::content::conversation::ModelId;
use ailly_two::content::conversation::Role;
use ailly_two::content::conversation::SpanId;
use ailly_two::content::conversation::TokenCounts;
use ailly_two::content::conversation::ToolUseId;
use ailly_two::content::conversation::Trace;
use ailly_two::content::evaluation::Evaluation;
use ailly_two::knowledge::assertions::AssertionOutcome;
use ailly_two::knowledge::assertions::EvaluationContext;

/// Regression-suite fixture covering one assertion per in-scope family
/// (text, tool-call, structural, performance) plus one `judge` variant
/// to demonstrate the `Deferred` outcome.
const REGRESSION_YAML: &str = "\
name: feature
cases:
  - name: missing-fields
    assertions:
      - { type: must_call_tool, tool: lookup_policy }
      - { type: must_not_call_tool, tool: auto_approve }
      - { type: tool_call_count, op: \"<=\", value: 2 }
      - { type: tool_call_order, sequence: [lookup_policy] }
      - { type: text_contains, value: \"policy number required\" }
      - { type: text_not_contains, value: \"auto-approve\" }
      - { type: text_matches, pattern: \"polic\\\\w+\", flags: \"i\" }
      - { type: text_equals, value: \"policy number required to validate the claim.\" }
      - { type: json_path, path: \"$.session[0].role\", op: \"==\", value: user }
      - { type: response_field, path: \"$.session[1].trace\", exists: true }
      - { type: tokens, metric: total, op: \"<\", value: 8000 }
      - { type: latency_ms, op: \"<=\", value: 500 }
      - type: judge
        prompt: \"the response routes to human-review\"
";

const MALFORMED_REGEX_YAML: &str = "\
name: bad
cases:
  - assertions:
      - { type: text_matches, pattern: \"[unclosed\" }
";

#[tokio::test]
async fn regression_suite_yields_per_assertion_verdicts_against_a_hand_built_conversation() {
    // Arrange: parse the suite the eval author hand-wrote, and build the
    // empty context the 12 sync families need (the judge variant will
    // see no engine and defer).
    let suite = Evaluation::from_yaml_str(REGRESSION_YAML).expect("regression fixture parses");
    let case = &suite.cases[0];
    let ctx = EvaluationContext::empty();

    // Arrange: build a conversation that satisfies every in-scope
    // assertion in `case`. The assistant turn calls `lookup_policy` and
    // replies with the documented prompt-for-policy-number text; inline
    // trace carries the token and latency numbers the performance
    // assertions sum.
    let passing = conversation_with(vec![
        user_text("claim narrative omits the policy number"),
        assistant_blocks(
            vec![
                ContentBlock::ToolUse {
                    id: ToolUseId::from("tool_1"),
                    name: String::from("lookup_policy"),
                    input: serde_yaml_ng::from_str("{ claim_id: C-1 }").expect("valid yaml"),
                },
                ContentBlock::Text {
                    text: String::from("policy number required to validate the claim."),
                },
            ],
            Some(trace(1200, 300, 450)),
        ),
    ]);

    // Act: ask each assertion in the suite for its verdict against the
    // passing conversation.
    let mut outcomes = Vec::with_capacity(case.assertions.len());
    for assertion in &case.assertions {
        outcomes.push(assertion.check(&passing, &ctx).await);
    }

    // Assert: the twelve in-scope assertions all Pass; indexes follow
    // the YAML order above.
    for (index, outcome) in outcomes.iter().enumerate().take(12) {
        assert_eq!(
            *outcome,
            AssertionOutcome::Pass,
            "assertion {index} ({:?}) expected Pass, got {outcome:?}",
            case.assertions[index],
        );
    }

    // Assert: the `judge` variant defers, because the empty context has
    // no engine the assertion could ask. The orchestrator wires an
    // engine later; this layer never invents one.
    assert_eq!(
        outcomes[12],
        AssertionOutcome::Deferred,
        "judge assertion must defer when no engine is supplied",
    );

    // Arrange: now build a conversation that violates the text, tool-
    // call, and performance assertions in well-named ways.
    let failing = conversation_with(vec![
        user_text("claim narrative omits the policy number"),
        assistant_blocks(
            vec![ContentBlock::Text {
                text: String::from("auto-approve: claim looks fine."),
            }],
            Some(trace(9000, 0, 9999)),
        ),
    ]);

    // Act + Assert: representative assertions across the families now
    // Fail, and each `reason` names the expectation that was violated.
    // (Per-variant exhaustive pass/fail coverage lives in the unit
    // tests; this slice proves the four-outcome public contract.)
    let must_call = case.assertions[0].check(&failing, &ctx).await;
    assert_fail_reason_contains(&must_call, "lookup_policy");

    let text_contains = case.assertions[4].check(&failing, &ctx).await;
    assert_fail_reason_contains(&text_contains, "policy number required");

    let text_not_contains = case.assertions[5].check(&failing, &ctx).await;
    assert_fail_reason_contains(&text_not_contains, "auto-approve");

    let text_equals = case.assertions[7].check(&failing, &ctx).await;
    assert_fail_reason_contains(&text_equals, "policy number required");

    let tokens = case.assertions[10].check(&failing, &ctx).await;
    assert_fail_reason_contains(&tokens, "9000");

    let latency = case.assertions[11].check(&failing, &ctx).await;
    assert_fail_reason_contains(&latency, "9999");

    // Arrange + Act + Assert: a deliberately invalid regex surfaces as
    // `Malformed`, distinct from `Fail`, so a suite-authoring bug does
    // not masquerade as a regression in the run.
    let bad =
        Evaluation::from_yaml_str(MALFORMED_REGEX_YAML).expect("malformed-regex suite parses");
    let outcome = bad.cases[0].assertions[0].check(&passing, &ctx).await;
    assert!(
        matches!(outcome, AssertionOutcome::Malformed { .. }),
        "invalid regex must produce Malformed, got {outcome:?}",
    );
}

fn assert_fail_reason_contains(outcome: &AssertionOutcome, needle: &str) {
    match outcome {
        AssertionOutcome::Fail { reason } => assert!(
            reason.contains(needle),
            "fail reason {reason:?} must mention {needle:?}",
        ),
        other => panic!("expected Fail with reason containing {needle:?}, got {other:?}"),
    }
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

fn assistant_blocks(blocks: Vec<ContentBlock>, trace: Option<Trace>) -> Message {
    Message {
        role: Role::Assistant,
        body: Some(Content::Blocks(blocks)),
        cache: false,
        trace,
        _phase: PhantomData,
    }
}

fn trace(input: u64, output: u64, latency_ms: u64) -> Trace {
    Trace {
        span_id: SpanId::from("span-1"),
        model: ModelId::from("noop"),
        tokens: TokenCounts {
            input,
            output,
            cache_hit: None,
            cache_write: None,
        },
        latency_ms,
        events: Vec::new(),
    }
}
