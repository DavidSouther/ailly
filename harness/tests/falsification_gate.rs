//! Feature test for the `ailly-skill-eval` falsification gate as a
//! first-class Rust API.
//!
//! User story: before a developer builds a new eval suite on top of
//! `ailly-skill-eval`'s documented method, they need to trust that the
//! baseline-arm falsification gate -- `improved > 0 && regressed == 0`,
//! documented in `skills/ailly-skill-eval/SKILL.md` and
//! `references/method.md` §6 -- is not just prose, but a formula the code
//! actually computes the way the docs claim, and that any consumer (a CI
//! script, a test, a future suite) can read the exact same decision instead
//! of re-deriving the two-line boolean by hand each time.
//!
//! `compute_comparison` (src/knowledge/report.rs) already tallies the four
//! buckets correctly -- confirmed separately by the existing
//! `tests/report_cmd.rs` fixtures -- but the *gate itself*, the boolean
//! decision the docs, `e2e/patterns-eval/ci.sh`'s Python heredoc, and
//! `tests/skill_forge_clean_comments_review.rs` each re-derive by hand from
//! those buckets, has no single, tested, first-class Rust implementation.
//! That is the gap this test drives:
//! `ComparisonTotals::passes_falsification_gate` does not exist yet.
//!
//! This test pins the gate's decision against three fixture pairs built with
//! no live model calls, mirroring the three qualitatively distinct results a
//! real comparison can produce (see design.md's User Journey and Prior Art):
//!
//!   1. **Clears the gate** -- the skill improves one assertion the baseline
//!      failed, and regresses nothing the baseline passed.
//!   2. **Fails on regression** -- the skill improves one assertion but also
//!      regresses a different one; `improved > 0` alone must not be enough to
//!      pass.
//!   3. **Fails as vacuous** -- the skill changes nothing (an unhelpfully
//!      lenient checker that never fails baseline output). `references/
//!      method.md` §6 calls a gate that never fails on this shape "the point."
//!      This is also the exact bucket shape (`improved: 0, regressed: 0`) the
//!      one comparison artifact found on disk during this feature's research
//!      showed -- see design.md's Prior Art.

use std::collections::BTreeMap;

use ailly_two::knowledge::eval::AssertionReport;
use ailly_two::knowledge::eval::CaseReport;
use ailly_two::knowledge::eval::EvalReport;
use ailly_two::knowledge::eval::MatchReport;
use ailly_two::knowledge::eval::ReportTotals;
use ailly_two::knowledge::report::compute_comparison;

/// Build a minimal `EvalReport` fixture: one named case, one conversation
/// match, and the given `(class, outcome)` assertion pairs. Only the fields
/// `compute_comparison` reads (`cases[].name`, `.matches[].conversation`,
/// `.matches[].assertions[].class` / `.outcome`) are populated meaningfully;
/// the rest take inert defaults, since the gate under test never looks at
/// them.
fn fixture_report(
    run_id: &str,
    case: &str,
    conversation: &str,
    assertions: &[(&str, &str)],
) -> EvalReport {
    EvalReport {
        suite: String::from("falsification-gate-fixture"),
        run_id: run_id.to_string(),
        timestamp: chrono::DateTime::<chrono::Utc>::UNIX_EPOCH,
        model: None,
        metrics: None,
        totals: ReportTotals::default(),
        per_class: BTreeMap::new(),
        cases: vec![CaseReport {
            name: Some(case.to_string()),
            matches: vec![MatchReport {
                conversation: conversation.to_string(),
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
fn falsification_gate_matches_the_documented_improved_and_regressed_formula() {
    // Scenario 1: the skill helps once (judge fail -> pass) and breaks
    // nothing (tokens stays pass; script stays fail on both arms). SKILL.md's
    // gate reads this as a skill that "earns its place."
    let baseline = fixture_report(
        "2026-07-06T00-00-00Z-baseline",
        "emitting-logs",
        "emitting-logs.yaml",
        &[("judge", "fail"), ("script", "fail"), ("tokens", "pass")],
    );
    let invocation = fixture_report(
        "2026-07-06T00-00-00Z-invocation",
        "emitting-logs",
        "emitting-logs.yaml",
        &[("judge", "pass"), ("script", "fail"), ("tokens", "pass")],
    );
    let clears = compute_comparison(&baseline, &invocation);
    assert_eq!(clears.totals.improved, 1, "one assertion improved");
    assert_eq!(clears.totals.regressed, 0, "nothing regressed");
    assert_eq!(clears.totals.unchanged_pass, 1, "tokens stayed pass");
    assert_eq!(clears.totals.unchanged_fail, 1, "script stayed fail");
    assert!(
        clears.totals.passes_falsification_gate(),
        "improved > 0 && regressed == 0 must clear the gate"
    );

    // Scenario 2: the skill helps one assertion (judge fail -> pass) but
    // regresses another (tokens pass -> fail). SKILL.md's gate is explicit
    // that `improved > 0` alone is not enough -- a single regression must
    // still fail the gate.
    let baseline = fixture_report(
        "2026-07-06T00-01-00Z-baseline",
        "configuring-logging",
        "configuring-logging.yaml",
        &[("judge", "fail"), ("tokens", "pass")],
    );
    let invocation = fixture_report(
        "2026-07-06T00-01-00Z-invocation",
        "configuring-logging",
        "configuring-logging.yaml",
        &[("judge", "pass"), ("tokens", "fail")],
    );
    let regressed = compute_comparison(&baseline, &invocation);
    assert_eq!(regressed.totals.improved, 1, "judge improved");
    assert_eq!(regressed.totals.regressed, 1, "tokens regressed");
    assert!(
        !regressed.totals.passes_falsification_gate(),
        "a single regression must fail the gate even though improved > 0"
    );

    // Scenario 3: the skill changes nothing -- both arms fail the same
    // checker (a checker too lenient to ever fail baseline output, or a
    // genuine null-result skill). `references/method.md` §6 calls a gate
    // that never fails on this shape "the point": `improved == 0` must fail
    // the gate even though `regressed == 0` too. This is also the exact
    // bucket shape the one real comparison artifact found on disk during
    // this feature's research showed -- see design.md's Prior Art.
    let baseline = fixture_report(
        "2026-07-06T00-02-00Z-baseline",
        "newtype",
        "newtype.yaml",
        &[("script", "fail")],
    );
    let invocation = fixture_report(
        "2026-07-06T00-02-00Z-invocation",
        "newtype",
        "newtype.yaml",
        &[("script", "fail")],
    );
    let vacuous = compute_comparison(&baseline, &invocation);
    assert_eq!(vacuous.totals.improved, 0, "nothing improved");
    assert_eq!(vacuous.totals.regressed, 0, "nothing regressed either");
    assert!(
        !vacuous.totals.passes_falsification_gate(),
        "improved == 0 must fail the gate even when regressed == 0 too"
    );
}
