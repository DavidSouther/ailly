//! Handler for `ailly eval <suite> --over <run-dir>`.
//!
//! Loads the suite, lists+loads the conversations under `over`, drives the
//! pure `knowledge::eval::evaluate` orchestrator, writes a JSON report to
//! `<project>/evals/reports/<run-id>.json`, and returns the structured outcome
//! the binary maps to an exit code.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;

use crate::content::evaluation::EvaluationError;
use crate::content::repository::ConversationKey;
use crate::content::repository::ConversationRepository;
use crate::content::repository::EvaluationRepository;
use crate::content::repository::RepositoryError;
use crate::engine::engine::open_engine_for_model;
use crate::knowledge::assertions::EvaluationContext;
use crate::knowledge::eval::ClassTotals;
use crate::knowledge::eval::EvalArgs;
use crate::knowledge::eval::evaluate;
use crate::knowledge::script_runner::TokioScriptRunner;

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
    /// Count of `script` + `program` assertions that resolved to `Deferred` —
    /// a runner-wiring regression, since those families only defer when the
    /// runner or project root is absent. Folded into [`Self::has_failures`].
    pub assertions_deferred_executable: usize,
    pub report_path: PathBuf,
}

impl EvalCmdOutcome {
    /// `true` when the run should exit non-zero: any failed, malformed, or
    /// errored assertion, or a `script`/`program` assertion that deferred (a
    /// runner-wiring regression). Passed and other deferred assertions (judge,
    /// tool, `text_semantic_match`) do not fail the run.
    #[must_use]
    pub fn has_failures(&self) -> bool {
        self.assertions_failed
            + self.assertions_malformed
            + self.assertions_errored
            + self.assertions_deferred_executable
            > 0
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

    let conv_repo = project.conversations();
    let run_id = project_relative(&project, &args.over);
    let keys = conv_repo.list(&run_id)?;
    let mut conversations: Vec<(PathBuf, _)> = Vec::with_capacity(keys.len());
    for key in &keys {
        let conv = conv_repo.load(key)?;
        conversations.push((key_path(key), conv));
    }

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
    let script_runner = TokioScriptRunner;
    let report = evaluate(EvalArgs {
        suite: &suite,
        conversations: &conversations,
        ctx: EvaluationContext {
            engine: engine.as_deref(),
            script_runner: Some(&script_runner),
            project_root: Some(args.project.as_path()),
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
        assertions_deferred_executable: executable_deferred_count(&report.per_class),
        report_path,
    })
}

/// Return `path` relative to the project host root, with forward-slash
/// separators. Mirrors the same helper in `cli/run.rs`.
fn project_relative(project: &crate::content::project::Project, path: &std::path::Path) -> String {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if let Some(host_root) = project.host_root() {
        canonical
            .strip_prefix(host_root)
            .ok()
            .and_then(|rel| rel.to_str())
            .map(|s| s.replace(std::path::MAIN_SEPARATOR, "/"))
            .unwrap_or_default()
    } else {
        path.to_str()
            .unwrap_or("")
            .replace(std::path::MAIN_SEPARATOR, "/")
    }
}

/// Stable `PathBuf` identity for a conversation key, used as the per-row
/// identifier in the eval report (`{run_id}/{name}.yaml`).
fn key_path(key: &ConversationKey) -> PathBuf {
    if key.run_id.is_empty() {
        PathBuf::from(format!("{}.yaml", key.name))
    } else {
        PathBuf::from(format!("{}/{}.yaml", key.run_id, key.name))
    }
}

/// Count `script` + `program` assertions that resolved to `Deferred`. Those
/// families defer ONLY when the runner or project root is missing — a wiring
/// regression — so the gate fails the run on them, unlike `judge` / `tool` /
/// `text_semantic_match` deferrals which stay non-failing.
fn executable_deferred_count(per_class: &BTreeMap<String, ClassTotals>) -> usize {
    per_class.get("script").map_or(0, |b| b.deferred)
        + per_class.get("program").map_or(0, |b| b.deferred)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executable_deferred_counts_only_script_and_program() {
        let mut per_class: BTreeMap<String, ClassTotals> = BTreeMap::new();
        per_class.insert(
            String::from("script"),
            ClassTotals {
                deferred: 1,
                ..Default::default()
            },
        );
        per_class.insert(
            String::from("program"),
            ClassTotals {
                deferred: 2,
                ..Default::default()
            },
        );
        per_class.insert(
            String::from("judge"),
            ClassTotals {
                deferred: 5,
                ..Default::default()
            },
        );
        assert_eq!(executable_deferred_count(&per_class), 3);
    }

    #[test]
    fn script_or_program_deferral_fails_the_run_but_other_deferrals_do_not() {
        let gated = EvalCmdOutcome {
            assertions_deferred_executable: 1,
            ..Default::default()
        };
        assert!(
            gated.has_failures(),
            "a script/program deferral must fail the run",
        );

        let benign = EvalCmdOutcome {
            assertions_deferred: 9,
            ..Default::default()
        };
        assert!(
            !benign.has_failures(),
            "judge/tool/text_semantic_match deferrals stay non-failing",
        );
    }
}
