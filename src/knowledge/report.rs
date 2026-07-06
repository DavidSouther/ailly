//! Pure domain types and computation for the `ailly report` output.
//! I/O lives in `cli::report`.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt::Write as FmtWrite;

use serde::Deserialize;
use serde::Serialize;

use crate::knowledge::eval::EvalReport;

/// The α level used for the paired-difference test's significance verdict.
/// See design.md Summary for why 0.05 (not a project-research-stated value)
/// was chosen as the conservative default.
pub const PAIRED_DIFFERENCE_ALPHA: f64 = 0.05;

/// Two-tailed paired Student's t-test over every assertion pair
/// `compute_comparison` already classifies as `Improved` (+1.0),
/// `Regressed` (-1.0), or `Unchanged{Pass,Fail}` (0.0).
#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PairedDifferenceTest {
    /// Fewer than 2 paired assertions exist between the two arms. A sample
    /// variance (and therefore a standard error and a t-statistic) cannot
    /// be estimated from 0 or 1 observations.
    InsufficientPairs { n: usize },
    Computed {
        n: usize,
        mean_difference: f64,
        sample_std_dev: f64,
        /// Standard error of the mean: `sample_std_dev / sqrt(n)`.
        standard_error: f64,
        degrees_of_freedom: usize,
        /// `None` iff `sample_std_dev == 0.0` — see the zero-variance
        /// convention below. `p_value`/`significant` stay well-defined by
        /// convention even when the t-statistic itself is not a real
        /// number (a 0/0 or x/0 limit).
        t_statistic: Option<f64>,
        p_value: f64,
        significant: bool,
    },
}

/// Top-level comparison report serialized to JSON.
#[derive(Serialize, Deserialize, Debug)]
pub struct ComparisonReport {
    pub arm_a: ArmRef,
    pub arm_b: ArmRef,
    pub totals: ComparisonTotals,
    /// [`ComparisonTotals::passes_falsification_gate`], surfaced once here
    /// rather than left for every consumer to re-derive from `totals`.
    /// Meaningful for any two-arm comparison, not only a baseline-arm one --
    /// the report doesn't know which comparison shape produced `arm_a`/
    /// `arm_b`, so this is the gate's verdict on the totals as given, not a
    /// claim that this comparison *is* a baseline falsification run.
    pub falsification_gate: bool,
    pub paired_difference: PairedDifferenceTest,
    pub cases: Vec<CaseComparison>,
}

/// Identifies one run arm.
#[derive(Serialize, Deserialize, Debug)]
pub struct ArmRef {
    pub run_id: String,
}

/// Aggregate change counts across all paired assertions.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct ComparisonTotals {
    pub improved: usize,
    pub regressed: usize,
    pub unchanged_pass: usize,
    pub unchanged_fail: usize,
    pub total_assertions: usize,
}

impl ComparisonTotals {
    /// The falsification gate documented in `skills/ailly-skill-eval/SKILL.md`
    /// ("Falsification as an optional layer") and `references/method.md` §6:
    /// the skill under test must help on at least one assertion the baseline
    /// arm failed, and must break nothing the baseline arm passed.
    #[must_use]
    pub fn passes_falsification_gate(&self) -> bool {
        self.improved > 0 && self.regressed == 0
    }
}

/// Per-case comparison data.
#[derive(Serialize, Deserialize, Debug)]
pub struct CaseComparison {
    pub case: String,
    pub assertions: Vec<AssertionComparison>,
}

/// One paired assertion comparison.
#[derive(Serialize, Deserialize, Debug)]
pub struct AssertionComparison {
    pub class: String,
    /// One of `"Improved"`, `"Regressed"`, `"UnchangedPass"`,
    /// `"UnchangedFail"`.
    pub change: String,
    pub arm_a: String,
    pub arm_b: String,
}

/// Compare two runs and produce the structured comparison report.
///
/// Assertions are paired by `(case_name, conversation, class)` triple.
/// Unnamed cases and `deferred`/`malformed` outcomes are excluded.
#[must_use]
pub fn compute_comparison(arm_a: &EvalReport, arm_b: &EvalReport) -> ComparisonReport {
    let a_index = index_assertions(arm_a);

    let mut totals = ComparisonTotals::default();
    let mut case_map: BTreeMap<String, Vec<AssertionComparison>> = BTreeMap::new();

    for case in &arm_b.cases {
        let Some(case_name) = &case.name else {
            continue;
        };
        for match_rep in &case.matches {
            for assertion in &match_rep.assertions {
                if assertion.outcome != "pass" && assertion.outcome != "fail" {
                    continue;
                }
                let key = (
                    case.name.clone(),
                    match_rep.conversation.clone(),
                    assertion.class.clone(),
                );
                let Some(a_outcome) = a_index.get(&key) else {
                    continue;
                };
                if a_outcome != "pass" && a_outcome != "fail" {
                    continue;
                }

                let change = match (a_outcome.as_str(), assertion.outcome.as_str()) {
                    ("fail", "pass") => "Improved",
                    ("pass", "fail") => "Regressed",
                    ("pass", "pass") => "UnchangedPass",
                    _ => "UnchangedFail",
                };

                match change {
                    "Improved" => totals.improved += 1,
                    "Regressed" => totals.regressed += 1,
                    "UnchangedPass" => totals.unchanged_pass += 1,
                    _ => totals.unchanged_fail += 1,
                }
                totals.total_assertions += 1;

                case_map
                    .entry(case_name.clone())
                    .or_default()
                    .push(AssertionComparison {
                        class: assertion.class.clone(),
                        change: change.to_string(),
                        arm_a: a_outcome.clone(),
                        arm_b: assertion.outcome.clone(),
                    });
            }
        }
    }

    let cases = case_map
        .into_iter()
        .map(|(case, assertions)| CaseComparison { case, assertions })
        .collect();

    ComparisonReport {
        arm_a: ArmRef {
            run_id: arm_a.run_id.clone(),
        },
        arm_b: ArmRef {
            run_id: arm_b.run_id.clone(),
        },
        falsification_gate: totals.passes_falsification_gate(),
        totals,
        cases,
        paired_difference: todo!(),
    }
}

/// Regularized incomplete beta function `I_x(a, b)`, computed via Lentz's
/// continued-fraction method with a Lanczos log-gamma approximation for the
/// beta normalization constant. Used by [`paired_difference_p_value`] to
/// derive the Student's-t two-tailed survival-function p-value.
fn regularized_incomplete_beta(x: f64, a: f64, b: f64) -> f64 {
    let _ = (x, a, b);
    todo!()
}

/// Two-tailed p-value for a Student's-t statistic via the closed-form
/// relationship to the regularized incomplete beta function:
/// `p = I_x(df/2, 1/2)`, `x = df / (df + t^2)`.
fn paired_difference_p_value(t: f64, df: usize) -> f64 {
    let _ = (t, df);
    todo!()
}

/// Fold a slice of per-pair diffs (`+1.0`/`-1.0`/`0.0`) into a
/// [`PairedDifferenceTest`].
fn compute_paired_difference(diffs: &[f64]) -> PairedDifferenceTest {
    let _ = diffs;
    todo!()
}

/// Render a single [`EvalReport`] as a markdown summary.
///
/// Layer 1: suite, `run_id`, overall pass rate.
/// Layer 2: per-case assertion table with one column per class (alphabetical).
#[must_use]
pub fn render_single_markdown(report: &EvalReport) -> String {
    let mut out = String::new();

    let _ = write!(
        out,
        "# Report: {} \u{2014} {}\n\n",
        report.suite, report.run_id
    );

    let passed = report.totals.assertions.passed;
    let failed = report.totals.assertions.failed;
    let total = passed + failed;
    let rate_pct = (passed * 100).checked_div(total).unwrap_or(0);
    let _ = write!(out, "**Pass rate:** {passed} / {total} ({rate_pct}%)\n\n");

    let mut classes: BTreeSet<String> = BTreeSet::new();
    for case in &report.cases {
        for match_rep in &case.matches {
            for assertion in &match_rep.assertions {
                classes.insert(assertion.class.clone());
            }
        }
    }
    let classes: Vec<String> = classes.into_iter().collect();

    out.push_str("| Case |");
    for class in &classes {
        let _ = write!(out, " {class} |");
    }
    out.push('\n');

    out.push_str("|------|");
    for _ in &classes {
        out.push_str("--------|");
    }
    out.push('\n');

    for case in &report.cases {
        let case_name = case.name.as_deref().unwrap_or("(unnamed)");
        for match_rep in &case.matches {
            let mut class_outcome: BTreeMap<&str, &str> = BTreeMap::new();
            for assertion in &match_rep.assertions {
                class_outcome.insert(&assertion.class, &assertion.outcome);
            }
            let _ = write!(out, "| {case_name} |");
            for class in &classes {
                let cell = class_outcome
                    .get(class.as_str())
                    .copied()
                    .unwrap_or("\u{2014}");
                let _ = write!(out, " {cell} |");
            }
            out.push('\n');
        }
    }

    out
}

/// Render a [`ComparisonReport`] as a three-layer markdown summary.
///
/// Layer 1: summary line with improved/regressed counts.
/// Layer 2: per-case headline table.
/// Layer 3: changed-assertions drill-down.
#[must_use]
pub fn render_comparison_markdown(
    report: &ComparisonReport,
    label_a: &str,
    label_b: &str,
) -> String {
    let mut out = String::new();

    let _ = write!(
        out,
        "# Report: {} vs {}\n\n",
        report.arm_a.run_id, report.arm_b.run_id
    );

    let _ = write!(
        out,
        "**Summary:** improved {}, regressed {}, unchanged pass {}, unchanged fail {}\n\n",
        report.totals.improved,
        report.totals.regressed,
        report.totals.unchanged_pass,
        report.totals.unchanged_fail,
    );

    let gate = if report.falsification_gate {
        "PASS"
    } else {
        "FAIL"
    };
    let _ = write!(out, "**Falsification gate:** {gate}\n\n");

    let _ = writeln!(out, "| Case | {label_a} | {label_b} |");
    out.push_str("|------|--------|--------|\n");
    for case in &report.cases {
        let improved = case
            .assertions
            .iter()
            .filter(|a| a.change == "Improved")
            .count();
        let regressed = case
            .assertions
            .iter()
            .filter(|a| a.change == "Regressed")
            .count();
        let _ = writeln!(out, "| {} | +{improved} / -{regressed} | |", case.case);
    }
    out.push('\n');

    let has_changes = report.cases.iter().any(|c| {
        c.assertions
            .iter()
            .any(|a| a.change == "Improved" || a.change == "Regressed")
    });

    if has_changes {
        out.push_str("## Changed assertions\n\n");
        for case in &report.cases {
            let changed: Vec<&AssertionComparison> = case
                .assertions
                .iter()
                .filter(|a| a.change == "Improved" || a.change == "Regressed")
                .collect();
            if !changed.is_empty() {
                let _ = write!(out, "### {}\n\n", case.case);
                let _ = writeln!(out, "| Class | {label_a} | {label_b} | Change |");
                out.push_str("|-------|--------|--------|--------|\n");
                for a in changed {
                    let _ = writeln!(
                        out,
                        "| {} | {} | {} | {} |",
                        a.class, a.arm_a, a.arm_b, a.change
                    );
                }
                out.push('\n');
            }
        }
    }

    out
}

type AssertionIndex = BTreeMap<(Option<String>, String, String), String>;

fn index_assertions(report: &EvalReport) -> AssertionIndex {
    let mut index = BTreeMap::new();
    for case in &report.cases {
        for match_rep in &case.matches {
            for assertion in &match_rep.assertions {
                let key = (
                    case.name.clone(),
                    match_rep.conversation.clone(),
                    assertion.class.clone(),
                );
                index.insert(key, assertion.outcome.clone());
            }
        }
    }
    index
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::knowledge::eval::AssertionReport;
    use crate::knowledge::eval::CaseReport;
    use crate::knowledge::eval::MatchReport;
    use crate::knowledge::eval::ReportTotals;

    fn fixture_report(run_id: &str, assertions: &[(&str, &str)]) -> EvalReport {
        EvalReport {
            suite: String::from("report-fixture"),
            run_id: run_id.to_string(),
            timestamp: chrono::DateTime::<chrono::Utc>::UNIX_EPOCH,
            model: None,
            metrics: None,
            totals: ReportTotals::default(),
            per_class: BTreeMap::new(),
            cases: vec![CaseReport {
                name: Some(String::from("case")),
                matches: vec![MatchReport {
                    conversation: String::from("case.yaml"),
                    assertions: assertions
                        .iter()
                        .map(|(class, outcome)| AssertionReport {
                            class: (*class).to_string(),
                            outcome: (*outcome).to_string(),
                            reason: None,
                        })
                        .collect(),
                }],
            }],
        }
    }

    #[test]
    fn compute_comparison_sets_falsification_gate_from_totals() {
        let baseline = fixture_report("baseline", &[("judge", "fail")]);
        let clears = fixture_report("invocation", &[("judge", "pass")]);
        let report = compute_comparison(&baseline, &clears);
        assert!(report.falsification_gate, "improved > 0 && regressed == 0");

        let regresses = fixture_report("invocation", &[("judge", "fail")]);
        let baseline = fixture_report("baseline", &[("judge", "pass")]);
        let report = compute_comparison(&baseline, &regresses);
        assert!(!report.falsification_gate, "a regression must clear FAIL");
    }

    #[test]
    fn render_comparison_markdown_shows_the_gate_verdict() {
        let passing = ComparisonReport {
            arm_a: ArmRef {
                run_id: String::from("a"),
            },
            arm_b: ArmRef {
                run_id: String::from("b"),
            },
            totals: ComparisonTotals {
                improved: 1,
                ..Default::default()
            },
            falsification_gate: true,
            paired_difference: PairedDifferenceTest::InsufficientPairs { n: 0 },
            cases: vec![],
        };
        assert!(render_comparison_markdown(&passing, "arm-a", "arm-b").contains("PASS"));

        let failing = ComparisonReport {
            falsification_gate: false,
            ..passing
        };
        assert!(render_comparison_markdown(&failing, "arm-a", "arm-b").contains("FAIL"));
    }

    #[test]
    fn passes_falsification_gate_clears_when_improved_and_not_regressed() {
        let totals = ComparisonTotals {
            improved: 1,
            regressed: 0,
            ..Default::default()
        };
        assert!(totals.passes_falsification_gate());
    }

    #[test]
    fn passes_falsification_gate_fails_when_a_regression_is_present() {
        let totals = ComparisonTotals {
            improved: 1,
            regressed: 1,
            ..Default::default()
        };
        assert!(!totals.passes_falsification_gate());
    }

    #[test]
    fn passes_falsification_gate_fails_when_vacuous() {
        let totals = ComparisonTotals::default();
        assert!(!totals.passes_falsification_gate());
    }
}
