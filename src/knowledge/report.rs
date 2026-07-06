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
    let mut diffs: Vec<f64> = Vec::new();
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
                    "Improved" => {
                        totals.improved += 1;
                        diffs.push(1.0);
                    }
                    "Regressed" => {
                        totals.regressed += 1;
                        diffs.push(-1.0);
                    }
                    "UnchangedPass" => {
                        totals.unchanged_pass += 1;
                        diffs.push(0.0);
                    }
                    _ => {
                        totals.unchanged_fail += 1;
                        diffs.push(0.0);
                    }
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
        paired_difference: compute_paired_difference(&diffs),
    }
}

/// Lanczos approximation of the natural log of the gamma function, in the
/// classic Numerical Recipes coefficient form. Used to build the beta
/// normalization constant in [`regularized_incomplete_beta`].
fn log_gamma(xx: f64) -> f64 {
    const COF: [f64; 6] = [
        76.180_091_729_471_46,
        -86.505_320_329_416_77,
        24.014_098_240_830_91,
        -1.231_739_572_450_155,
        0.120_865_097_386_617_9e-2,
        -0.539_523_938_495_3e-5,
    ];
    let x = xx;
    let mut y = xx;
    let tmp = x + 5.5;
    let tmp = tmp - (x + 0.5) * tmp.ln();
    let mut ser = 1.000_000_000_190_015;
    for c in COF {
        y += 1.0;
        ser += c / y;
    }
    -tmp + (2.506_628_274_631_000_5 * ser / x).ln()
}

/// Lentz's continued-fraction evaluation of the incomplete beta function,
/// used only inside its valid convergence domain (`x < (a+1)/(a+b+2)`) by
/// [`regularized_incomplete_beta`], which flips to the complementary
/// identity outside that domain.
///
/// Variable names (`a`, `b`, `c`, `d`, `h`, `m`) intentionally mirror the
/// textbook Numerical Recipes `betacf` routine this implements, so the code
/// can be checked line-by-line against that reference.
#[allow(clippy::many_single_char_names)]
fn incomplete_beta_continued_fraction(a: f64, b: f64, x: f64) -> f64 {
    const MAX_ITERATIONS: u32 = 200;
    const EPSILON: f64 = 1e-14;
    const FP_MIN: f64 = 1e-300;

    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < FP_MIN {
        d = FP_MIN;
    }
    d = 1.0 / d;
    let mut h = d;

    for m in 1..=MAX_ITERATIONS {
        let m_f = f64::from(m);
        let m2 = 2.0 * m_f;

        let aa = m_f * (b - m_f) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < FP_MIN {
            d = FP_MIN;
        }
        c = 1.0 + aa / c;
        if c.abs() < FP_MIN {
            c = FP_MIN;
        }
        d = 1.0 / d;
        h *= d * c;

        let aa = -(a + m_f) * (qab + m_f) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < FP_MIN {
            d = FP_MIN;
        }
        c = 1.0 + aa / c;
        if c.abs() < FP_MIN {
            c = FP_MIN;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;

        if (del - 1.0).abs() < EPSILON {
            break;
        }
    }

    h
}

/// Regularized incomplete beta function `I_x(a, b)`, computed via Lentz's
/// continued-fraction method with a Lanczos log-gamma approximation for the
/// beta normalization constant. Used by [`paired_difference_p_value`] to
/// derive the Student's-t two-tailed survival-function p-value.
fn regularized_incomplete_beta(x: f64, a: f64, b: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }

    let ln_beta_normalization = log_gamma(a + b) - log_gamma(a) - log_gamma(b);
    let front = (ln_beta_normalization + a * x.ln() + b * (1.0 - x).ln()).exp();

    if x < (a + 1.0) / (a + b + 2.0) {
        front * incomplete_beta_continued_fraction(a, b, x) / a
    } else {
        1.0 - front * incomplete_beta_continued_fraction(b, a, 1.0 - x) / b
    }
}

/// Two-tailed p-value for a Student's-t statistic via the closed-form
/// relationship to the regularized incomplete beta function:
/// `p = I_x(df/2, 1/2)`, `x = df / (df + t^2)`.
fn paired_difference_p_value(t: f64, df: usize) -> f64 {
    // `df` is a paired-assertion count (realistically single-to-low-double
    // digits per suite comparison), never near f64's 2^52 mantissa limit.
    #[allow(clippy::cast_precision_loss)]
    let df = df as f64;
    let x = df / (df + t * t);
    regularized_incomplete_beta(x, df / 2.0, 0.5)
}

/// Fold a slice of per-pair diffs (`+1.0`/`-1.0`/`0.0`) into a
/// [`PairedDifferenceTest`].
///
/// Edge-case conventions, fixed by design.md's Specification (not
/// reinvented here):
/// - `n < 2` → [`PairedDifferenceTest::InsufficientPairs`]: a sample variance
///   (and therefore a standard error and a t-statistic) cannot be estimated
///   from 0 or 1 observations.
/// - `n >= 2` and zero variance (every diff identical) → `t_statistic: None`
///   (not `f64::INFINITY`/`NaN`, which `serde_json` cannot round-trip).
///   `mean_difference == 0.0` is the vacuous "every pair unchanged" comparison
///   (`p_value: 1.0, significant: false`); `mean_difference != 0.0` is a
///   perfect unanimous shift, the strongest evidence a paired comparison can
///   produce (`p_value: 0.0, significant: true`).
fn compute_paired_difference(diffs: &[f64]) -> PairedDifferenceTest {
    let n = diffs.len();
    if n < 2 {
        return PairedDifferenceTest::InsufficientPairs { n };
    }

    // `n` is a paired-assertion count (realistically single-to-low-double
    // digits per suite comparison), never near f64's 2^52 mantissa limit.
    #[allow(clippy::cast_precision_loss)]
    let n_f64 = n as f64;
    let mean_difference = diffs.iter().sum::<f64>() / n_f64;
    let sum_squared_deviation: f64 = diffs
        .iter()
        .map(|diff| (diff - mean_difference).powi(2))
        .sum();
    let sample_std_dev = (sum_squared_deviation / (n_f64 - 1.0)).sqrt();

    if sample_std_dev.abs() < f64::EPSILON {
        let (p_value, significant) = if mean_difference.abs() < f64::EPSILON {
            (1.0, false)
        } else {
            (0.0, true)
        };
        return PairedDifferenceTest::Computed {
            n,
            mean_difference,
            sample_std_dev,
            standard_error: 0.0,
            degrees_of_freedom: n - 1,
            t_statistic: None,
            p_value,
            significant,
        };
    }

    let standard_error = sample_std_dev / n_f64.sqrt();
    let degrees_of_freedom = n - 1;
    let t_statistic = mean_difference / standard_error;
    let p_value = paired_difference_p_value(t_statistic, degrees_of_freedom);

    PairedDifferenceTest::Computed {
        n,
        mean_difference,
        sample_std_dev,
        standard_error,
        degrees_of_freedom,
        t_statistic: Some(t_statistic),
        p_value,
        significant: p_value < PAIRED_DIFFERENCE_ALPHA,
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

    let gate = if report.falsification_gate {
        "PASS"
    } else {
        "FAIL"
    };
    let _ = write!(out, "**Falsification gate:** {gate}\n\n");

    match &report.paired_difference {
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
            let t_display =
                t_statistic.map_or_else(|| "undefined".to_string(), |t| format!("{t:.2}"));
            let verdict = if *significant {
                format!("significant at α = {PAIRED_DIFFERENCE_ALPHA}")
            } else {
                format!("not significant at α = {PAIRED_DIFFERENCE_ALPHA}")
            };
            let _ = write!(
                out,
                "**Paired-difference test:** n={n}, mean Δ {mean_difference:.3} (SEM {standard_error:.3}), t({degrees_of_freedom}) = {t_display}, p = {p_value:.4} — {verdict}\n\n",
            );
        }
        PairedDifferenceTest::InsufficientPairs { n } => {
            let _ = write!(
                out,
                "**Paired-difference test:** insufficient paired assertions (n={n}) for a significance test\n\n",
            );
        }
    }

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

    /// Reference value cross-check, required by design.md Specification
    /// item 1 before `paired_difference_p_value` is trusted: the standard
    /// printed two-tailed-5%-critical-value table entry for `df = 9` is
    /// `t = 2.262`. Independently re-verified for this plan against
    /// `scipy.stats.t.sf` and a from-scratch continued-fraction port
    /// (agreement to ~1e-8); this test pins the value at a looser `1e-3`
    /// tolerance appropriate for this repo's own implementation.
    #[test]
    fn p_value_matches_standard_two_tailed_five_percent_critical_value() {
        let p = paired_difference_p_value(2.262, 9);
        assert!(
            (p - 0.0500).abs() < 1e-3,
            "expected p ~= 0.0500 for t=2.262, df=9, got {p}"
        );
    }

    /// A t-statistic of exactly zero carries no evidence against the null,
    /// regardless of sample size.
    #[test]
    fn zero_t_statistic_always_yields_p_one() {
        assert!((paired_difference_p_value(0.0, 5) - 1.0).abs() < 1e-9);
        assert!((paired_difference_p_value(0.0, 9) - 1.0).abs() < 1e-9);
    }

    /// Monotonicity spot-check guarding against a sign or formula
    /// inversion that could still hit the two pinned reference values by
    /// coincidence.
    #[test]
    fn p_value_strictly_decreases_as_t_grows() {
        let p_small = paired_difference_p_value(1.0, 9);
        let p_large = paired_difference_p_value(5.0, 9);
        assert!(
            p_large < p_small,
            "expected p(t=5) < p(t=1), got p(t=5)={p_large}, p(t=1)={p_small}"
        );
    }

    /// Fewer than 2 paired assertions exist: a sample variance (and
    /// therefore a standard error and a t-statistic) cannot be estimated
    /// from 0 or 1 observations.
    #[test]
    fn fewer_than_two_diffs_yields_insufficient_pairs() {
        match compute_paired_difference(&[]) {
            PairedDifferenceTest::InsufficientPairs { n } => assert_eq!(n, 0),
            other => panic!("expected InsufficientPairs, got {other:?}"),
        }
        match compute_paired_difference(&[1.0]) {
            PairedDifferenceTest::InsufficientPairs { n } => assert_eq!(n, 1),
            other => panic!("expected InsufficientPairs, got {other:?}"),
        }
    }

    /// Every pair unchanged (the "vacuous comparison" Feature G's design
    /// independently named): zero variance, zero mean difference. No
    /// evidence of a difference either way.
    #[test]
    fn all_zero_diffs_yields_p_one_not_significant() {
        match compute_paired_difference(&[0.0, 0.0, 0.0]) {
            PairedDifferenceTest::Computed {
                mean_difference,
                t_statistic,
                p_value,
                significant,
                ..
            } => {
                assert!((mean_difference - 0.0).abs() < 1e-9);
                assert_eq!(t_statistic, None);
                assert!((p_value - 1.0).abs() < 1e-9);
                assert!(!significant);
            }
            other => panic!("expected Computed, got {other:?}"),
        }
    }

    /// Every pair moved by the same nonzero amount: zero variance, nonzero
    /// mean difference. The strongest evidence a paired comparison can
    /// produce (a perfect, unanimous shift with zero within-pair
    /// variance).
    #[test]
    fn all_equal_nonzero_diffs_yields_p_zero_significant() {
        match compute_paired_difference(&[1.0, 1.0, 1.0]) {
            PairedDifferenceTest::Computed {
                mean_difference,
                t_statistic,
                p_value,
                significant,
                ..
            } => {
                assert!((mean_difference - 1.0).abs() < 1e-9);
                assert_eq!(t_statistic, None);
                assert!((p_value - 0.0).abs() < 1e-9);
                assert!(significant);
            }
            other => panic!("expected Computed, got {other:?}"),
        }
    }

    fn comparison_report_with(paired_difference: PairedDifferenceTest) -> super::ComparisonReport {
        super::ComparisonReport {
            arm_a: super::ArmRef {
                run_id: String::from("arm-a"),
            },
            arm_b: super::ArmRef {
                run_id: String::from("arm-b"),
            },
            totals: super::ComparisonTotals::default(),
            paired_difference,
            cases: vec![],
        }
    }

    /// Facts-present check (design.md's own resolved wording decision):
    /// `n`, `mean_difference`, `standard_error`, `t_statistic`,
    /// `degrees_of_freedom`, `p_value`, and a not-significant phrase
    /// against α = 0.05 must all appear somewhere in the rendered output.
    /// Sentence structure is not load-bearing.
    #[test]
    fn render_comparison_markdown_names_computed_paired_difference_facts() {
        let report = comparison_report_with(PairedDifferenceTest::Computed {
            n: 10,
            mean_difference: 0.2,
            sample_std_dev: 0.632_455_53,
            standard_error: 0.2,
            degrees_of_freedom: 9,
            t_statistic: Some(1.0),
            p_value: 0.3434,
            significant: false,
        });
        let markdown = super::render_comparison_markdown(&report, "before", "after");

        assert!(markdown.contains("10"), "should name n: {markdown}");
        assert!(
            markdown.contains("0.2"),
            "should name mean_difference/standard_error: {markdown}"
        );
        assert!(markdown.contains('9'), "should name df: {markdown}");
        assert!(
            markdown.contains('1'),
            "should name t_statistic: {markdown}"
        );
        assert!(
            markdown.contains("0.3434") || markdown.contains("0.343"),
            "should name p_value: {markdown}"
        );
        assert!(markdown.contains("0.05"), "should name α: {markdown}");
        assert!(
            markdown.to_lowercase().contains("not significant"),
            "should say not significant: {markdown}"
        );
    }

    /// `InsufficientPairs` names `n` and says "insufficient".
    #[test]
    fn render_comparison_markdown_names_insufficient_pairs() {
        let report = comparison_report_with(PairedDifferenceTest::InsufficientPairs { n: 1 });
        let markdown = super::render_comparison_markdown(&report, "before", "after");

        assert!(markdown.contains('1'), "should name n: {markdown}");
        assert!(
            markdown.to_lowercase().contains("insufficient"),
            "should say insufficient: {markdown}"
        );
    }
}
