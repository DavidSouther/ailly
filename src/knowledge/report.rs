//! Pure domain types and computation for the `ailly report` output.
//! I/O lives in `cli::report`.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt::Write as FmtWrite;

use serde::Deserialize;
use serde::Serialize;

use crate::knowledge::eval::EvalReport;

/// Top-level comparison report serialized to JSON.
#[derive(Serialize, Deserialize, Debug)]
pub struct ComparisonReport {
    pub arm_a: ArmRef,
    pub arm_b: ArmRef,
    pub totals: ComparisonTotals,
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
    pub fn passes_falsification_gate(&self) -> bool {
        todo!()
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
        totals,
        cases,
    }
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
