//! Feature test for `ailly eval <suite> --over <run-dir>`.
//!
//! User story: an eval-suite author has a `runs/<id>/` directory of completed
//! conversations and an `evals/<suite>.yaml`. They invoke
//! `ailly eval regression --over runs/<id>/`; the orchestrator pairs each
//! suite case with its matching conversations, drives the per-assertion
//! executor across them, writes a structured JSON report to
//! `<project>/evals/reports/<run-id>.json`, and returns counts the binary
//! maps to a non-zero exit code when any assertion failed or was malformed.
//!
//! This test drives the orchestrator-and-CLI seam end-to-end. Per-variant
//! pass/fail coverage already lives in `tests/eval_assertions.rs` and the
//! unit tests inside `src/knowledge/assertions.rs` per the design doc; the
//! suite below exercises four outcome buckets (pass, fail, malformed, deferred)
//! plus the two matching modes (`name:` exact match and `when:` subset filter),
//! which is what the orchestrator owns. The judge assertion lands in the
//! `deferred` bucket: `model: noop` resolves to no judge grader (an auto Noop
//! adapter cannot produce a `GRADE:` line), so the judge defers rather than
//! malforms. A `claude-*` model with no API key defers for the same reason —
//! no grader resolves.

use std::fs;

use ailly_two::cli::eval::EvalCmdArgs;
use ailly_two::cli::eval::run as eval_run;

const SUITE_YAML: &str = "\
name: regression
cases:
  - name: missing-fields
    assertions:
      - { type: text_contains, value: \"policy number\" }
      - { type: text_equals, value: \"unreachable expectation\" }
      - { type: text_matches, pattern: \"[unclosed\" }
      - type: judge
        prompt: \"the response routes to human-review\"
  - when: { severity: high }
    assertions:
      - { type: response_field, path: \"$.session[1].trace\", exists: true }
";

const CONV_MISSING_FIELDS: &str = "\
---
model: noop
assembly: claim-handler
binding:
  case: missing-fields
  severity: high
---
role: user
content: \"claim narrative omits the policy number\"
---
role: assistant
content: \"policy number required to validate the claim.\"
trace:
  model: noop
  span_id: span-1
  request_id: req-1
  started_at: 2026-05-23T14:32:00Z
  finished_at: 2026-05-23T14:32:01Z
  latency_ms: 450
  tokens:
    input: 1200
    output: 300
    cache_hit: 200
    total: 1500
";

const CONV_OVER_LIMIT: &str = "\
---
model: noop
assembly: claim-handler
binding:
  case: over-limit
  severity: high
---
role: user
content: \"claim amount exceeds auto-approval ceiling\"
---
role: assistant
content: \"human-review: claim amount exceeds auto-approval ceiling.\"
trace:
  model: noop
  span_id: span-2
  request_id: req-2
  started_at: 2026-05-23T14:32:02Z
  finished_at: 2026-05-23T14:32:03Z
  latency_ms: 380
  tokens:
    input: 1100
    output: 280
    total: 1380
";

const CONV_DEFAULT: &str = "\
---
model: noop
assembly: claim-handler
binding:
  case: default
  severity: low
---
role: user
content: \"claim narrative is within policy threshold\"
---
role: assistant
content: \"auto-approve: claim is within policy threshold and required fields are present.\"
trace:
  model: noop
  span_id: span-3
  request_id: req-3
  started_at: 2026-05-23T14:32:04Z
  finished_at: 2026-05-23T14:32:05Z
  latency_ms: 220
  tokens:
    input: 900
    output: 200
    total: 1100
";

#[tokio::test]
async fn eval_writes_report_and_reports_failure_counts_across_match_modes() {
    // Arrange: stand up a tempdir project with one suite and three completed
    // conversations. The run-directory basename is the canonical run id; the
    // report will land at <project>/evals/reports/<run-id>.json.
    let tmp = tempfile::tempdir().expect("tempdir");
    let project = tmp.path().to_path_buf();
    let run_id = "2026-05-23T14-32-claim-handler";

    let evals_dir = project.join("evals");
    fs::create_dir_all(&evals_dir).expect("create evals/");
    fs::write(evals_dir.join("regression.yaml"), SUITE_YAML).expect("write suite");

    let run_dir = project.join("runs").join(run_id);
    fs::create_dir_all(&run_dir).expect("create run dir");
    fs::write(run_dir.join("missing-fields.yaml"), CONV_MISSING_FIELDS)
        .expect("write missing-fields");
    fs::write(run_dir.join("over-limit.yaml"), CONV_OVER_LIMIT).expect("write over-limit");
    fs::write(run_dir.join("default.yaml"), CONV_DEFAULT).expect("write default");

    // Act: invoke the CLI handler exactly as the binary will.
    let outcome = eval_run(EvalCmdArgs {
        project: project.clone(),
        suite: String::from("regression"),
        over: run_dir.clone(),
        ..Default::default()
    })
    .await
    .expect("eval handler succeeds end-to-end");

    // Assert: outcome counts cover all three conversations once (missing-fields
    // matched by name; over-limit and missing-fields matched by the `when:`
    // case; default is filtered out). The name-targeted case's four assertions
    // resolve to pass / fail / malformed / deferred: `text_contains` passes,
    // `text_equals` fails, the bad-regex `text_matches` is malformed, and the
    // judge defers — `model: noop` resolves to no judge grader (an auto Noop
    // adapter cannot produce a `GRADE:` line), so the judge assertion defers
    // rather than malforms. The when-filtered case adds two passes from its one
    // structural assertion.
    assert_eq!(outcome.conversations_matched, 3);
    assert_eq!(outcome.assertions_passed, 3); // 1 from name case + 2 from when case
    assert_eq!(outcome.assertions_failed, 1);
    assert_eq!(outcome.assertions_deferred, 1);
    assert_eq!(outcome.assertions_malformed, 1);
    assert!(
        outcome.assertions_failed + outcome.assertions_malformed > 0,
        "binary will exit non-zero",
    );

    // Assert: the report path the handler returns matches the documented
    // location and the file exists on disk.
    let expected_report = project
        .join("evals")
        .join("reports")
        .join(format!("{run_id}.json"));
    assert_eq!(outcome.report_path, expected_report);
    assert!(expected_report.exists(), "report file written");

    // Assert: report JSON shape matches the design doc. We parse with
    // serde_json::Value so the test does not depend on the orchestrator's
    // private struct layout, only on its on-disk contract.
    let report_text = fs::read_to_string(&expected_report).expect("read report");
    let report: serde_json::Value =
        serde_json::from_str(&report_text).expect("report is valid json");

    assert_eq!(report["suite"], "regression");
    assert_eq!(report["run_id"], run_id);

    let totals = &report["totals"];
    assert_eq!(totals["conversations_matched"], 3);
    assert_eq!(totals["assertions"]["passed"], 3);
    assert_eq!(totals["assertions"]["failed"], 1);
    assert_eq!(totals["assertions"]["deferred"], 1);
    assert_eq!(totals["assertions"]["malformed"], 1);

    // Per-class rollup: each variant's lowercase serde tag is its own key.
    // text_contains passes (one conversation), text_equals fails (one), the
    // bad regex on text_matches is malformed (one), the judge defers (one:
    // `model: noop` resolves to no grader), and response_field passes twice
    // (the when-filter matched two conversations).
    let per_class = &report["per_class"];
    assert_eq!(per_class["text_contains"]["passed"], 1);
    assert_eq!(per_class["text_equals"]["failed"], 1);
    assert_eq!(per_class["text_matches"]["malformed"], 1);
    assert_eq!(per_class["judge"]["deferred"], 1);
    assert_eq!(per_class["response_field"]["passed"], 2);

    // Cases preserve in-suite order; the name-targeted case's matches list
    // contains exactly one conversation file, and its assertions list
    // preserves YAML order across all four outcome variants.
    let cases = report["cases"].as_array().expect("cases is an array");
    assert_eq!(cases.len(), 2);

    let missing = &cases[0];
    assert_eq!(missing["name"], "missing-fields");
    let matches = missing["matches"].as_array().expect("matches array");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0]["conversation"], "missing-fields.yaml");
    let assertions = matches[0]["assertions"]
        .as_array()
        .expect("assertions array");
    assert_eq!(assertions.len(), 4);
    assert_eq!(assertions[0]["class"], "text_contains");
    assert_eq!(assertions[0]["outcome"], "pass");
    assert_eq!(assertions[1]["class"], "text_equals");
    assert_eq!(assertions[1]["outcome"], "fail");
    assert_eq!(assertions[2]["class"], "text_matches");
    assert_eq!(assertions[2]["outcome"], "malformed");
    assert_eq!(assertions[3]["class"], "judge");
    assert_eq!(assertions[3]["outcome"], "deferred");

    // The when-filtered case matched two conversations; both must appear in
    // its matches list, each carrying one passing assertion.
    let when_case = &cases[1];
    let when_matches = when_case["matches"].as_array().expect("when matches array");
    assert_eq!(when_matches.len(), 2);
    let mut matched_names: Vec<&str> = when_matches
        .iter()
        .map(|m| m["conversation"].as_str().expect("conversation string"))
        .collect();
    matched_names.sort_unstable();
    assert_eq!(
        matched_names,
        vec!["missing-fields.yaml", "over-limit.yaml"]
    );
    for m in when_matches {
        let a = m["assertions"].as_array().expect("assertions array");
        assert_eq!(a.len(), 1);
        assert_eq!(a[0]["class"], "response_field");
        assert_eq!(a[0]["outcome"], "pass");
    }

    // Trace rollup: every conversation in the run directory carries a trace
    // block, so the report's top-level `model` and `metrics` fields are
    // populated from the full run, not just the suite-matched subset. Sums
    // are over every Message.trace across every conversation loaded from
    // run-dir (missing-fields: 1200/300/450, over-limit: 1100/280/380,
    // default: 900/200/220).
    assert_eq!(report["model"], "noop");
    let metrics = &report["metrics"];
    assert_eq!(metrics["total_input_tokens"], 3200);
    assert_eq!(metrics["total_output_tokens"], 780);
    assert_eq!(metrics["total_cache_hit_tokens"], 200);
    assert_eq!(metrics["total_latency_ms"], 1050);
    assert_eq!(metrics["conversations_with_trace"], 3);
}
