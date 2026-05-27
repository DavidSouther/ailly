//! Pure domain types and computation for the `ailly report` output.
//! I/O lives in `cli::report`.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt::Write as FmtWrite;

use serde::Deserialize;
use serde::Serialize;

use crate::knowledge::eval::EvalReport;

/// Top-level `summary.json` contract.
#[derive(Serialize, Deserialize, Debug)]
pub struct ReportSummary {
    pub suite: String,
    pub baseline: RunRef,
    pub target: RunRef,
    pub verdict: Verdict,
    pub headline: Headline,
    pub quadrants: QuadrantBreakdown,
    pub divergent_cases: Vec<DivergentCase>,
}

/// Identifies one run in the comparison pair.
#[derive(Serialize, Deserialize, Debug)]
pub struct RunRef {
    pub run_id: String,
    pub timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Verdict label and emoji derived from pass-rate lift.
/// Thresholds: ≥0.50 Strong 🟢 | 0.20–0.49 Moderate 🟢 | 0.05–0.19 Weak 🟡
///             −0.05–0.04 Indistinguishable ⚪ | <−0.05 Regression 🔴
#[derive(Serialize, Deserialize, Debug)]
pub struct Verdict {
    pub label: String,
    pub emoji: String,
    pub lift: f64,
}

/// Compact pass-rate and efficiency metrics for the headline table.
#[derive(Serialize, Deserialize, Debug)]
pub struct Headline {
    pub baseline_pass_rate: f64,
    pub target_pass_rate: f64,
    pub delta_pp: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_latency_ms: Option<u64>,
}

/// Four-quadrant classification of all (baseline, target) assertion pairs.
/// `deferred` and `malformed` outcomes are excluded.
#[derive(Serialize, Deserialize, Debug)]
pub struct QuadrantBreakdown {
    pub signal: QuadrantDetail,
    pub regression: QuadrantDetail,
    pub baseline: QuadrantDetail,
    pub unreachable: QuadrantDetail,
}

/// Count and assertion-class list for one quadrant.
#[derive(Serialize, Deserialize, Debug, Default)]
pub struct QuadrantDetail {
    pub count: usize,
    pub classes: Vec<String>,
}

/// One case with at least one assertion that changed outcome between runs.
#[derive(Serialize, Deserialize, Debug)]
pub struct DivergentCase {
    pub case: String,
    pub conversation: String,
    pub changes: Vec<AssertionChange>,
}

/// One changed assertion within a divergent case.
#[derive(Serialize, Deserialize, Debug)]
pub struct AssertionChange {
    pub class: String,
    pub baseline: String,
    pub target: String,
    pub quadrant: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
}

/// Compare two runs of the same suite and produce the structured summary.
///
/// `baseline` is the older run; `target` is the newer run. Assertions are
/// paired by `(case_name, conversation, class)` triple. Assertions present in
/// one report but absent in the other are silently skipped.
#[must_use]
#[expect(
    clippy::too_many_lines,
    reason = "linear pass over the assertion matrix; extracting helpers would obscure the quadrant logic"
)]
pub fn compute_summary(suite: &str, baseline: &EvalReport, target: &EvalReport) -> ReportSummary {
    let (b_pass_rate, t_pass_rate, lift, delta_pp) = pass_rates(baseline, target);
    let (label, emoji) = verdict_label(lift);

    let baseline_index = index_assertions(baseline);

    let mut signal_count: usize = 0;
    let mut regression_count: usize = 0;
    let mut baseline_count: usize = 0;
    let mut unreachable_count: usize = 0;
    let mut signal_classes: BTreeSet<String> = BTreeSet::new();
    let mut regression_classes: BTreeSet<String> = BTreeSet::new();
    let mut baseline_classes: BTreeSet<String> = BTreeSet::new();
    let mut unreachable_classes: BTreeSet<String> = BTreeSet::new();

    let mut divergent_map: BTreeMap<(String, String), Vec<AssertionChange>> = BTreeMap::new();

    for case in &target.cases {
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
                let Some(b_outcome) = baseline_index.get(&key) else {
                    continue;
                };
                if b_outcome != "pass" && b_outcome != "fail" {
                    continue;
                }
                let quadrant = match (b_outcome.as_str(), assertion.outcome.as_str()) {
                    ("fail", "pass") => "signal",
                    ("pass", "fail") => "regression",
                    ("pass", "pass") => "baseline",
                    _ => "unreachable",
                };
                match quadrant {
                    "signal" => {
                        signal_count += 1;
                        signal_classes.insert(assertion.class.clone());
                    }
                    "regression" => {
                        regression_count += 1;
                        regression_classes.insert(assertion.class.clone());
                    }
                    "baseline" => {
                        baseline_count += 1;
                        baseline_classes.insert(assertion.class.clone());
                    }
                    _ => {
                        unreachable_count += 1;
                        unreachable_classes.insert(assertion.class.clone());
                    }
                }
                if b_outcome != &assertion.outcome {
                    let case_key = (
                        case.name.clone().unwrap_or_default(),
                        match_rep.conversation.clone(),
                    );
                    divergent_map
                        .entry(case_key)
                        .or_default()
                        .push(AssertionChange {
                            class: assertion.class.clone(),
                            baseline: b_outcome.clone(),
                            target: assertion.outcome.clone(),
                            quadrant: quadrant.to_string(),
                            evidence: None,
                        });
                }
            }
        }
    }

    let divergent_cases = divergent_map
        .into_iter()
        .map(|((case, conversation), changes)| DivergentCase {
            case,
            conversation,
            changes,
        })
        .collect();

    ReportSummary {
        suite: suite.to_string(),
        baseline: RunRef {
            run_id: baseline.run_id.clone(),
            timestamp: baseline.timestamp.to_rfc3339(),
            model: baseline.model.clone(),
        },
        target: RunRef {
            run_id: target.run_id.clone(),
            timestamp: target.timestamp.to_rfc3339(),
            model: target.model.clone(),
        },
        verdict: Verdict {
            label: label.to_string(),
            emoji: emoji.to_string(),
            lift,
        },
        headline: Headline {
            baseline_pass_rate: b_pass_rate,
            target_pass_rate: t_pass_rate,
            delta_pp,
            baseline_tokens: baseline
                .metrics
                .map(|m| m.total_input_tokens + m.total_output_tokens),
            target_tokens: target
                .metrics
                .map(|m| m.total_input_tokens + m.total_output_tokens),
            baseline_latency_ms: baseline.metrics.map(|m| m.total_latency_ms),
            target_latency_ms: target.metrics.map(|m| m.total_latency_ms),
        },
        quadrants: QuadrantBreakdown {
            signal: QuadrantDetail {
                count: signal_count,
                classes: signal_classes.into_iter().collect(),
            },
            regression: QuadrantDetail {
                count: regression_count,
                classes: regression_classes.into_iter().collect(),
            },
            baseline: QuadrantDetail {
                count: baseline_count,
                classes: baseline_classes.into_iter().collect(),
            },
            unreachable: QuadrantDetail {
                count: unreachable_count,
                classes: unreachable_classes.into_iter().collect(),
            },
        },
        divergent_cases,
    }
}

/// Render a `ReportSummary` as a three-layer Markdown report.
///
/// Layer 1: verdict + headline metrics table.
/// Layer 2: `## Quadrant breakdown` table.
/// Layer 3: `## Changes` — only divergent cases.
#[must_use]
pub fn render_markdown(summary: &ReportSummary) -> String {
    let mut out = String::new();

    let _ = write!(
        out,
        "# Report: {} vs {}\n\n",
        summary.baseline.run_id, summary.target.run_id
    );
    let _ = write!(
        out,
        "**Verdict:** {} {} ({:+}pp)\n\n",
        summary.verdict.emoji, summary.verdict.label, summary.headline.delta_pp
    );

    out.push_str("| Metric | Baseline | Target |\n");
    out.push_str("|--------|----------|--------|\n");
    let _ = writeln!(
        out,
        "| Pass rate | {:.0}% | {:.0}% |",
        summary.headline.baseline_pass_rate * 100.0,
        summary.headline.target_pass_rate * 100.0
    );
    let _ = writeln!(out, "| Delta | | {:+}pp |", summary.headline.delta_pp);
    match (
        summary.headline.baseline_tokens,
        summary.headline.target_tokens,
    ) {
        (Some(b), Some(t)) => {
            let _ = writeln!(out, "| Total tokens | {b} | {t} |");
        }
        (None, Some(t)) => {
            let _ = writeln!(out, "| Total tokens | \u{2014} | {t} |");
        }
        _ => {}
    }
    match (
        summary.headline.baseline_latency_ms,
        summary.headline.target_latency_ms,
    ) {
        (Some(b), Some(t)) => {
            let _ = writeln!(out, "| Latency (ms) | {b} | {t} |");
        }
        (None, Some(t)) => {
            let _ = writeln!(out, "| Latency (ms) | \u{2014} | {t} |");
        }
        _ => {}
    }
    out.push('\n');

    out.push_str("## Quadrant breakdown\n\n");
    out.push_str("| Quadrant | Count |\n");
    out.push_str("|----------|-------|\n");
    let _ = writeln!(
        out,
        "| Signal (fail\u{2192}pass) | {} |",
        summary.quadrants.signal.count
    );
    let _ = writeln!(
        out,
        "| Regression (pass\u{2192}fail) | {} |",
        summary.quadrants.regression.count
    );
    let _ = writeln!(
        out,
        "| Baseline (pass\u{2192}pass) | {} |",
        summary.quadrants.baseline.count
    );
    let _ = writeln!(
        out,
        "| Unreachable (fail\u{2192}fail) | {} |",
        summary.quadrants.unreachable.count
    );
    out.push('\n');

    if !summary.divergent_cases.is_empty() {
        out.push_str("## Changes\n\n");
        for case in &summary.divergent_cases {
            let _ = write!(out, "### {}\n\n", case.case);
            out.push_str("| Class | Baseline | Target | Quadrant |\n");
            out.push_str("|-------|----------|--------|----------|\n");
            for change in &case.changes {
                let _ = writeln!(
                    out,
                    "| {} | {} | {} | {} |",
                    change.class, change.baseline, change.target, change.quadrant
                );
            }
            out.push('\n');
        }
    }

    out
}

#[expect(
    clippy::cast_precision_loss,
    reason = "assertion counts are far below f64's 2^52 precision limit in practice"
)]
fn pass_rate(report: &EvalReport) -> f64 {
    let p = report.totals.assertions.passed;
    let f = report.totals.assertions.failed;
    if p + f == 0 {
        0.0
    } else {
        p as f64 / (p + f) as f64
    }
}

fn pass_rates(baseline: &EvalReport, target: &EvalReport) -> (f64, f64, f64, i64) {
    let b = pass_rate(baseline);
    let t = pass_rate(target);
    let lift = t - b;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "pass-rate lift is in [-1,1]; rounding then casting to i64 is safe"
    )]
    let delta_pp = (lift * 100.0).round() as i64;
    (b, t, lift, delta_pp)
}

fn verdict_label(lift: f64) -> (&'static str, &'static str) {
    // Tolerance guards against floating-point rounding when lift is computed
    // from integer pass/fail counts (e.g. 7/10 − 5/10 ≈ 0.19999…).
    const EPS: f64 = 1e-9;
    if lift >= 0.50 - EPS {
        ("Strong", "\u{1f7e2}")
    } else if lift >= 0.20 - EPS {
        ("Moderate", "\u{1f7e2}")
    } else if lift >= 0.05 - EPS {
        ("Weak", "\u{1f7e1}")
    } else if lift > -0.05 + EPS {
        ("Indistinguishable", "\u{26aa}")
    } else {
        ("Regression", "\u{1f534}")
    }
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
