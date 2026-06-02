//! Feature test for the e2e-patterns-eval wiring slice.
//!
//! User story: an operator has completed both patterns-eval suites — six
//! discovery conversations and three invocation conversations against the
//! e2e/patterns-eval project. They invoke
//!
//!   ailly -p e2e/patterns-eval eval discovery  --over <discovery-run-dir>
//!   ailly -p e2e/patterns-eval eval invocation --over <invocation-run-dir>
//!
//! Each call reads the README-canonical evals/<suite>.yaml, scores every
//! case, writes a JSON report under evals/reports/<run-id>.json, and exits
//! 0 (`assertions_failed` + `assertions_malformed` == 0). The deferred-carry
//! matches the design exactly: two judge assertions defer on the paired
//! discovery cases; one judge per invocation case defers, while each
//! invocation `script` now executes through the wired subprocess runner and
//! passes (the `knowledge: eval-script` slice).
//!
//! Fails until e2e/patterns-eval/evals/discovery.yaml and
//! evals/invocation.yaml exist with the README-verbatim assertions.

use std::fs;
use std::path::Path;
use std::path::PathBuf;

use ailly_two::cli::eval::EvalCmdArgs;
use ailly_two::cli::eval::run as eval_run;

// --- Discovery synthetics ---------------------------------------------------
//
// Each assistant turn contains the case's required text_contains substring
// and avoids every text_not_contains substring. Paired cases ride alongside
// a judge assertion that defers because no engine is wired at this slice.

const CONV_NEWTYPE_MIXED_IDS: &str = "\
---
model: noop
assembly: discovery
binding:
  case: newtype-mixed-ids
---
role: user
content: \"We keep passing UserId where OrderId is expected. Which skill applies?\"
---
role: assistant
content: \"Load patterns:newtype. Wrapping each id in its own type turns the mismatch into a compile error.\"
trace:
  span_id: span-d1
  model: noop
  tokens:
    input: 800
    output: 40
  latency_ms: 200
";

const CONV_NEWTYPE_VS_EVS_ORDER_LINE: &str = "\
---
model: noop
assembly: discovery
binding:
  case: newtype-vs-evs-order-line
---
role: user
content: \"OrderLine carries price math. Which skill applies?\"
---
role: assistant
content: \"Load patterns:entities-value-objects-services. OrderLine carries behaviour, not just a wrapped primitive.\"
trace:
  span_id: span-d2
  model: noop
  tokens:
    input: 820
    output: 50
  latency_ms: 210
";

const CONV_CONFIGURING_FIRST_LOG_LINE: &str = "\
---
model: noop
assembly: discovery
binding:
  case: configuring-first-log-line
---
role: user
content: \"My first log line in main has nowhere to go. Which skill applies?\"
---
role: assistant
content: \"Load patterns:configuring-logging. The bootstrap registry is what gives the log line a destination.\"
trace:
  span_id: span-d3
  model: noop
  tokens:
    input: 810
    output: 45
  latency_ms: 205
";

const CONV_EMITTING_ORDER_PLACED_DISCOVERY: &str = "\
---
model: noop
assembly: discovery
binding:
  case: emitting-order-placed
---
role: user
content: \"I want to record order.placed inside a request handler. Which skill applies?\"
---
role: assistant
content: \"Load patterns:emitting-logs. The pipeline is already bootstrapped; this is a per-call-site concern.\"
trace:
  span_id: span-d4
  model: noop
  tokens:
    input: 815
    output: 48
  latency_ms: 215
";

const CONV_PAIRED_ADD_PROPAGATOR: &str = "\
---
model: noop
assembly: discovery
binding:
  case: paired-add-propagator
---
role: user
content: \"Where do I install the W3C propagator?\"
---
role: assistant
content: \"Load patterns:configuring-logging. The W3C propagator is installed once at process bootstrap, alongside the other registry layers.\"
trace:
  span_id: span-d5
  model: noop
  tokens:
    input: 830
    output: 60
  latency_ms: 220
";

const CONV_PAIRED_LOG_HANDLER_SUCCESS: &str = "\
---
model: noop
assembly: discovery
binding:
  case: paired-log-handler-success
---
role: user
content: \"How do I record a successful create_order?\"
---
role: assistant
content: \"Load patterns:emitting-logs. The success record is one call site inside a handler whose pipeline is already running.\"
trace:
  span_id: span-d6
  model: noop
  tokens:
    input: 825
    output: 55
  latency_ms: 218
";

// --- Invocation synthetics --------------------------------------------------
//
// Each assistant turn keeps tokens.total well below the case budget
// (6000, 8000, 6000 respectively). Assistant body content is brief; the
// script and judge assertions defer at this slice, so what the body says
// does not affect the outcome.

const CONV_NEWTYPE_WRAP_USER_ID: &str = "\
---
model: noop
assembly: invocation
binding:
  case: newtype-wrap-user-id
---
role: user
content: \"Wrap a string UserId.\"
---
role: assistant
content: \"type UserId = string & { readonly __brand: 'UserId' }; export function makeUserId(raw: string): UserId { if (raw.length === 0) throw new Error('empty UserId'); return raw as UserId; }\"
trace:
  span_id: span-i1
  model: noop
  tokens:
    input: 1800
    output: 200
  latency_ms: 500
";

const CONV_CONFIGURING_SERVICE_PIPELINE: &str = "\
---
model: noop
assembly: invocation
binding:
  case: configuring-service-pipeline
---
role: user
content: \"Stand up the five-layer subscriber registry in main.\"
---
role: assistant
content: \"export function initLogging(): void { const registry = new Registry().with(Format.json()).with(Filter.fromEnv()).with(Enrich.resource({ 'service.name': 'hello' })).with(Export.otlp({ endpoint: 'http://collector:4317' })); registry.install(); process.on('SIGTERM', () => registry.shutdown(5000)); }\"
trace:
  span_id: span-i2
  model: noop
  tokens:
    input: 2400
    output: 320
  latency_ms: 700
";

const CONV_EMITTING_ORDER_PLACED_INVOCATION: &str = "\
---
model: noop
assembly: invocation
binding:
  case: emitting-order-placed
---
role: user
content: \"Emit order.placed with semantic-convention keys.\"
---
role: assistant
content: \"logger.info({ eventName: 'order.placed', 'order.id': order.id, 'user.id': user.id, 'http.response.status_code': res.statusCode }, 'order placed');\"
trace:
  span_id: span-i3
  model: noop
  tokens:
    input: 1900
    output: 220
  latency_ms: 520
";

const DISCOVERY_CASES: &[(&str, &str)] = &[
    ("newtype-mixed-ids", CONV_NEWTYPE_MIXED_IDS),
    ("newtype-vs-evs-order-line", CONV_NEWTYPE_VS_EVS_ORDER_LINE),
    (
        "configuring-first-log-line",
        CONV_CONFIGURING_FIRST_LOG_LINE,
    ),
    (
        "emitting-order-placed",
        CONV_EMITTING_ORDER_PLACED_DISCOVERY,
    ),
    ("paired-add-propagator", CONV_PAIRED_ADD_PROPAGATOR),
    (
        "paired-log-handler-success",
        CONV_PAIRED_LOG_HANDLER_SUCCESS,
    ),
];

const INVOCATION_CASES: &[(&str, &str)] = &[
    ("newtype", CONV_NEWTYPE_WRAP_USER_ID),
    ("configuring-logging", CONV_CONFIGURING_SERVICE_PIPELINE),
    ("emitting-logs", CONV_EMITTING_ORDER_PLACED_INVOCATION),
];

#[tokio::test]
async fn patterns_eval_slice_evaluates_both_suites_end_to_end() {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("e2e")
        .join("patterns-eval");
    let reports_dir = project.join("evals").join("reports");

    let tmp = tempfile::tempdir().expect("tempdir");

    // ---- Discovery suite ----------------------------------------------------
    let discovery_run_id = "2026-05-26T00-00-00Z-test-discovery";
    let discovery_dir = tmp.path().join(discovery_run_id);
    fs::create_dir_all(&discovery_dir).expect("create discovery run dir");
    for (name, body) in DISCOVERY_CASES {
        fs::write(discovery_dir.join(format!("{name}.yaml")), body)
            .expect("write discovery conversation");
    }

    let discovery_outcome = eval_run(EvalCmdArgs {
        project: project.clone(),
        suite: String::from("discovery"),
        over: discovery_dir.clone(),
    })
    .await
    .expect("discovery eval succeeds end-to-end");

    // Per-case assertion totals for discovery:
    //   4 single-skill cases × (text_contains + text_not_contains) = 8 passed
    //   2 paired cases × (text_contains + text_not_contains + judge) = 4 passed + 2
    // deferred
    assert_eq!(discovery_outcome.conversations_matched, 6);
    assert_eq!(discovery_outcome.assertions_passed, 12);
    assert_eq!(discovery_outcome.assertions_failed, 0);
    assert_eq!(discovery_outcome.assertions_deferred, 2);
    assert_eq!(discovery_outcome.assertions_malformed, 0);
    assert_eq!(
        discovery_outcome.assertions_failed + discovery_outcome.assertions_malformed,
        0,
        "binary must exit 0 on discovery",
    );

    let discovery_report = reports_dir.join(format!("{discovery_run_id}.json"));
    assert_eq!(discovery_outcome.report_path, discovery_report);
    assert!(
        discovery_report.exists(),
        "discovery report written to disk"
    );
    assert_report_totals(
        &discovery_report,
        "discovery",
        discovery_run_id,
        6,
        12,
        0,
        2,
        0,
    );

    // ---- Invocation suite ---------------------------------------------------
    let invocation_run_id = "2026-05-26T00-00-00Z-test-invocation";
    let invocation_dir = tmp.path().join(invocation_run_id);
    fs::create_dir_all(&invocation_dir).expect("create invocation run dir");
    for (name, body) in INVOCATION_CASES {
        fs::write(invocation_dir.join(format!("{name}.yaml")), body)
            .expect("write invocation conversation");
    }

    let invocation_outcome = eval_run(EvalCmdArgs {
        project: project.clone(),
        suite: String::from("invocation"),
        over: invocation_dir.clone(),
    })
    .await
    .expect("invocation eval succeeds end-to-end");

    // Per-case assertion totals for invocation:
    //   3 cases × (script + judge + tokens) = 6 passed (script + tokens) + 3
    //   deferred (judge). The `knowledge: eval-script` slice wires a real
    //   subprocess runner into `ailly eval`, so each placeholder checker now
    //   executes and exits 0 (Pass) instead of deferring.
    assert_eq!(invocation_outcome.conversations_matched, 3);
    assert_eq!(invocation_outcome.assertions_passed, 6);
    assert_eq!(invocation_outcome.assertions_failed, 0);
    assert_eq!(invocation_outcome.assertions_deferred, 3);
    assert_eq!(invocation_outcome.assertions_malformed, 0);
    assert_eq!(
        invocation_outcome.assertions_failed + invocation_outcome.assertions_malformed,
        0,
        "binary must exit 0 on invocation",
    );

    let invocation_report = reports_dir.join(format!("{invocation_run_id}.json"));
    assert_eq!(invocation_outcome.report_path, invocation_report);
    assert!(
        invocation_report.exists(),
        "invocation report written to disk"
    );
    assert_report_totals(
        &invocation_report,
        "invocation",
        invocation_run_id,
        3,
        6,
        0,
        3,
        0,
    );

    // Cleanup so the e2e project tree stays pristine.
    let _ = fs::remove_file(&discovery_report);
    let _ = fs::remove_file(&invocation_report);
    let _ = fs::remove_dir(&reports_dir);
}

#[expect(
    clippy::too_many_arguments,
    reason = "shape-of-report assertion is naturally wide; named call sites read clearly"
)]
fn assert_report_totals(
    path: &Path,
    suite: &str,
    run_id: &str,
    conversations: usize,
    passed: usize,
    failed: usize,
    deferred: usize,
    malformed: usize,
) {
    let text = fs::read_to_string(path).expect("read report");
    let report: serde_json::Value = serde_json::from_str(&text).expect("report is valid JSON");
    assert_eq!(report["suite"], suite);
    assert_eq!(report["run_id"], run_id);
    assert_eq!(report["totals"]["conversations_matched"], conversations);
    assert_eq!(report["totals"]["assertions"]["passed"], passed);
    assert_eq!(report["totals"]["assertions"]["failed"], failed);
    assert_eq!(report["totals"]["assertions"]["deferred"], deferred);
    assert_eq!(report["totals"]["assertions"]["malformed"], malformed);
}
