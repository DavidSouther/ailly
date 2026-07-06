//! Feature test for Feature F — `report`'s statistical-rigor upgrade.
//! See `.ailly/developer/2026-07-06-A-ailly-evals/feature-f-report-stats/
//! design.md`.
//!
//! Two independent capabilities land together in this feature-step:
//!
//! Story 1 (paired-difference + SEM, Closing Bell Task 5 — "Is this
//! regression real?"): `compute_comparison` already buckets every paired
//! assertion into Improved/Regressed/UnchangedPass/UnchangedFail
//! (`src/knowledge/report.rs`). This feature-step folds those same paired
//! outcomes into a two-tailed paired Student's t-test (mean difference,
//! sample standard deviation, standard error of the mean, t-statistic,
//! degrees of freedom, p-value, and a significance verdict at α = 0.05) so a
//! developer gets an actual statistical answer instead of raw counts to
//! eyeball.
//!
//! Story 2 (`tool_call_collection`): a suite author wants "these tools were
//! all called, in any order" instead of `tool_call_order`'s rigid
//! subsequence match — Anthropic's own guidance against over-rigid
//! tool-call-order assertions, cited in the parent project's research.md.
//! `tool_call_collection` reuses `extract_tool_uses`/`tool_use_name` (the
//! same helpers `tool_call_order` already uses) as a multiset comparison:
//! each named tool must appear at least as many times as requested,
//! regardless of order or intervening calls, alongside (not replacing)
//! `tool_call_order`.
//!
//! This file pins the happy path for both. Edge cases (insufficient pairs,
//! a zero-variance/vacuous comparison, a perfect-separation comparison, an
//! empty `tools` list) are Plan-phase red-green-refactor unit tests inside
//! `src/knowledge/report.rs` and `src/knowledge/assertions.rs`, matching
//! this repo's existing split between a feature test's public contract and
//! a module's own unit-test coverage (see `tests/eval_judge.rs`'s own doc
//! comment for the same pattern).

use std::marker::PhantomData;

use ailly_two::content::conversation::BindingMap;
use ailly_two::content::conversation::Content;
use ailly_two::content::conversation::ContentBlock;
use ailly_two::content::conversation::Conversation;
use ailly_two::content::conversation::Message;
use ailly_two::content::conversation::Meta;
use ailly_two::content::conversation::ModelId;
use ailly_two::content::conversation::Role;
use ailly_two::content::conversation::ToolUseId;
use ailly_two::content::evaluation::Assertion;
use ailly_two::knowledge::assertions::AssertionOutcome;
use ailly_two::knowledge::assertions::EvaluationContext;
use ailly_two::knowledge::eval::EvalReport;
use ailly_two::knowledge::report::PairedDifferenceTest;
use ailly_two::knowledge::report::compute_comparison;

/// Reuses `tests/report_cmd.rs`'s exact `ARM_A`/`ARM_B` assertion split (3
/// improved, 1 regressed, 4 unchanged-pass, 2 unchanged-fail — 10 paired
/// assertions total, already proven and green in that file) so the
/// paired-difference numbers asserted below cross-check against an
/// already-verified fixture instead of an unvetted new one. Only the
/// fields `compute_comparison` actually reads (`cases`) carry real data;
/// `totals`/`per_class` are present-but-empty because `EvalReport`
/// requires them to deserialize, not because this test exercises them.
const ARM_A_JSON: &str = r#"{
  "suite": "regression",
  "run_id": "2026-05-20T10-00-00Z-baseline",
  "totals": { "conversations_matched": 2, "assertions": { "passed": 5, "failed": 5, "deferred": 0, "malformed": 0 } },
  "per_class": {},
  "cases": [
    { "name": "missing-fields", "matches": [ { "conversation": "missing-fields.yaml", "assertions": [
      { "class": "must_call_tool",     "outcome": "fail" },
      { "class": "text_contains",      "outcome": "fail" },
      { "class": "text_equals",        "outcome": "fail" },
      { "class": "must_not_call_tool", "outcome": "pass" },
      { "class": "tool_call_count",    "outcome": "pass" }
    ] } ] },
    { "name": "over-limit", "matches": [ { "conversation": "over-limit.yaml", "assertions": [
      { "class": "tool_call_order", "outcome": "pass" },
      { "class": "text_contains",   "outcome": "fail" },
      { "class": "text_equals",     "outcome": "fail" },
      { "class": "judge",           "outcome": "pass" },
      { "class": "latency_ms",      "outcome": "pass" }
    ] } ] }
  ]
}"#;

const ARM_B_JSON: &str = r#"{
  "suite": "regression",
  "run_id": "2026-05-27T14-43-31Z-target",
  "totals": { "conversations_matched": 2, "assertions": { "passed": 7, "failed": 3, "deferred": 0, "malformed": 0 } },
  "per_class": {},
  "cases": [
    { "name": "missing-fields", "matches": [ { "conversation": "missing-fields.yaml", "assertions": [
      { "class": "must_call_tool",     "outcome": "pass" },
      { "class": "text_contains",      "outcome": "pass" },
      { "class": "text_equals",        "outcome": "pass" },
      { "class": "must_not_call_tool", "outcome": "pass" },
      { "class": "tool_call_count",    "outcome": "pass" }
    ] } ] },
    { "name": "over-limit", "matches": [ { "conversation": "over-limit.yaml", "assertions": [
      { "class": "tool_call_order", "outcome": "fail" },
      { "class": "text_contains",   "outcome": "fail" },
      { "class": "text_equals",     "outcome": "fail" },
      { "class": "judge",           "outcome": "pass" },
      { "class": "latency_ms",      "outcome": "pass" }
    ] } ] }
  ]
}"#;

/// Given two runs whose paired assertions already bucket into 3 improved,
/// 1 regressed, 4 unchanged-pass, 2 unchanged-fail (10 pairs; diffs
/// = [+1,+1,+1,-1,0,0,0,0,0,0] using the "`arm_b` relative to `arm_a`" sign
/// convention: pass=1, fail=0, diff = b - a):
///
/// When `compute_comparison` runs,
///
/// Then it reports the existing bucket totals unchanged (regression guard)
/// AND a paired-difference test computed from the same 10 pairs: mean
/// difference 0.2, sample std dev sqrt(0.4), SEM = sqrt(0.4)/sqrt(10) = 0.2
/// exactly, t = mean/SEM = 1.0 exactly, df = 9, two-tailed p ≈ 0.3434 (not
/// significant at α = 0.05 — a 30%-ish swing on 10 pairs is exactly the
/// kind of small-sample noise this project's own research.md warns a raw
/// bucket count can't distinguish from a real regression).
#[test]
fn compute_comparison_reports_paired_difference_test_with_standard_error() {
    // Given
    let arm_a: EvalReport = serde_json::from_str(ARM_A_JSON).expect("arm_a parses");
    let arm_b: EvalReport = serde_json::from_str(ARM_B_JSON).expect("arm_b parses");

    // When
    let comparison = compute_comparison(&arm_a, &arm_b);

    // Then: existing bucket counts are unchanged.
    assert_eq!(
        comparison.totals.improved, 3,
        "3 improved (regression guard)"
    );
    assert_eq!(
        comparison.totals.regressed, 1,
        "1 regressed (regression guard)"
    );
    assert_eq!(
        comparison.totals.unchanged_pass, 4,
        "4 unchanged pass (regression guard)"
    );
    assert_eq!(
        comparison.totals.unchanged_fail, 2,
        "2 unchanged fail (regression guard)"
    );

    // Then: the new paired-difference test is computed from those same 10 pairs.
    match comparison.paired_difference {
        PairedDifferenceTest::Computed {
            n,
            mean_difference,
            standard_error,
            degrees_of_freedom,
            t_statistic,
            p_value,
            significant,
            ..
        } => {
            assert_eq!(n, 10, "10 paired assertions feed the test");
            assert!(
                (mean_difference - 0.2).abs() < 1e-9,
                "mean difference: expected 0.2, got {mean_difference}"
            );
            assert!(
                (standard_error - 0.2).abs() < 1e-9,
                "SEM: expected 0.2, got {standard_error}"
            );
            assert_eq!(degrees_of_freedom, 9, "df = n - 1 = 9");
            let t = t_statistic.expect("variance is nonzero here; t-statistic is defined");
            assert!(
                (t - 1.0).abs() < 1e-9,
                "t = mean_difference / SEM = 1.0 exactly, got {t}"
            );
            assert!(
                (p_value - 0.3434).abs() < 1e-3,
                "two-tailed p for t=1.0, df=9 is ~0.3434 (must be a real Student's-t \
                 computation: a one-tailed value would read ~0.172, a normal/z \
                 approximation would read ~0.317, an off-by-one df=10 bug would read \
                 ~0.341 — all outside this tolerance), got {p_value}"
            );
            assert!(
                !significant,
                "p ~0.34 does not clear the α = 0.05 significance bar"
            );
        }
        insufficient @ PairedDifferenceTest::InsufficientPairs { .. } => {
            panic!("expected PairedDifferenceTest::Computed, got {insufficient:?}")
        }
    }
}

/// Given an assistant turn that calls `lookup_claim_history` once and
/// `lookup_policy` twice, in that order (the reverse order, and a
/// different multiplicity, than a hand-written `tool_call_order` sequence
/// would name),
///
/// When a `tool_call_collection` assertion requires the same multiset
/// (`lookup_policy` twice, `lookup_claim_history` once) but lists it in the
/// opposite order,
///
/// Then it passes — proving the check is order-insensitive, unlike
/// `tool_call_order`. And when a second assertion requires a third
/// `lookup_policy` call that was never made, it fails, naming the tool and
/// both the required and observed counts.
#[tokio::test]
async fn tool_call_collection_passes_regardless_of_order_and_fails_on_missing_calls() {
    // Given
    let ctx = EvaluationContext::empty();
    let conv = conversation_with(vec![assistant_blocks(vec![
        tool_use("lookup_claim_history"),
        tool_use("lookup_policy"),
        tool_use("lookup_policy"),
    ])]);

    // When: requires the same multiset, listed in the opposite order.
    let passing = Assertion::ToolCallCollection {
        tools: vec![
            String::from("lookup_policy"),
            String::from("lookup_policy"),
            String::from("lookup_claim_history"),
        ],
    };

    // Then: passes — order-insensitive.
    assert_eq!(passing.check(&conv, &ctx).await, AssertionOutcome::Pass);

    // When: requires a third lookup_policy call that was never made.
    let failing = Assertion::ToolCallCollection {
        tools: vec![
            String::from("lookup_policy"),
            String::from("lookup_policy"),
            String::from("lookup_policy"),
        ],
    };

    // Then: fails, naming the tool and both the required (3) and observed
    // (2) counts.
    match failing.check(&conv, &ctx).await {
        AssertionOutcome::Fail { reason } => {
            assert!(reason.contains("lookup_policy"), "got {reason}");
            assert!(
                reason.contains('3'),
                "reason should name the required count: {reason}"
            );
            assert!(
                reason.contains('2'),
                "reason should name the observed count: {reason}"
            );
        }
        other => panic!("expected Fail, got {other:?}"),
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

fn assistant_blocks(blocks: Vec<ContentBlock>) -> Message {
    Message {
        role: Role::Assistant,
        body: Some(Content::Blocks(blocks)),
        cache: false,
        trace: None,
        _phase: PhantomData,
    }
}

fn tool_use(name: &str) -> ContentBlock {
    ContentBlock::ToolUse {
        id: ToolUseId::from("tool_1"),
        name: String::from(name),
        input: serde_yaml_ng::Value::Null,
    }
}
