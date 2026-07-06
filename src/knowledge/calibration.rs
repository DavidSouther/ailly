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
    let _ = (report, labels);
    todo!()
}
