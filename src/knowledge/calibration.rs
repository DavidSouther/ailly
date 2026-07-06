//! Judge calibration harness: fold an `EvalReport` produced by a
//! judge-calibration suite (one case per human-labeled example) against a
//! human-authored label map into an agreement rate and a bar-crossing
//! verdict. Pure and synchronous — no engine, no I/O; mirrors `report.rs`'s
//! split (computation here, persistence is a `cli`-layer concern).

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::Serialize;

use crate::knowledge::eval::EvalReport;

/// Ground truth for one calibration example, authored by a human labeler.
/// Binary by design: a labeler is expected to reach a decision, unlike the
/// judge's own ternary P/F/I (which gives the *judge* an honest "cannot
/// decide" out, not the human ground truth).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HumanVerdict {
    Pass,
    Fail,
}

/// How one example's judge outcome relates to its human label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Agreement {
    Agree,
    Disagree,
    /// Excluded from the rate's denominator: the judge call itself failed
    /// (`Errored`) or never ran (`Deferred`) — an environmental/wiring
    /// problem, not evidence about the judge's grading quality.
    Excluded,
}

/// One example's outcome, folded for reporting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExampleAgreement {
    /// Matches the suite case's `name:`.
    pub id: String,
    pub human_verdict: HumanVerdict,
    /// The report's own
    /// `"pass"`/`"fail"`/`"malformed"`/`"errored"`/`"deferred"`.
    pub judge_outcome: String,
    /// Carried from the report's `AssertionReport.reason`.
    pub reason: Option<String>,
    pub agreement: Agreement,
}

/// The ≥90% bar the parent research found as the industry threshold before
/// trusting an LLM judge for release decisions.
pub const JUDGE_CALIBRATION_BAR: f64 = 0.90;

/// Full calibration report for one `(EvalReport, labels)` pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationReport {
    pub suite: String,
    pub run_id: String,
    pub total_examples: usize,
    pub excluded: usize,
    /// `total_examples - excluded`.
    pub considered: usize,
    pub agreements: usize,
    /// `agreements / considered`; `0.0` when `considered == 0`.
    pub agreement_rate: f64,
    /// `considered > 0 && agreement_rate >= JUDGE_CALIBRATION_BAR`.
    pub meets_bar: bool,
    pub examples: Vec<ExampleAgreement>,
}

/// Rejected shapes for a calibration suite. `compute_calibration` is a
/// library-internal `pub fn`, not an application boundary, so a typed error
/// a caller can match on is the right shape here (matches
/// `EngineError`/`ScriptError`'s convention) rather than a silent skip
/// (which would quietly exclude an example from both the numerator and
/// denominator) or a panic (which would crash the whole calibration run
/// instead of surfacing a precise diagnosis).
#[derive(Debug, thiserror::Error)]
pub enum CalibrationError {
    /// A case matched zero conversations.
    #[error("calibration case {case:?} matched no conversation")]
    NoMatch { case: String },
    /// A case matched more than one conversation (a calibration suite must
    /// be one case per example).
    #[error("calibration case {case:?} matched {count} conversations, expected exactly one")]
    MultipleMatches { case: String, count: usize },
    /// A case's single match carried a number of assertions other than one
    /// (a calibration case must carry exactly one `judge` assertion).
    #[error("calibration case {case:?} carried {count} assertions, expected exactly one")]
    MultipleAssertions { case: String, count: usize },
    /// A case name has no entry in the supplied `labels` map.
    #[error("calibration case {case:?} has no entry in the human-labeled set")]
    MissingLabel { case: String },
}

/// Fold an `EvalReport` produced by a judge-calibration suite (one case per
/// labeled example, each case carrying exactly one `judge` assertion over
/// exactly one matched conversation) against a human-authored label map keyed
/// by case name.
///
/// # Errors
///
/// Returns [`CalibrationError`] when a case does not match the expected
/// one-case/one-match/one-assertion shape, or when a case's name has no
/// entry in `labels`.
pub fn compute_calibration(
    report: &EvalReport,
    labels: &BTreeMap<String, HumanVerdict>,
) -> Result<CalibrationReport, CalibrationError> {
    let mut validated: Vec<ValidatedCase> = Vec::with_capacity(report.cases.len());
    for (index, case) in report.cases.iter().enumerate() {
        let case_name = case_identifier(case, index);
        match case.matches.len() {
            1 => {}
            0 => return Err(CalibrationError::NoMatch { case: case_name }),
            count => {
                return Err(CalibrationError::MultipleMatches {
                    case: case_name,
                    count,
                });
            }
        }
        let assertion_count = case.matches[0].assertions.len();
        if assertion_count != 1 {
            return Err(CalibrationError::MultipleAssertions {
                case: case_name,
                count: assertion_count,
            });
        }
        let Some(&human_verdict) = labels.get(&case_name) else {
            return Err(CalibrationError::MissingLabel { case: case_name });
        };
        let assertion = &case.matches[0].assertions[0];
        validated.push(ValidatedCase {
            case_name,
            outcome: assertion.outcome.clone(),
            reason: assertion.reason.clone(),
            human_verdict,
        });
    }

    let total_examples = validated.len();
    let mut excluded = 0usize;
    let mut agreements = 0usize;
    let mut examples = Vec::with_capacity(total_examples);
    for validated_case in validated {
        let agreement = if is_excluded_outcome(&validated_case.outcome) {
            excluded += 1;
            Agreement::Excluded
        } else {
            let agreement = map_agreement(&validated_case.outcome, validated_case.human_verdict);
            if agreement == Agreement::Agree {
                agreements += 1;
            }
            agreement
        };
        examples.push(ExampleAgreement {
            id: validated_case.case_name,
            human_verdict: validated_case.human_verdict,
            judge_outcome: validated_case.outcome,
            reason: validated_case.reason,
            agreement,
        });
    }
    let considered = total_examples - excluded;
    #[expect(
        clippy::cast_precision_loss,
        reason = "a calibration suite's example count is nowhere near f64's 2^52 exact-integer \
                  range; precision loss is not a real concern for this ratio"
    )]
    let agreement_rate = if considered == 0 {
        0.0
    } else {
        agreements as f64 / considered as f64
    };
    let meets_bar = considered > 0 && agreement_rate >= JUDGE_CALIBRATION_BAR;

    Ok(CalibrationReport {
        suite: report.suite.clone(),
        run_id: report.run_id.clone(),
        total_examples,
        excluded,
        considered,
        agreements,
        agreement_rate,
        meets_bar,
        examples,
    })
}

/// One case that cleared Step 1/2's shape-and-label validation, carrying
/// exactly what the folding pass (Steps 3-5) needs.
struct ValidatedCase {
    case_name: String,
    outcome: String,
    reason: Option<String>,
    human_verdict: HumanVerdict,
}

/// `"errored"` and `"deferred"` are transport/wiring failures excluded from
/// both the numerator and denominator; every other outcome
/// (`"pass"`/`"fail"`/`"malformed"`) is folded into the agreement rate.
fn is_excluded_outcome(outcome: &str) -> bool {
    matches!(outcome, "errored" | "deferred")
}

/// Map a non-excluded judge outcome against its human label.
///
/// `"pass"` agrees with [`HumanVerdict::Pass`], disagrees with `Fail`;
/// `"fail"` agrees with [`HumanVerdict::Fail`], disagrees with `Pass`;
/// `"malformed"` (and any other unrecognized outcome string) always
/// disagrees — a judge that cannot commit to a verdict, or that produced an
/// outcome this closed set doesn't expect, is never "agreeing" with a human
/// who did commit to one.
fn map_agreement(outcome: &str, human_verdict: HumanVerdict) -> Agreement {
    match (outcome, human_verdict) {
        ("pass", HumanVerdict::Pass) | ("fail", HumanVerdict::Fail) => Agreement::Agree,
        _ => Agreement::Disagree,
    }
}

/// The case's `name:`, falling back to a positional identifier for the
/// (not-expected-in-a-well-formed calibration suite) unnamed case, mirroring
/// `eval.rs::case_tag`'s same fallback convention.
fn case_identifier(case: &crate::knowledge::eval::CaseReport, index: usize) -> String {
    case.name.clone().unwrap_or_else(|| format!("case-{index}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::knowledge::eval::AssertionReport;
    use crate::knowledge::eval::CaseReport;
    use crate::knowledge::eval::MatchReport;

    fn assertion(outcome: &str) -> AssertionReport {
        AssertionReport {
            class: String::from("judge"),
            outcome: String::from(outcome),
            reason: None,
        }
    }

    fn match_report(conversation: &str, assertions: Vec<AssertionReport>) -> MatchReport {
        MatchReport {
            conversation: String::from(conversation),
            assertions,
        }
    }

    fn case(name: &str, matches: Vec<MatchReport>) -> CaseReport {
        CaseReport {
            name: Some(String::from(name)),
            matches,
        }
    }

    fn report_with(cases: Vec<CaseReport>) -> EvalReport {
        EvalReport {
            suite: String::from("judge-calibration"),
            run_id: String::from("test-run"),
            timestamp: chrono::DateTime::<chrono::Utc>::UNIX_EPOCH,
            model: None,
            metrics: None,
            totals: crate::knowledge::eval::ReportTotals::default(),
            per_class: BTreeMap::new(),
            cases,
        }
    }

    #[test]
    fn zero_matches_is_rejected_with_no_match() {
        let report = report_with(vec![case("example-01", vec![])]);
        let labels = BTreeMap::new();

        let result = compute_calibration(&report, &labels);

        assert!(
            matches!(&result, Err(CalibrationError::NoMatch { case }) if case == "example-01"),
            "expected NoMatch, got {result:?}"
        );
    }

    #[test]
    fn two_matches_is_rejected_with_multiple_matches() {
        let report = report_with(vec![case(
            "example-01",
            vec![
                match_report("a.yaml", vec![assertion("pass")]),
                match_report("b.yaml", vec![assertion("pass")]),
            ],
        )]);
        let labels = BTreeMap::new();

        let result = compute_calibration(&report, &labels);

        assert!(
            matches!(
                &result,
                Err(CalibrationError::MultipleMatches { case, count: 2 }) if case == "example-01"
            ),
            "expected MultipleMatches {{ count: 2 }}, got {result:?}"
        );
    }

    #[test]
    fn case_name_absent_from_labels_is_rejected_with_missing_label() {
        let report = report_with(vec![
            case(
                "example-01",
                vec![match_report("a.yaml", vec![assertion("pass")])],
            ),
            case(
                "example-02",
                vec![match_report("b.yaml", vec![assertion("fail")])],
            ),
        ]);
        let mut labels = BTreeMap::new();
        labels.insert(String::from("example-01"), HumanVerdict::Pass);
        // example-02 deliberately has no label entry.

        let result = compute_calibration(&report, &labels);

        assert!(
            matches!(&result, Err(CalibrationError::MissingLabel { case }) if case == "example-02"),
            "expected MissingLabel {{ case: \"example-02\" }}, got {result:?}"
        );
    }

    #[test]
    fn errored_or_deferred_outcomes_are_excluded_from_considered_and_agreements() {
        let report = report_with(vec![
            case(
                "example-01",
                vec![match_report("a.yaml", vec![assertion("pass")])],
            ),
            case(
                "example-02",
                vec![match_report("b.yaml", vec![assertion("deferred")])],
            ),
            case(
                "example-03",
                vec![match_report("c.yaml", vec![assertion("fail")])],
            ),
        ]);
        let mut labels = BTreeMap::new();
        labels.insert(String::from("example-01"), HumanVerdict::Pass);
        labels.insert(String::from("example-02"), HumanVerdict::Pass);
        labels.insert(String::from("example-03"), HumanVerdict::Fail);

        let calibration =
            compute_calibration(&report, &labels).expect("all three cases are well-formed");

        assert_eq!(calibration.excluded, 1, "only example-02 is deferred");
        assert_eq!(calibration.considered, 2);
        let example_02 = calibration
            .examples
            .iter()
            .find(|e| e.id == "example-02")
            .expect("example-02 present in breakdown");
        assert_eq!(example_02.agreement, Agreement::Excluded);
    }

    #[test]
    fn malformed_outcome_always_disagrees_regardless_of_human_verdict() {
        let report = report_with(vec![
            case(
                "example-01",
                vec![match_report("a.yaml", vec![assertion("malformed")])],
            ),
            case(
                "example-02",
                vec![match_report("b.yaml", vec![assertion("malformed")])],
            ),
        ]);
        let mut labels = BTreeMap::new();
        labels.insert(String::from("example-01"), HumanVerdict::Pass);
        labels.insert(String::from("example-02"), HumanVerdict::Fail);

        let calibration =
            compute_calibration(&report, &labels).expect("both cases are well-formed");

        for id in ["example-01", "example-02"] {
            let example = calibration
                .examples
                .iter()
                .find(|e| e.id == id)
                .unwrap_or_else(|| panic!("{id} present in breakdown"));
            assert_eq!(
                example.agreement,
                Agreement::Disagree,
                "malformed must disagree regardless of human_verdict, got {example:#?}"
            );
        }
        assert_eq!(calibration.agreements, 0);
    }

    #[test]
    fn pass_agrees_with_pass_label_and_disagrees_with_fail_label() {
        let report = report_with(vec![
            case(
                "example-01",
                vec![match_report("a.yaml", vec![assertion("pass")])],
            ),
            case(
                "example-02",
                vec![match_report("b.yaml", vec![assertion("pass")])],
            ),
        ]);
        let mut labels = BTreeMap::new();
        labels.insert(String::from("example-01"), HumanVerdict::Pass);
        labels.insert(String::from("example-02"), HumanVerdict::Fail);

        let calibration =
            compute_calibration(&report, &labels).expect("both cases are well-formed");

        let example_01 = calibration
            .examples
            .iter()
            .find(|e| e.id == "example-01")
            .expect("example-01 present");
        let example_02 = calibration
            .examples
            .iter()
            .find(|e| e.id == "example-02")
            .expect("example-02 present");
        assert_eq!(example_01.agreement, Agreement::Agree);
        assert_eq!(example_02.agreement, Agreement::Disagree);
        assert_eq!(calibration.agreements, 1);
    }

    #[test]
    fn nine_considered_eight_agree_is_just_under_the_bar() {
        // 8/9 ≈ 0.888..., below the inclusive 0.90 bar -- proves meets_bar's
        // boundary is a real >= comparison, not "close to 90%" by
        // construction of the one fixture the feature test happens to use.
        let mut cases = Vec::new();
        let mut labels = BTreeMap::new();
        for n in 1..=8 {
            let name = format!("example-{n:02}");
            cases.push(case(
                &name,
                vec![match_report("a.yaml", vec![assertion("pass")])],
            ));
            labels.insert(name, HumanVerdict::Pass);
        }
        // The ninth example disagrees.
        cases.push(case(
            "example-09",
            vec![match_report("a.yaml", vec![assertion("pass")])],
        ));
        labels.insert(String::from("example-09"), HumanVerdict::Fail);

        let report = report_with(cases);
        let calibration = compute_calibration(&report, &labels).expect("all nine well-formed");

        assert_eq!(calibration.considered, 9);
        assert_eq!(calibration.agreements, 8);
        assert!(
            (calibration.agreement_rate - 8.0 / 9.0).abs() < 1e-9,
            "expected 8/9, got {}",
            calibration.agreement_rate
        );
        assert!(
            !calibration.meets_bar,
            "8/9 must not clear the inclusive >= 0.90 bar"
        );
    }

    #[test]
    fn ten_considered_nine_agree_lands_exactly_on_the_bar() {
        let mut cases = Vec::new();
        let mut labels = BTreeMap::new();
        for n in 1..=9 {
            let name = format!("example-{n:02}");
            cases.push(case(
                &name,
                vec![match_report("a.yaml", vec![assertion("pass")])],
            ));
            labels.insert(name, HumanVerdict::Pass);
        }
        cases.push(case(
            "example-10",
            vec![match_report("a.yaml", vec![assertion("pass")])],
        ));
        labels.insert(String::from("example-10"), HumanVerdict::Fail);

        let report = report_with(cases);
        let calibration = compute_calibration(&report, &labels).expect("all ten well-formed");

        assert_eq!(calibration.considered, 10);
        assert_eq!(calibration.agreements, 9);
        assert!((calibration.agreement_rate - 0.9).abs() < 1e-9);
        assert!(
            calibration.meets_bar,
            "0.9 must clear the inclusive >= 0.90 bar"
        );
    }

    #[test]
    fn two_assertions_on_one_match_is_rejected_with_multiple_assertions() {
        let report = report_with(vec![case(
            "example-01",
            vec![match_report(
                "a.yaml",
                vec![assertion("pass"), assertion("fail")],
            )],
        )]);
        let labels = BTreeMap::new();

        let result = compute_calibration(&report, &labels);

        assert!(
            matches!(
                &result,
                Err(CalibrationError::MultipleAssertions { case, count: 2 }) if case == "example-01"
            ),
            "expected MultipleAssertions {{ count: 2 }}, got {result:?}"
        );
    }
}
