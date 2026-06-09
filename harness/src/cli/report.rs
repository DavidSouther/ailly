//! Handler for `ailly -p <project> report <run-id>` (single mode)
//! and `ailly -p <project> report <run-id-a> <run-id-b>` (comparison mode).

use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

use crate::knowledge::eval::EvalReport;
use crate::knowledge::report::compute_comparison;
use crate::knowledge::report::render_comparison_markdown;
use crate::knowledge::report::render_single_markdown;

/// Selects single-eval or two-arm comparison mode.
pub enum ReportMode {
    Single { run_id: String },
    Comparison { run_id_a: String, run_id_b: String },
}

pub struct ReportCmdArgs {
    pub project: PathBuf,
    pub mode: ReportMode,
    /// Display label for arm A. Defaults to `"arm-a"`.
    pub label_a: Option<String>,
    /// Display label for arm B. Defaults to `"arm-b"`.
    pub label_b: Option<String>,
}

pub struct SingleOutcome {
    pub report_md: PathBuf,
}

pub struct ComparisonOutcome {
    pub comparison_json: PathBuf,
    pub comparison_md: PathBuf,
}

pub enum ReportCmdOutcome {
    Single(SingleOutcome),
    Comparison(ComparisonOutcome),
}

#[derive(Debug, thiserror::Error)]
pub enum ReportCmdError {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("JSON error in {path}: {source}")]
    Json {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
}

/// End-to-end CLI handler.
///
/// # Errors
/// Returns [`ReportCmdError::Io`] if a report file cannot be read or the
/// output file cannot be written. Returns [`ReportCmdError::Json`] if a
/// report file contains invalid JSON.
#[expect(
    clippy::unused_async,
    reason = "async for interface consistency with other CLI handlers"
)]
pub async fn run(args: ReportCmdArgs) -> Result<ReportCmdOutcome, ReportCmdError> {
    let reports_dir = args.project.join("evals").join("reports");
    // Ensure the output directory exists before any single- or comparison-mode
    // write; `eval` may not have created it (e.g. reporting over hand-authored
    // report JSON) and `fs::write` does not create parents.
    fs::create_dir_all(&reports_dir)?;

    match args.mode {
        ReportMode::Single { run_id } => {
            let report_path = reports_dir.join(format!("{run_id}.json"));
            let report = load_report(&report_path)?;

            let md_text = render_single_markdown(&report);
            let report_md = reports_dir.join(format!("{run_id}-report.md"));
            fs::write(&report_md, md_text)?;

            Ok(ReportCmdOutcome::Single(SingleOutcome { report_md }))
        }
        ReportMode::Comparison { run_id_a, run_id_b } => {
            let path_a = reports_dir.join(format!("{run_id_a}.json"));
            let path_b = reports_dir.join(format!("{run_id_b}.json"));
            let report_a = load_report(&path_a)?;
            let report_b = load_report(&path_b)?;

            let comparison = compute_comparison(&report_a, &report_b);

            let label_a = args.label_a.unwrap_or_else(|| String::from("arm-a"));
            let label_b = args.label_b.unwrap_or_else(|| String::from("arm-b"));

            let stem = format!("{run_id_a}-vs-{run_id_b}");
            let comparison_json = reports_dir.join(format!("{stem}.json"));
            let json_text = serde_json::to_string_pretty(&comparison).map_err(|source| {
                ReportCmdError::Json {
                    path: comparison_json.clone(),
                    source,
                }
            })?;
            fs::write(&comparison_json, json_text)?;

            let comparison_md = reports_dir.join(format!("{stem}.md"));
            let md_text = render_comparison_markdown(&comparison, &label_a, &label_b);
            fs::write(&comparison_md, md_text)?;

            Ok(ReportCmdOutcome::Comparison(ComparisonOutcome {
                comparison_json,
                comparison_md,
            }))
        }
    }
}

fn load_report(path: &Path) -> Result<EvalReport, ReportCmdError> {
    let text = fs::read_to_string(path)?;
    serde_json::from_str::<EvalReport>(&text).map_err(|source| ReportCmdError::Json {
        path: path.to_path_buf(),
        source,
    })
}
