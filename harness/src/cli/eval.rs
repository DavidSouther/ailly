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
use crate::content::repository::RepositoryError;
use crate::content::repository::VfsConversationRepository;
use crate::engine::engine::open_engine_for_model;
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
    pub assertions_errored: usize,
    pub report_path: PathBuf,
}

impl EvalCmdOutcome {
    /// `true` when the run should exit non-zero: any failed, malformed, or
    /// errored assertion. Passed and deferred assertions do not fail the run.
    #[must_use]
    pub fn has_failures(&self) -> bool {
        self.assertions_failed + self.assertions_malformed + self.assertions_errored > 0
    }
}

#[derive(Debug, thiserror::Error)]
pub enum EvalCmdError {
    #[error("project error: {0}")]
    Project(#[from] crate::content::project::ProjectError),
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
    #[error("over path {path:?} is not valid UTF-8")]
    NonUtf8Path { path: PathBuf },
}

/// End-to-end CLI handler. Loads the suite, loads every conversation under
/// `over`, calls [`evaluate`], writes the report, and returns the totals the
/// binary maps to an exit code.
///
/// # Errors
/// See [`EvalCmdError`]. The orchestrator itself does not produce errors; per-
/// assertion failures surface as `fail` or `malformed` verdicts in the report.
pub async fn run(args: EvalCmdArgs) -> Result<EvalCmdOutcome, EvalCmdError> {
    let project = crate::content::project::Project::open(&args.project)?;
    crate::cli::env::load_project_env(&args.project);
    let suite = project.evals().get(&args.suite)?;

    let conv_repo = VfsConversationRepository;
    let over_vfs = resolve_over(&project, &args.over)?;
    let paths = conv_repo.list(&over_vfs)?;
    let mut conversations: Vec<(PathBuf, _)> = Vec::with_capacity(paths.len());
    for path in paths {
        let conv = conv_repo.load(&path)?;
        conversations.push((PathBuf::from(path.as_str()), conv));
    }

    let run_id = derive_run_id(&args.over);

    // Resolve the judge engine once, from the first conversation's model. A
    // failed open (no API key, unserviceable model) is not fatal: log it and
    // proceed with `engine: None`, which makes judge assertions defer. The
    // "engine present but call fails mid-evaluation" path is the only one that
    // produces `Errored`. Heterogeneous run dirs bind to the first model; per-
    // conversation dispatch is deferred
    // (docs/developer/TASK-NOTES-eval-judge-deferred.md).
    let engine = match conversations.first() {
        Some((_, conv)) => match open_engine_for_model(&conv.meta.model) {
            Ok(engine) => Some(engine),
            Err(err) => {
                tracing::warn!("judge engine unavailable: {err}; judge assertions will defer");
                None
            }
        },
        None => None,
    };

    let judge_dir = args.project.join("evals").join("judges").join(&run_id);
    let report = evaluate(EvalArgs {
        suite: &suite,
        conversations: &conversations,
        ctx: EvaluationContext {
            engine: engine.as_deref(),
        },
        suite_name: &args.suite,
        run_id: &run_id,
        judge_output_dir: Some(&judge_dir),
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
        assertions_errored: report.totals.assertions.errored,
        report_path,
    })
}

/// Resolve `over` to a [`vfs::VfsPath`]. Absolute paths mount onto a
/// host-rooted `vfs::PhysicalFS::new("/")`; relative paths join onto
/// `project.root()` (the project's vfs mount). UTF-8 invalid bytes in
/// the input are an explicit error at the CLI argument boundary.
fn resolve_over(
    project: &crate::content::project::Project,
    over: &std::path::Path,
) -> Result<vfs::VfsPath, EvalCmdError> {
    let over_str = over.to_str().ok_or_else(|| EvalCmdError::NonUtf8Path {
        path: over.to_path_buf(),
    })?;
    if over.is_absolute() {
        let host = vfs::VfsPath::new(vfs::PhysicalFS::new("/"));
        host.join(over_str.trim_start_matches('/'))
            .map_err(|source| RepositoryError::Vfs {
                path: over_str.to_string(),
                source,
            })
            .map_err(EvalCmdError::from)
    } else {
        project
            .root()
            .join(over_str)
            .map_err(|source| RepositoryError::Vfs {
                path: over_str.to_string(),
                source,
            })
            .map_err(EvalCmdError::from)
    }
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
