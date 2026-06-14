//! Feature test for the e2e-research tool-call wiring slice.
//!
//! User story: a research operator has completed the `web-research` case
//! against the e2e/research project — a multi-turn tool conversation that
//! searches the web for a source and then fetches it. They invoke
//!
//!   ailly -p e2e/research eval research --over <run-dir>
//!
//! The eval reads the README-canonical evals/research.yaml, scores the
//! `web-research` case, writes a JSON report under evals/reports/<run-id>.json,
//! and exits 0 (`assertions_failed` + `assertions_malformed` == 0). The suite
//! asserts the now-live tool-call read path: `must_call_tool: web_search`,
//! `must_call_tool: web_fetch`, and `tool_call_order: [web_search, web_fetch]`
//! all pass on the authored `tool_use` blocks (Feature 1's `extract_tool_uses`
//! reads them exactly as if a model had emitted them; design §"noop tool-result
//! scripting mechanism").
//!
//! Unlike the three baseline `eval_*` e2e tests — which write their synthetic
//! run dir to a tmp path OUTSIDE the project root, where `project_relative`
//! (`cli/mod.rs:15-30`) returns an empty `RunId` and so `conversations_matched`
//! is 0 — this test writes its run dir INSIDE the e2e/research project tree
//! (a unique nonced subdir under `runs/`, gitignored), exactly as
//! `e2e/research/ci.sh` does. With the run dir under the project's host root,
//! `project_relative` strips the prefix to `runs/<nonce>`, the conversation
//! repository finds the fixture, `conversations_matched == 1`, and the three
//! tool-call assertions score as passing. The nonced run dir and its report are
//! removed at the end so the project tree stays pristine.
//!
//! Fails until e2e/research/evals/research.yaml exists with the three
//! tool-call assertions and the synthetic body's `tool_use` blocks are
//! scorable.

use std::fs;
use std::path::Path;
use std::path::PathBuf;

use ailly_two::cli::eval::EvalCmdArgs;
use ailly_two::cli::eval::run as eval_run;

// The synthetic completed conversation for the `web-research` case. Mirrors
// `e2e/research/fixtures/web-research.yaml`: one `tool_use` per assistant turn
// (web_search then web_fetch, in message order), each followed by a
// `role: tool` tool_result, then a final text turn. No blank assistant slot.
// The block schema (`tool_use { id, name, input }` / `tool_result
// { tool_use_id, content }`) matches the live read path in `extract_tool_uses`.
const CONV_WEB_RESEARCH: &str = "\
---
model: noop
assembly: research
binding:
  case: web-research
---
role: user
content: \"Find the official Rust homepage and fetch its tagline.\"
---
role: assistant
content:
  - type: tool_use
    id: tu-1
    name: web_search
    input: { query: \"rust language official site\" }
---
role: tool
content:
  - type: tool_result
    tool_use_id: tu-1
    content: \"Rust — https://www.rust-lang.org\\nA language empowering everyone.\"
---
role: assistant
content:
  - type: tool_use
    id: tu-2
    name: web_fetch
    input: { url: \"https://www.rust-lang.org\" }
---
role: tool
content:
  - type: tool_result
    tool_use_id: tu-2
    content: \"HTTP 200 (text/html)\\n\\nA language empowering everyone to build reliable and efficient software.\"
---
role: assistant
content: \"The Rust homepage tagline is: A language empowering everyone to build reliable and efficient software.\"
trace:
  span_id: span-research
  model: noop
  tokens:
    input: 1200
    output: 60
  latency_ms: 400
";

#[tokio::test]
async fn research_suite_scores_tool_call_conversation_end_to_end() {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("e2e")
        .join("research");

    // In-tree, nonced run dir under the project's `runs/` (gitignored). The
    // UUIDv7 nonce keeps concurrent test/ci runs from colliding. Writing inside
    // the project root is what makes `project_relative` resolve the listing key
    // to `runs/<nonce>` so the conversation is found and scored.
    let run_id = format!("test-{}", uuid::Uuid::now_v7());
    let run_dir = project.join("runs").join(&run_id);
    fs::create_dir_all(&run_dir).expect("create in-tree run dir");
    fs::write(run_dir.join("web-research.yaml"), CONV_WEB_RESEARCH)
        .expect("write web-research conversation");

    let report_path = project
        .join("evals")
        .join("reports")
        .join(format!("{run_id}.json"));

    // Run the eval, then clean up regardless of assertion outcome so a failing
    // assertion does not leave the nonced run dir / report behind.
    let result = eval_run(EvalCmdArgs {
        project: project.clone(),
        suite: String::from("research"),
        over: run_dir.clone(),
    })
    .await;

    let cleanup = || {
        let _ = fs::remove_file(&report_path);
        let _ = fs::remove_dir_all(&run_dir);
    };

    let outcome = match result {
        Ok(outcome) => outcome,
        Err(err) => {
            cleanup();
            panic!("research eval succeeds end-to-end: {err}");
        }
    };

    // The in-tree run dir resolves, so the single fixture is matched and the
    // three tool-call assertions score as passing with no live API:
    //   must_call_tool: web_search   -> Pass
    //   must_call_tool: web_fetch    -> Pass
    //   tool_call_order: [web_search, web_fetch] -> Pass
    assert_eq!(outcome.conversations_matched, 1, "in-tree fixture matched");
    assert_eq!(
        outcome.assertions_passed, 3,
        "three tool-call assertions pass"
    );
    assert_eq!(outcome.assertions_failed, 0);
    assert_eq!(outcome.assertions_deferred, 0);
    assert_eq!(outcome.assertions_malformed, 0);
    assert_eq!(
        outcome.assertions_failed + outcome.assertions_malformed,
        0,
        "binary must exit 0 on research",
    );

    assert_eq!(outcome.report_path, report_path, "report at nonced path");
    assert!(report_path.exists(), "research report written to disk");
    assert_report_totals(&report_path, "research", &run_id, 1, 3, 0, 0, 0);

    cleanup();
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
