//! Handler for `ailly eval <suite> --over <run-dir>`.
//!
//! Loads the suite, lists+loads the conversations under `over`, drives the
//! pure `knowledge::eval::evaluate` orchestrator, writes a JSON report to
//! `<project>/evals/reports/<run-id>.json`, and returns the structured outcome
//! the binary maps to an exit code.

use std::fs;
use std::io;
use std::path::PathBuf;

use crate::content::evaluation::EvaluationError;
use crate::content::repository::ConversationRepository;
use crate::content::repository::EvaluationRepository;
use crate::content::repository::FsConversationRepository;
use crate::content::repository::FsEvaluationRepository;
use crate::content::repository::RepositoryError;
use crate::knowledge::assertions::EvaluationContext;
use crate::knowledge::eval::EvalArgs;
use crate::knowledge::eval::evaluate;

#[derive(Clone, Debug)]
pub struct EvalCmdArgs {
    pub project: PathBuf,
    /// Suite name; resolves to `<project>/evals/<suite>.yaml`.
    pub suite: String,
    /// Conversation file or run directory. The `<run-id>` for the report path
    /// is the directory basename when `over` is a directory, or the file stem
    /// when `over` is a single file.
    pub over: PathBuf,
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct EvalCmdOutcome {
    pub conversations_matched: usize,
    pub assertions_passed: usize,
    pub assertions_failed: usize,
    pub assertions_deferred: usize,
    pub assertions_malformed: usize,
    pub report_path: PathBuf,
}

#[derive(Debug, thiserror::Error)]
pub enum EvalCmdError {
    #[error("repository error: {0}")]
    Repository(#[from] RepositoryError),
    #[error("evaluation error: {0}")]
    Evaluation(#[from] EvaluationError),
    #[error("writing report to {path:?}: {source}")]
    Report {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
}

/// End-to-end CLI handler. Loads the suite, loads every conversation under
/// `over`, calls [`evaluate`], writes the report, and returns the totals the
/// binary maps to an exit code.
///
/// # Errors
/// See [`EvalCmdError`]. The orchestrator itself does not produce errors; per-
/// assertion failures surface as `fail` or `malformed` verdicts in the report.
pub async fn run(args: EvalCmdArgs) -> Result<EvalCmdOutcome, EvalCmdError> {
    let suite_repo = FsEvaluationRepository::new(args.project.clone());
    let suite = suite_repo.get(&args.suite)?;

    let conv_repo = FsConversationRepository;
    let paths = conv_repo.list(&args.over)?;
    let mut conversations: Vec<(PathBuf, _)> = Vec::with_capacity(paths.len());
    for path in paths {
        let conv = conv_repo.load(&path)?;
        conversations.push((path, conv));
    }

    let run_id = derive_run_id(&args.over);
    let report = evaluate(EvalArgs {
        suite: &suite,
        conversations: &conversations,
        ctx: EvaluationContext::empty(),
        suite_name: &args.suite,
        run_id: &run_id,
    })
    .await;

    let report_dir = args.project.join("evals").join("reports");
    fs::create_dir_all(&report_dir).map_err(|source| EvalCmdError::Report {
        path: report_dir.clone(),
        source,
    })?;
    let report_path = report_dir.join(format!("{run_id}.json"));
    let writer = fs::File::create(&report_path).map_err(|source| EvalCmdError::Report {
        path: report_path.clone(),
        source,
    })?;
    serde_json::to_writer_pretty(&writer, &report).map_err(|source| EvalCmdError::Report {
        path: report_path.clone(),
        source: io::Error::other(source),
    })?;

    Ok(EvalCmdOutcome {
        conversations_matched: report.totals.conversations_matched,
        assertions_passed: report.totals.assertions.passed,
        assertions_failed: report.totals.assertions.failed,
        assertions_deferred: report.totals.assertions.deferred,
        assertions_malformed: report.totals.assertions.malformed,
        report_path,
    })
}

fn derive_run_id(over: &std::path::Path) -> String {
    if over.is_dir() {
        over.file_name()
            .and_then(|s| s.to_str())
            .map(String::from)
            .unwrap_or_default()
    } else {
        over.file_stem()
            .and_then(|s| s.to_str())
            .map(String::from)
            .unwrap_or_default()
    }
}
