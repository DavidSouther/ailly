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

use crate::cli::env;
use crate::content::evaluation::EvaluationError;
use crate::content::project::Project;
use crate::content::repository::ConversationKey;
use crate::content::repository::ConversationRepository;
use crate::content::repository::EvaluationRepository;
use crate::content::repository::RepositoryError;
use crate::content::repository::RunId;
use crate::content::repository::VfsConversationRepository;
use crate::engine::engine::is_noop_model;
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
    #[error("resolving --over path {path:?}: {source}")]
    Over {
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
    let project = Project::open(&args.project)?;
    env::load_project_env(&args.project);
    let suite = project.evals().get(&args.suite)?;

    // Root a conversations repository at `over` itself, not at the project
    // root: `--over` may point anywhere on disk, including outside the project
    // tree. Canonicalize first so a symlinked tempdir (macOS `/var` ->
    // `/private/var`) resolves to real files, then list with the empty `RunId`,
    // which `VfsConversationRepository::dir_for` maps to the repository root.
    let over_root = args
        .over
        .canonicalize()
        .map_err(|source| EvalCmdError::Over {
            path: args.over.clone(),
            source,
        })?;
    let conversations_repository =
        VfsConversationRepository::new(vfs::VfsPath::new(vfs::PhysicalFS::new(over_root)));
    // The report id is the run-dir basename (or single-file stem) — the
    // per-run identity `report` reads (`evals/reports/<report_id>.json`) and
    // the e2e scripts pass on the command line. It is independent of the
    // listing root: the root locates the conversation files (anywhere on
    // disk), the report id names the per-run report under the project.
    let report_id = report_id_for(&args.over);
    let keys = conversations_repository.list(&RunId::default())?;
    let mut conversations: Vec<(PathBuf, _)> = Vec::with_capacity(keys.len());
    for key in &keys {
        let conv = conversations_repository.load(key)?;
        conversations.push((key_path(key), conv));
    }

    // Resolve the judge engine once, from the first conversation's model. A
    // failed open (no API key, unserviceable model) is not fatal: log it and
    // proceed with `engine: None`, which makes judge assertions defer. The
    // "engine present but call fails mid-evaluation" path is the only one that
    // produces `Errored`. Heterogeneous run dirs bind to the first model; per-
    // conversation dispatch is deferred
    // (docs/developer/TASK-NOTES-eval-judge-deferred.md).
    //
    // A `model: noop` run resolves to no judge engine: an auto Noop adapter
    // cannot produce a `GRADE:` line, so using it as a grader would malform
    // rather than defer. Declining it makes judge assertions defer, which is
    // the meaningful verdict offline (see `is_noop_model`; a scriptable Noop
    // judge engine is tracked in TASKS.md).
    let engine = match conversations.first() {
        Some((_, conv)) if !is_noop_model(&conv.meta.model) => {
            match open_engine_for_model(&conv.meta.model) {
                Ok(engine) => Some(engine),
                Err(err) => {
                    tracing::warn!("judge engine unavailable: {err}; judge assertions will defer");
                    None
                }
            }
        }
        _ => None,
    };

    let judge_dir = args.project.join("evals").join("judges").join(&report_id);
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
        run_id: &report_id,
        judge_output_dir: Some(&judge_dir),
    })
    .await;

    let report_dir = args.project.join("evals").join("reports");
    let report_path = report_dir.join(format!("{report_id}.json"));
    // Create the report file's actual parent so a key with separators (or a
    // missing `evals/reports/`) does not fail the write with NotFound.
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|source| EvalCmdError::Report {
            path: parent.to_path_buf(),
            source,
        })?;
    }
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

/// Derive the per-run report id from the `--over` target: the directory
/// basename when `over` is a directory, or the file stem when it is a single
/// conversation file. This is the identity `report` reads
/// (`evals/reports/<report_id>.json`) and the e2e scripts pass on the command
/// line — deliberately distinct from the project-relative listing key, which
/// carries the `runs/` segment so the repository can resolve the run dir.
///
/// Falls back to the whole path string for the (CLI-unreachable) empty-path
/// case so a `report_id` is always produced.
fn report_id_for(over: &std::path::Path) -> String {
    // Directory: the full basename (run-dir names like
    // `<ts>-<uuid6>-<name>` carry no extension to strip). Single file: the
    // stem, so `missing-fields.yaml` keys the report as `missing-fields`.
    let component = if over.is_dir() {
        over.file_name()
    } else {
        over.file_stem().or_else(|| over.file_name())
    };
    component
        .and_then(|s| s.to_str())
        .map_or_else(|| over.to_string_lossy().into_owned(), str::to_string)
}

/// Stable `PathBuf` identity for a conversation key, used as the per-row
/// identifier in the eval report (`{run_id}/{name}.yaml`).
fn key_path(key: &ConversationKey) -> PathBuf {
    if key.run_id.as_str().is_empty() {
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
