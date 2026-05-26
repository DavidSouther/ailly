//! Feature test for the insurance-claim eval CUJ.
//!
//! User story: an operator has completed an insurance-claim run (three
//! conversations: missing-fields, ambiguous, over-limit). They invoke
//! `ailly eval regression --over <run-dir>` against the e2e/insurance-claim
//! project. The eval reads `evals/regression.yaml`, scores each case, and
//! exits 0. The report shows every sync assertion passed, the one judge
//! assertion on over-limit deferred, and zero failures or malformed.
//!
//! This test fails until `e2e/insurance-claim/evals/regression.yaml` exists.

use std::fs;
use std::path::PathBuf;

use ailly_two::cli::eval::EvalCmdArgs;
use ailly_two::cli::eval::run as eval_run;

// Three synthetic completed conversations that satisfy all sync assertions in
// the regression suite. Token values are chosen to stay inside every budget
// constraint the suite specifies.

const CONV_MISSING_FIELDS: &str = "\
---
model: noop
assembly: claim-handler
binding:
  case: missing-fields
---
role: user
content: \"Claim 77: my car was hit in a parking lot. No policy number provided. Classify.\"
---
role: assistant
content:
  - type: tool_use
    id: tu-001
    name: lookup_policy
    input:
      policy_number: unknown
  - type: text
    text: \"I attempted the policy lookup, but no policy number was supplied. policy number required to validate the claim. Routing to human-review.\"
trace:
  span_id: span-001
  model: noop
  tokens:
    input: 1200
    output: 60
  latency_ms: 450
";

// must_not_call_tool: auto_approve
// text_matches: clarif|specif (case-insensitive)
// tokens output < 500
const CONV_AMBIGUOUS: &str = "\
---
model: noop
assembly: claim-handler
binding:
  case: ambiguous
---
role: user
content: \"Claim 88: water damage from a leak. Classify this claim.\"
---
role: assistant
content: \"Routing to human-review. The claim requires clarification on the source of the leak before a routing decision can be made.\"
trace:
  span_id: span-002
  model: noop
  tokens:
    input: 1100
    output: 40
  latency_ms: 300
";

// must_not_call_tool: auto_approve
// tool_call_order: [lookup_policy, lookup_claim_history]
// judge: deferred (no engine wired)
const CONV_OVER_LIMIT: &str = "\
---
model: noop
assembly: claim-handler
binding:
  case: over-limit
---
role: user
content: \"Claim 99: property damage of $15,000. Policy P-99. Classify this claim.\"
---
role: assistant
content:
  - type: tool_use
    id: tu-002
    name: lookup_policy
    input:
      policy_number: P-99
  - type: tool_use
    id: tu-003
    name: lookup_claim_history
    input:
      claimant_id: C-99
  - type: text
    text: \"Routing to human-review. The claim amount of $15,000 exceeds the $10,000 auto-approve ceiling per the hard constraints.\"
trace:
  span_id: span-003
  model: noop
  tokens:
    input: 1300
    output: 55
  latency_ms: 500
";

#[tokio::test]
async fn insurance_claim_regression_suite_all_sync_pass_judge_defers() {
    // Arrange: build a synthetic run directory with three completed conversations.
    let tmp = tempfile::tempdir().expect("tempdir");
    let run_id = "2026-05-26T00-00-00Z-test-claim-handler";
    let run_dir = tmp.path().join(run_id);
    fs::create_dir_all(&run_dir).expect("create run dir");
    fs::write(run_dir.join("missing-fields.yaml"), CONV_MISSING_FIELDS)
        .expect("write missing-fields");
    fs::write(run_dir.join("ambiguous.yaml"), CONV_AMBIGUOUS).expect("write ambiguous");
    fs::write(run_dir.join("over-limit.yaml"), CONV_OVER_LIMIT).expect("write over-limit");

    // Point the project at the real e2e/insurance-claim directory so the test
    // reads the actual evals/regression.yaml fixture. The test fails until that
    // file exists.
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("e2e")
        .join("insurance-claim");

    // Act: invoke the CLI handler as the binary does.
    let outcome = eval_run(EvalCmdArgs {
        project: project.clone(),
        suite: String::from("regression"),
        over: run_dir.clone(),
    })
    .await
    .expect("eval handler succeeds end-to-end");

    // Assert: sync assertions on all three cases pass; the judge on over-limit
    // defers; nothing fails or is malformed.
    //
    // Sync assertion counts per case:
    //   missing-fields: must_call_tool + text_contains + tool_call_count + tokens =
    // 4   ambiguous:      must_not_call_tool + text_matches + tokens
    // = 3   over-limit:     must_not_call_tool + tool_call_order
    // = 2   judge on over-limit: deferred
    // = 1
    assert_eq!(outcome.conversations_matched, 3);
    assert_eq!(outcome.assertions_passed, 9);
    assert_eq!(outcome.assertions_failed, 0);
    assert_eq!(outcome.assertions_deferred, 1);
    assert_eq!(outcome.assertions_malformed, 0);

    // Assert: binary would exit 0 (no failures, no malformed).
    assert_eq!(
        outcome.assertions_failed + outcome.assertions_malformed,
        0,
        "binary must exit 0",
    );

    // Assert: report file written at the documented location.
    let expected_report = project
        .join("evals")
        .join("reports")
        .join(format!("{run_id}.json"));
    assert_eq!(outcome.report_path, expected_report);
    assert!(expected_report.exists(), "report file written to disk");

    // Assert: report JSON matches the documented contract.
    let report_text = fs::read_to_string(&expected_report).expect("read report");
    let report: serde_json::Value =
        serde_json::from_str(&report_text).expect("report is valid JSON");

    assert_eq!(report["suite"], "regression");
    assert_eq!(report["run_id"], run_id);
    assert_eq!(report["totals"]["conversations_matched"], 3);
    assert_eq!(report["totals"]["assertions"]["passed"], 9);
    assert_eq!(report["totals"]["assertions"]["failed"], 0);
    assert_eq!(report["totals"]["assertions"]["deferred"], 1);
    assert_eq!(report["totals"]["assertions"]["malformed"], 0);

    // Clean up the report so the e2e project directory stays pristine.
    let _ = fs::remove_file(&expected_report);
    let _ = fs::remove_dir(project.join("evals").join("reports"));
}
