//! Feature test for `ailly report` — benchmark-style two-run comparison.
//!
//! User story: a developer has run `ailly eval` twice against the same suite.
//! The baseline run (2026-05-20) had 5/10 assertions pass; the target run
//! (2026-05-27) has 7/10 pass. They invoke `ailly -p <project> report`; the
//! command reads both `evals/reports/*.json` files, computes three-layer
//! progressive disclosure (verdict + headline, quadrant breakdown, per-case
//! drill-down of divergent assertions), and writes `evals/reports/summary.json`
//! and `evals/reports/summary.md`.
//!
//! The test drives the report CLI handler end-to-end and validates that the
//! verdict label, headline pass-rate delta, quadrant bucket counts, and
//! divergent-case list all round-trip through the full pipeline.

use std::fs;

use ailly_two::cli::report::ReportCmdArgs;
use ailly_two::cli::report::run as report_run;

// Baseline: 5/10 pass (50%). No metrics (old run, trace not yet collected).
// missing-fields: must_call_tool=fail, text_contains=fail, text_equals=fail,
//   must_not_call_tool=pass, tool_call_count=pass
// over-limit: tool_call_order=pass, text_contains=fail, text_equals=fail,
//   judge=pass, latency_ms=pass
const BASELINE_REPORT: &str = r#"{
  "suite": "regression",
  "run_id": "2026-05-20T10-00-00Z-baseline",
  "timestamp": "2026-05-20T10:00:00Z",
  "model": "claude-sonnet-4-6",
  "totals": {
    "conversations_matched": 2,
    "assertions": { "passed": 5, "failed": 5, "deferred": 0, "malformed": 0 }
  },
  "per_class": {
    "must_call_tool":     { "passed": 0, "failed": 1, "deferred": 0, "malformed": 0 },
    "text_contains":      { "passed": 0, "failed": 2, "deferred": 0, "malformed": 0 },
    "text_equals":        { "passed": 0, "failed": 2, "deferred": 0, "malformed": 0 },
    "must_not_call_tool": { "passed": 1, "failed": 0, "deferred": 0, "malformed": 0 },
    "tool_call_count":    { "passed": 1, "failed": 0, "deferred": 0, "malformed": 0 },
    "tool_call_order":    { "passed": 1, "failed": 0, "deferred": 0, "malformed": 0 },
    "judge":              { "passed": 1, "failed": 0, "deferred": 0, "malformed": 0 },
    "latency_ms":         { "passed": 1, "failed": 0, "deferred": 0, "malformed": 0 }
  },
  "cases": [
    {
      "name": "missing-fields",
      "matches": [
        {
          "conversation": "missing-fields.yaml",
          "assertions": [
            { "class": "must_call_tool",     "outcome": "fail" },
            { "class": "text_contains",      "outcome": "fail" },
            { "class": "text_equals",        "outcome": "fail" },
            { "class": "must_not_call_tool", "outcome": "pass" },
            { "class": "tool_call_count",    "outcome": "pass" }
          ]
        }
      ]
    },
    {
      "name": "over-limit",
      "matches": [
        {
          "conversation": "over-limit.yaml",
          "assertions": [
            { "class": "tool_call_order", "outcome": "pass" },
            { "class": "text_contains",   "outcome": "fail" },
            { "class": "text_equals",     "outcome": "fail" },
            { "class": "judge",           "outcome": "pass" },
            { "class": "latency_ms",      "outcome": "pass" }
          ]
        }
      ]
    }
  ]
}"#;

// Target: 7/10 pass (70%). Metrics present (trace collected).
// Tokens: input=30000 + output=12300 = 42300 total.
// missing-fields: all three previously-failing assertions now pass (Signal ×3);
//   the two previously-passing assertions stay passing (Baseline ×2).
// over-limit: tool_call_order regressed pass→fail (Regression ×1);
//   the two text assertions stay failing (Unreachable ×2);
//   judge and latency_ms stay passing (Baseline ×2).
const TARGET_REPORT: &str = r#"{
  "suite": "regression",
  "run_id": "2026-05-27T14-43-31Z-target",
  "timestamp": "2026-05-27T14:43:31Z",
  "model": "claude-sonnet-4-6",
  "metrics": {
    "total_input_tokens": 30000,
    "total_output_tokens": 12300,
    "total_cache_hit_tokens": 14000,
    "total_latency_ms": 3840,
    "conversations_with_trace": 2
  },
  "totals": {
    "conversations_matched": 2,
    "assertions": { "passed": 7, "failed": 3, "deferred": 0, "malformed": 0 }
  },
  "per_class": {
    "must_call_tool":     { "passed": 1, "failed": 0, "deferred": 0, "malformed": 0 },
    "text_contains":      { "passed": 1, "failed": 1, "deferred": 0, "malformed": 0 },
    "text_equals":        { "passed": 1, "failed": 1, "deferred": 0, "malformed": 0 },
    "must_not_call_tool": { "passed": 1, "failed": 0, "deferred": 0, "malformed": 0 },
    "tool_call_count":    { "passed": 1, "failed": 0, "deferred": 0, "malformed": 0 },
    "tool_call_order":    { "passed": 0, "failed": 1, "deferred": 0, "malformed": 0 },
    "judge":              { "passed": 1, "failed": 0, "deferred": 0, "malformed": 0 },
    "latency_ms":         { "passed": 1, "failed": 0, "deferred": 0, "malformed": 0 }
  },
  "cases": [
    {
      "name": "missing-fields",
      "matches": [
        {
          "conversation": "missing-fields.yaml",
          "assertions": [
            { "class": "must_call_tool",     "outcome": "pass" },
            { "class": "text_contains",      "outcome": "pass" },
            { "class": "text_equals",        "outcome": "pass" },
            { "class": "must_not_call_tool", "outcome": "pass" },
            { "class": "tool_call_count",    "outcome": "pass" }
          ]
        }
      ]
    },
    {
      "name": "over-limit",
      "matches": [
        {
          "conversation": "over-limit.yaml",
          "assertions": [
            { "class": "tool_call_order", "outcome": "fail" },
            { "class": "text_contains",   "outcome": "fail" },
            { "class": "text_equals",     "outcome": "fail" },
            { "class": "judge",           "outcome": "pass" },
            { "class": "latency_ms",      "outcome": "pass" }
          ]
        }
      ]
    }
  ]
}"#;

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "single feature-test function exercises the full report pipeline end-to-end"
)]
async fn report_writes_summary_with_verdict_quadrants_and_divergent_cases() {
    // Arrange: tempdir project with two eval report files for the same suite.
    // Baseline (2026-05-20) passes 5/10; target (2026-05-27) passes 7/10.
    // The +20pp lift classifies as "Moderate".
    let tmp = tempfile::tempdir().expect("tempdir");
    let project = tmp.path().to_path_buf();
    let reports_dir = project.join("evals").join("reports");
    fs::create_dir_all(&reports_dir).expect("create reports dir");
    fs::write(
        reports_dir.join("2026-05-20T10-00-00Z-baseline.json"),
        BASELINE_REPORT,
    )
    .expect("write baseline");
    fs::write(
        reports_dir.join("2026-05-27T14-43-31Z-target.json"),
        TARGET_REPORT,
    )
    .expect("write target");

    // Act: invoke the CLI handler exactly as the binary will.
    let outcome = report_run(ReportCmdArgs {
        project: project.clone(),
        suite: None,
        run_ids: vec![],
    })
    .await
    .expect("report handler succeeds end-to-end");

    // Assert: output paths returned by handler exist on disk.
    assert!(outcome.summary_json.exists(), "summary.json written");
    assert!(outcome.summary_md.exists(), "summary.md written");

    // Assert: summary.json structure matches the design doc contract.
    let json_text = fs::read_to_string(&outcome.summary_json).expect("read summary.json");
    let summary: serde_json::Value =
        serde_json::from_str(&json_text).expect("summary.json is valid json");

    assert_eq!(summary["suite"], "regression");
    assert_eq!(
        summary["baseline"]["run_id"],
        "2026-05-20T10-00-00Z-baseline"
    );
    assert_eq!(summary["target"]["run_id"], "2026-05-27T14-43-31Z-target");

    // Verdict: +20pp lift → "Moderate" 🟢.
    assert_eq!(summary["verdict"]["label"], "Moderate");
    assert_eq!(summary["verdict"]["emoji"], "🟢");

    let headline = &summary["headline"];
    assert_eq!(headline["baseline_pass_rate"], 0.5_f64);
    assert_eq!(headline["target_pass_rate"], 0.7_f64);
    assert_eq!(headline["delta_pp"], 20_i64);
    // Baseline report has no metrics field; tokens and latency are null.
    assert!(
        headline["baseline_tokens"].is_null(),
        "baseline has no metrics — tokens must be null"
    );
    assert_eq!(
        headline["target_tokens"], 42_300_i64,
        "target_tokens = total_input + total_output"
    );
    assert_eq!(headline["target_latency_ms"], 3_840_i64);

    // Quadrant breakdown: computed from per-assertion (baseline, target) pairs.
    // Signal(3) + Regression(1) + Baseline(4) + Unreachable(2) = 10 pairs.
    let quadrants = &summary["quadrants"];
    assert_eq!(quadrants["signal"]["count"], 3, "Signal: 3 fail→pass");
    assert_eq!(
        quadrants["regression"]["count"], 1,
        "Regression: 1 pass→fail"
    );
    assert_eq!(quadrants["baseline"]["count"], 4, "Baseline: 4 pass→pass");
    assert_eq!(
        quadrants["unreachable"]["count"], 2,
        "Unreachable: 2 fail→fail"
    );

    // Quadrant class lists name which assertion class contributed.
    let signal_classes = quadrants["signal"]["classes"]
        .as_array()
        .expect("signal classes array");
    assert!(
        signal_classes.iter().any(|c| c == "must_call_tool"),
        "must_call_tool in Signal"
    );

    let regression_classes = quadrants["regression"]["classes"]
        .as_array()
        .expect("regression classes array");
    assert_eq!(
        regression_classes,
        &[serde_json::Value::String(String::from("tool_call_order"))]
    );

    // Divergent cases: missing-fields (Signal ×3), over-limit (Regression ×1).
    // Non-divergent cases are excluded from divergent_cases.
    let divergent = summary["divergent_cases"]
        .as_array()
        .expect("divergent_cases is an array");
    assert_eq!(divergent.len(), 2, "two cases have divergent assertions");

    let mf = divergent
        .iter()
        .find(|c| c["case"] == "missing-fields")
        .expect("missing-fields in divergent_cases");
    assert_eq!(mf["conversation"], "missing-fields.yaml");
    let mf_changes = mf["changes"].as_array().expect("missing-fields changes");
    assert_eq!(
        mf_changes.len(),
        3,
        "three assertions changed in missing-fields"
    );
    assert!(
        mf_changes.iter().all(|ch| ch["quadrant"] == "signal"),
        "all missing-fields changes are Signal"
    );

    let ol = divergent
        .iter()
        .find(|c| c["case"] == "over-limit")
        .expect("over-limit in divergent_cases");
    assert_eq!(ol["conversation"], "over-limit.yaml");
    let ol_changes = ol["changes"].as_array().expect("over-limit changes");
    assert_eq!(ol_changes.len(), 1, "one assertion changed in over-limit");
    assert_eq!(ol_changes[0]["quadrant"], "regression");
    assert_eq!(ol_changes[0]["class"], "tool_call_order");
    assert_eq!(ol_changes[0]["baseline"], "pass");
    assert_eq!(ol_changes[0]["target"], "fail");

    // Assert: summary.md contains the three-layer structural markers so the
    // human-readable output is consistent with the machine-readable contract.
    let md_text = fs::read_to_string(&outcome.summary_md).expect("read summary.md");
    assert!(md_text.contains("Moderate"), "verdict label in markdown");
    assert!(md_text.contains("50%"), "baseline pass rate in markdown");
    assert!(md_text.contains("70%"), "target pass rate in markdown");
    assert!(md_text.contains("+20pp"), "delta in markdown");
    assert!(
        md_text.contains("## Quadrant breakdown"),
        "quadrant section header"
    );
    assert!(
        md_text.contains("## Changes"),
        "divergent-cases section header"
    );
    assert!(
        md_text.contains("missing-fields"),
        "divergent case in markdown"
    );
    assert!(
        md_text.contains("over-limit"),
        "regression case in markdown"
    );
}
