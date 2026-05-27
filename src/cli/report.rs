//! Handler for `ailly -p <project> report [--suite <suite>] [<run-id>...]`.
//!
//! Reads all `EvalReport` JSON files from `<project>/evals/reports/`,
//! selects the oldest as baseline and newest as target (by timestamp),
//! delegates comparison to `knowledge::report::compute_summary`, and
//! writes `summary.json` + `summary.md` to the same directory.

use std::fs;
use std::io;
use std::path::PathBuf;

use crate::knowledge::eval::EvalReport;
use crate::knowledge::report::compute_summary;
use crate::knowledge::report::render_markdown;

pub struct ReportCmdArgs {
    pub project: PathBuf,
    /// Optional suite filter. When `None`, the suite name is taken from the
    /// first report file; all files must share a suite.
    pub suite: Option<String>,
    /// Explicit run IDs to include. When empty, all `*.json` files in the
    /// reports directory are used (excluding `summary.json`).
    pub run_ids: Vec<String>,
}

pub struct ReportCmdOutcome {
    pub summary_json: PathBuf,
    pub summary_md: PathBuf,
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
    #[error("need at least 2 runs to compare; found {0}")]
    NotEnoughRuns(usize),
    #[error("suite mismatch across report files")]
    SuiteMismatch,
}

/// End-to-end CLI handler.
///
/// # Errors
/// See [`ReportCmdError`].
///
/// # Panics
/// Panics if the internal sort leaves fewer than 2 reports — this cannot
/// happen because the `reports.len() < 2` guard runs before the sort.
#[expect(
    clippy::unused_async,
    reason = "async for interface consistency with other CLI handlers"
)]
pub async fn run(args: ReportCmdArgs) -> Result<ReportCmdOutcome, ReportCmdError> {
    let reports_dir = args.project.join("evals").join("reports");

    let mut report_files: Vec<PathBuf> = Vec::new();
    for entry in fs::read_dir(&reports_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if stem == "summary" {
            continue;
        }
        if args.run_ids.is_empty() || args.run_ids.iter().any(|id| id == stem) {
            report_files.push(path);
        }
    }

    let mut reports: Vec<EvalReport> = report_files
        .iter()
        .map(|path| {
            let text = fs::read_to_string(path)?;
            serde_json::from_str::<EvalReport>(&text).map_err(|source| ReportCmdError::Json {
                path: path.clone(),
                source,
            })
        })
        .collect::<Result<_, _>>()?;

    let suite_name = if let Some(s) = &args.suite {
        reports.retain(|r| &r.suite == s);
        s.clone()
    } else {
        let s = reports.first().map(|r| r.suite.clone()).unwrap_or_default();
        if reports.iter().any(|r| r.suite != s) {
            return Err(ReportCmdError::SuiteMismatch);
        }
        s
    };

    if reports.len() < 2 {
        return Err(ReportCmdError::NotEnoughRuns(reports.len()));
    }

    reports.sort_by_key(|r| r.timestamp);
    let baseline = reports.remove(0);
    let target = reports.pop().expect("at least 2 reports remain after sort");

    let summary = compute_summary(&suite_name, &baseline, &target);

    let summary_json = reports_dir.join("summary.json");
    let json_text =
        serde_json::to_string_pretty(&summary).map_err(|source| ReportCmdError::Json {
            path: summary_json.clone(),
            source,
        })?;
    fs::write(&summary_json, json_text)?;

    let summary_md = reports_dir.join("summary.md");
    let md_text = render_markdown(&summary);
    fs::write(&summary_md, md_text)?;

    Ok(ReportCmdOutcome {
        summary_json,
        summary_md,
    })
}
