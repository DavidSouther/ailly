//! Feature test: `eval` and `report` agree on the per-run report path.
//!
//! User story: an eval author assembles a run, evaluates it with
//! `ailly eval <suite> --over <run-dir>`, then compares two runs with
//! `ailly report <id-a> <id-b>`. The per-run report `eval` writes must land at
//! exactly the path `report` reads — keyed by the run-dir basename
//! (`evals/reports/<basename>.json`), with no `runs/` segment — so the
//! round-trip resolves and the comparison output is
//! `evals/reports/<a>-vs-<b>.json`.
//!
//! This drives the assemble → eval → report seam end-to-end against the
//! in-repo `e2e/patterns-eval` fixture, copied into a tempdir so the working
//! tree stays clean. No API key is required: the judge engine open fails and
//! judge assertions defer, which is fine — this test asserts WHERE the report
//! files land, not their grading outcomes.

use std::fs;
use std::path::Path;
use std::path::PathBuf;

use ailly_two::cli::assemble::AssembleArgs;
use ailly_two::cli::assemble::run as assemble_run;
use ailly_two::cli::eval::EvalCmdArgs;
use ailly_two::cli::eval::run as eval_run;
use ailly_two::cli::report::ReportCmdArgs;
use ailly_two::cli::report::ReportCmdOutcome;
use ailly_two::cli::report::ReportMode;
use ailly_two::cli::report::run as report_run;

/// Recursively copy a directory tree.
fn copy_tree(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).expect("create dst dir");
    for entry in fs::read_dir(src).expect("read src dir") {
        let entry = entry.expect("dir entry");
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&from, &to);
        } else {
            fs::copy(&from, &to).expect("copy file");
        }
    }
}

/// Basename of a run directory — the canonical per-run report id.
fn basename(run_dir: &Path) -> String {
    run_dir
        .file_name()
        .expect("run dir has a basename")
        .to_str()
        .expect("basename is utf-8")
        .to_string()
}

#[tokio::test]
async fn eval_and_report_agree_on_per_run_report_path() {
    // Arrange: copy the patterns-eval fixture into a tempdir project so the
    // assemble/eval/report artifacts never touch the source tree.
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("e2e/patterns-eval");
    let tmp = tempfile::tempdir().expect("tempdir");
    let project = tmp.path().join("patterns-eval");
    copy_tree(&fixture, &project);
    // Start from a clean reports/runs slate inside the copy.
    let _ = fs::remove_dir_all(project.join("runs"));
    let _ = fs::remove_dir_all(project.join("evals").join("reports"));

    // Act 1: assemble both arms; each mints runs/<id>/ and returns the host
    // run directory.
    let run_a = assemble_run(AssembleArgs {
        project: project.clone(),
        name: String::from("baseline"),
        ..Default::default()
    })
    .expect("assemble baseline");
    let run_b = assemble_run(AssembleArgs {
        project: project.clone(),
        name: String::from("invocation"),
        ..Default::default()
    })
    .expect("assemble invocation");

    let id_a = basename(&run_a);
    let id_b = basename(&run_b);
    assert_ne!(id_a, id_b, "the two arms mint distinct run ids");

    // Act 2: evaluate each arm over its run directory.
    let outcome_a = eval_run(EvalCmdArgs {
        project: project.clone(),
        suite: String::from("baseline"),
        over: run_a.clone(),
        ..Default::default()
    })
    .await
    .expect("eval baseline succeeds end-to-end (no nested-dir write failure)");
    let outcome_b = eval_run(EvalCmdArgs {
        project: project.clone(),
        suite: String::from("invocation"),
        over: run_b.clone(),
        ..Default::default()
    })
    .await
    .expect("eval invocation succeeds end-to-end");

    // Assert: the per-run report lands at evals/reports/<basename>.json — no
    // `runs/` segment — which is exactly where `report` reads it.
    let reports_dir = project.join("evals").join("reports");
    let expected_a = reports_dir.join(format!("{id_a}.json"));
    let expected_b = reports_dir.join(format!("{id_b}.json"));

    assert_eq!(
        outcome_a.report_path, expected_a,
        "eval must report the basename-keyed path it actually wrote",
    );
    assert!(
        expected_a.exists(),
        "baseline report at evals/reports/<basename>.json",
    );
    assert_eq!(outcome_b.report_path, expected_b);
    assert!(
        expected_b.exists(),
        "invocation report at evals/reports/<basename>.json",
    );
    assert!(
        !reports_dir.join("runs").exists(),
        "no nested runs/ segment under evals/reports/",
    );

    // The report payload's run_id is the basename, matching what `report`
    // and the e2e scripts pass on the command line.
    let report_a: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&expected_a).expect("read report a"))
            .expect("report a is valid json");
    assert_eq!(report_a["run_id"], id_a, "report run_id is the basename");

    // Act 3: two-arm comparison, keyed by the same basenames.
    let outcome = report_run(ReportCmdArgs {
        project: project.clone(),
        mode: ReportMode::Comparison {
            run_id_a: id_a.clone(),
            run_id_b: id_b.clone(),
        },
        label_a: None,
        label_b: None,
    })
    .await
    .expect("report reads both per-run reports and writes the comparison");

    // Assert: comparison output is evals/reports/<a>-vs-<b>.json.
    let ReportCmdOutcome::Comparison(comparison) = outcome else {
        panic!("expected Comparison outcome");
    };
    let expected_cmp = reports_dir.join(format!("{id_a}-vs-{id_b}.json"));
    assert_eq!(
        comparison.comparison_json, expected_cmp,
        "comparison JSON keyed by both basenames",
    );
    assert!(expected_cmp.exists(), "comparison JSON written");
    assert!(
        comparison.comparison_md.exists(),
        "comparison markdown written"
    );
}
