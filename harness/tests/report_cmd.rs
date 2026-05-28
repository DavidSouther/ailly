//! Feature tests for `ailly report` — single-eval and comparison modes.
//!
//! Story 1 (single-eval): a developer has run `ailly eval` for a discovery
//! suite. They invoke `ailly -p <project> report <run-id>`. The command reads
//! the single `EvalReport` JSON and writes a markdown summary with the overall
//! pass rate and a per-case table, one column per assertion class. No
//! comparison logic, no verdict, no delta column.
//!
//! Story 2 (comparison): a developer has two runs — `baseline` (no skill
//! prefix) and `invocation` (skill prefix loaded). They invoke
//! `ailly -p <project> report <baseline-run-id> <invocation-run-id>`. The
//! command pairs assertions by (case, conversation, class), classifies each
//! pair as improved / regressed / unchanged, and writes a JSON report and a
//! markdown summary.

use std::fs;

use ailly_two::cli::report::ReportCmdArgs;
use ailly_two::cli::report::ReportCmdOutcome;
use ailly_two::cli::report::ReportMode;
use ailly_two::cli::report::run as report_run;

const ARM_A_RUN_ID: &str = "2026-05-20T10-00-00Z-baseline";
const ARM_B_RUN_ID: &str = "2026-05-27T14-43-31Z-target";

// ARM_A: 5/10 pass (50%). Two cases:
//   missing-fields: must_call_tool=fail, text_contains=fail, text_equals=fail,
//                   must_not_call_tool=pass, tool_call_count=pass
//   over-limit:     tool_call_order=pass, text_contains=fail, text_equals=fail,
//                   judge=pass, latency_ms=pass
const ARM_A_REPORT: &str = r#"{
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

// ARM_B: 7/10 pass (70%). Over arm_a:
//   missing-fields improved: must_call_tool, text_contains, text_equals
// (fail→pass)   over-limit regressed:    tool_call_order (pass→fail)
//   unchanged pass:          must_not_call_tool, tool_call_count, judge,
// latency_ms   unchanged fail:          text_contains, text_equals (over-limit)
const ARM_B_REPORT: &str = r#"{
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
async fn report_single_eval_mode_writes_markdown_with_per_class_table() {
    // Arrange: one EvalReport, 5/10 pass.
    let tmp = tempfile::tempdir().expect("tempdir");
    let project = tmp.path().to_path_buf();
    let reports_dir = project.join("evals").join("reports");
    fs::create_dir_all(&reports_dir).expect("create reports dir");
    fs::write(
        reports_dir.join(format!("{ARM_A_RUN_ID}.json")),
        ARM_A_REPORT,
    )
    .expect("write report");

    // Act
    let outcome = report_run(ReportCmdArgs {
        project: project.clone(),
        mode: ReportMode::Single {
            run_id: ARM_A_RUN_ID.to_string(),
        },
        label_a: None,
        label_b: None,
    })
    .await
    .expect("report handler succeeds");

    // Assert: output path matches naming convention.
    let ReportCmdOutcome::Single(single) = outcome else {
        panic!("expected Single outcome");
    };
    assert!(single.report_md.exists(), "report markdown written");
    let md_name = single.report_md.file_name().unwrap().to_str().unwrap();
    assert!(
        md_name.starts_with(ARM_A_RUN_ID),
        "filename starts with run_id"
    );
    assert!(
        md_name.ends_with("-report.md"),
        "filename ends with -report.md"
    );

    let md = fs::read_to_string(&single.report_md).expect("read markdown");

    // Layer 1: suite and run_id in header; overall pass rate.
    assert!(md.contains("regression"), "suite name in header");
    assert!(md.contains(ARM_A_RUN_ID), "run_id in header");
    assert!(md.contains("5 / 10"), "pass count / total");
    assert!(md.contains("50%"), "pass rate percentage");

    // Single mode has no comparison concepts.
    assert!(!md.contains("Verdict"), "no verdict in single mode");
    assert!(!md.contains("pp"), "no delta-pp in single mode");

    // Per-case assertion table has one column per class, sorted alphabetically.
    // Classes in this report: judge, latency_ms, must_call_tool,
    // must_not_call_tool,                         text_contains, text_equals,
    // tool_call_count, tool_call_order
    for class in [
        "judge",
        "latency_ms",
        "must_call_tool",
        "must_not_call_tool",
        "text_contains",
        "text_equals",
        "tool_call_count",
        "tool_call_order",
    ] {
        assert!(md.contains(class), "column '{class}' in table");
    }

    // Both case rows present.
    assert!(md.contains("missing-fields"), "missing-fields row");
    assert!(md.contains("over-limit"), "over-limit row");

    // Cell values use "pass", "fail", and em-dash for absent classes.
    assert!(md.contains("pass"), "pass cells in table");
    assert!(md.contains("fail"), "fail cells in table");
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "single feature-test function exercises the full comparison pipeline end-to-end"
)]
async fn report_comparison_mode_writes_json_and_markdown_with_change_summary() {
    // Arrange: two EvalReports. arm_a = 5/10 (baseline), arm_b = 7/10 (invocation).
    let tmp = tempfile::tempdir().expect("tempdir");
    let project = tmp.path().to_path_buf();
    let reports_dir = project.join("evals").join("reports");
    fs::create_dir_all(&reports_dir).expect("create reports dir");
    fs::write(
        reports_dir.join(format!("{ARM_A_RUN_ID}.json")),
        ARM_A_REPORT,
    )
    .expect("write arm_a");
    fs::write(
        reports_dir.join(format!("{ARM_B_RUN_ID}.json")),
        ARM_B_REPORT,
    )
    .expect("write arm_b");

    // Act
    let outcome = report_run(ReportCmdArgs {
        project: project.clone(),
        mode: ReportMode::Comparison {
            run_id_a: ARM_A_RUN_ID.to_string(),
            run_id_b: ARM_B_RUN_ID.to_string(),
        },
        label_a: None,
        label_b: None,
    })
    .await
    .expect("report handler succeeds");

    // Assert: both output paths exist with correct filenames.
    let ReportCmdOutcome::Comparison(comparison) = outcome else {
        panic!("expected Comparison outcome");
    };
    assert!(
        comparison.comparison_json.exists(),
        "comparison JSON written"
    );
    assert!(
        comparison.comparison_md.exists(),
        "comparison markdown written"
    );

    let json_stem = comparison
        .comparison_json
        .file_stem()
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        json_stem.contains(ARM_A_RUN_ID),
        "arm_a run_id in JSON filename"
    );
    assert!(
        json_stem.contains(ARM_B_RUN_ID),
        "arm_b run_id in JSON filename"
    );

    // Assert: ComparisonReport JSON structure.
    let json_text = fs::read_to_string(&comparison.comparison_json).expect("read JSON");
    let report: serde_json::Value = serde_json::from_str(&json_text).expect("valid JSON");

    assert_eq!(report["arm_a"]["run_id"], ARM_A_RUN_ID, "arm_a run_id");
    assert_eq!(report["arm_b"]["run_id"], ARM_B_RUN_ID, "arm_b run_id");

    let totals = &report["totals"];
    // 3 improved: must_call_tool, text_contains, text_equals in missing-fields
    // (fail→pass).
    assert_eq!(totals["improved"], 3, "3 assertions improved");
    // 1 regressed: tool_call_order in over-limit (pass→fail).
    assert_eq!(totals["regressed"], 1, "1 assertion regressed");
    // 4 unchanged pass: must_not_call_tool + tool_call_count (missing-fields)
    //                  + judge + latency_ms (over-limit).
    assert_eq!(totals["unchanged_pass"], 4, "4 unchanged passing");
    // 2 unchanged fail: text_contains + text_equals in over-limit (both stayed
    // fail).
    assert_eq!(totals["unchanged_fail"], 2, "2 unchanged failing");
    assert_eq!(totals["total_assertions"], 10, "10 paired assertions");

    // Per-case detail.
    let cases = report["cases"].as_array().expect("cases array");
    assert_eq!(cases.len(), 2, "2 cases");

    let mf = cases
        .iter()
        .find(|c| c["case"] == "missing-fields")
        .expect("missing-fields case");
    let mf_assertions = mf["assertions"]
        .as_array()
        .expect("missing-fields assertions");
    let mf_improved: Vec<_> = mf_assertions
        .iter()
        .filter(|a| a["change"] == "Improved")
        .collect();
    assert_eq!(
        mf_improved.len(),
        3,
        "3 improved assertions in missing-fields"
    );
    assert!(
        mf_improved
            .iter()
            .all(|a| a["arm_a"] == "fail" && a["arm_b"] == "pass"),
        "all improved went fail→pass"
    );

    let ol = cases
        .iter()
        .find(|c| c["case"] == "over-limit")
        .expect("over-limit case");
    let ol_assertions = ol["assertions"].as_array().expect("over-limit assertions");
    let regressed = ol_assertions
        .iter()
        .find(|a| a["change"] == "Regressed")
        .expect("one regressed assertion in over-limit");
    assert_eq!(
        regressed["class"], "tool_call_order",
        "tool_call_order regressed"
    );
    assert_eq!(regressed["arm_a"], "pass", "was passing in arm_a");
    assert_eq!(regressed["arm_b"], "fail", "now failing in arm_b");

    // Assert: markdown content.
    let md = fs::read_to_string(&comparison.comparison_md).expect("read markdown");

    // Summary line names both counts.
    assert!(md.contains("improved 3"), "improved count in summary line");
    assert!(
        md.contains("regressed 1"),
        "regressed count in summary line"
    );

    // Per-case headline table rows.
    assert!(md.contains("missing-fields"), "missing-fields in markdown");
    assert!(md.contains("over-limit"), "over-limit in markdown");

    // Arm labels (default extracted from run_id suffix or displayed as arm-a /
    // arm-b).
    let md_lower = md.to_lowercase();
    assert!(
        md_lower.contains("arm-a") || md_lower.contains("baseline"),
        "arm-a label in markdown"
    );
    assert!(
        md_lower.contains("arm-b") || md_lower.contains("target"),
        "arm-b label in markdown"
    );

    // Changed-assertions drill-down section is present and names the regressed
    // class.
    assert!(
        md.contains("Changed assertions") || md.contains("## Changes"),
        "changes section header"
    );
    assert!(
        md.contains("tool_call_order"),
        "regressed class in changes section"
    );
}
