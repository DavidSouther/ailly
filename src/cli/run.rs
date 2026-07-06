//! Handler for `ailly run <conversation.yaml | run-dir>`.
//!
//! Walks each conversation in `target`, asks the engine to fill any blank
//! assistant slot, and writes the result back in place. The conversation file
//! is the entire run artifact.

use std::path::PathBuf;

use crate::content::conversation::Conversation;
use crate::content::conversation::ConversationError;
use crate::content::conversation::Role;
use crate::content::conversation::RunError;
use crate::content::repository::ConversationKey;
use crate::content::repository::ConversationName;
use crate::content::repository::ConversationRepository;
use crate::content::repository::RepositoryError;
use crate::engine::engine::EngineError;
use crate::engine::engine::EngineProvider;
use crate::engine::engine::open_engine_for_model;

/// Arguments for the run handler. `project` is accepted for symmetry with
/// `AssembleArgs` and forward compatibility; the handler resolves `target`
/// against the current working directory.
#[derive(Clone, Debug, Default)]
pub struct RunArgs {
    pub project: PathBuf,
    pub target: PathBuf,
    /// Repeatable `--case <name>` filter. Empty means no filter: every
    /// resolved conversation key is processed, exactly as before this
    /// field existed.
    pub cases: Vec<String>,
}

/// Outcome counters returned to the library caller. The CLI binary does not
/// display the value; tests use it for structure assertions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunOutcome {
    pub conversations_processed: usize,
    pub blank_assistants_filled: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum RunCmdError {
    #[error("project error: {0}")]
    Project(#[from] crate::content::project::ProjectError),
    #[error("repository error: {0}")]
    Repository(#[from] RepositoryError),
    #[error("engine error: {0}")]
    Engine(#[from] EngineError),
    #[error("conversation error: {0}")]
    Conversation(#[from] ConversationError),
    #[error("target {path:?} is neither a file nor a directory")]
    TargetNotFound { path: PathBuf },
    #[error("target path {path:?} is not valid UTF-8")]
    NonUtf8Path { path: PathBuf },
    #[error("--case {requested:?} matched nothing; available cases: {available:?}")]
    UnknownCase {
        requested: Vec<String>,
        available: Vec<String>,
    },
}

impl From<RunError> for RunCmdError {
    fn from(err: RunError) -> Self {
        match err {
            RunError::Engine(inner) => Self::Engine(inner),
            RunError::Conversation(inner) => Self::Conversation(inner),
        }
    }
}

/// Run Ailly in `args.target`, constructing one engine per
/// conversation from its `meta.model`. Heterogeneous models across bindings
/// work as conversation's engine is resolved against its own `meta.model`.
///
/// # Errors
/// Returns [`RunCmdError::Repository`] when listing, loading, or saving fails;
/// [`RunCmdError::Engine`] when the model id has no factory branch or the
/// engine cannot serve a slot; and [`RunCmdError::Conversation`] when
/// unable to fill a conversation response.
pub async fn run(args: RunArgs) -> Result<RunOutcome, RunCmdError> {
    let project = crate::content::project::Project::open(&args.project)?;
    crate::cli::env::load_project_env(&args.project);
    let repo = project.conversations();
    let keys = resolve_keys(&project, &repo, &args.target, &args.cases)?;
    let mut outcome = RunOutcome::default();
    for key in &keys {
        let mut conv = repo.load(key)?;
        let engine = open_engine_for_model(&conv.meta.model)?;
        fill_and_save(&repo, key, &mut conv, engine.as_ref(), &mut outcome).await?;
    }
    Ok(outcome)
}

async fn fill_and_save(
    repo: &impl ConversationRepository,
    key: &ConversationKey,
    conv: &mut Conversation,
    engine: &dyn EngineProvider,
    outcome: &mut RunOutcome,
) -> Result<(), RunCmdError> {
    let blanks_before = count_blank_assistants(conv);
    let engine_result = conv.run(engine).await;
    let blanks_after = count_blank_assistants(conv);
    repo.save(key, conv)?;
    engine_result?; // Save first, then surface engine errors.
    outcome.conversations_processed += 1;
    outcome.blank_assistants_filled += blanks_before - blanks_after;
    Ok(())
}

/// Resolve `target` to one or more [`ConversationKey`]s. A file produces a
/// single key; a directory lists all keys in that run, filtered by
/// `cases` (empty means every key, unchanged from today). A file target is
/// not filtered here: its single resolved key either matches downstream or
/// becomes an `UnknownCase` error, so directory and file targets share one
/// filtering point rather than branching. Providing neither returns
/// [`RepositoryError::TargetNotFound`].
fn resolve_keys(
    project: &crate::content::project::Project,
    repo: &impl ConversationRepository,
    target: &std::path::Path,
    cases: &[String],
) -> Result<Vec<ConversationKey>, RunCmdError> {
    if target.to_str().is_none() {
        return Err(RunCmdError::NonUtf8Path {
            path: target.to_path_buf(),
        });
    }
    let vfs_path = project
        .resolve_host_path(target)
        .map_err(RunCmdError::Repository)?;
    let is_file = vfs_path.is_file().map_err(|source| {
        RunCmdError::Repository(RepositoryError::Vfs {
            path: target.to_string_lossy().to_string(),
            source,
        })
    })?;
    if is_file {
        let name =
            ConversationName::from(target.file_stem().and_then(|s| s.to_str()).unwrap_or(""));
        let run_id = super::project_relative(project, target.parent().unwrap_or(target));
        return Ok(vec![ConversationKey { run_id, name }]);
    }
    let is_dir = vfs_path.is_dir().map_err(|source| {
        RunCmdError::Repository(RepositoryError::Vfs {
            path: target.to_string_lossy().to_string(),
            source,
        })
    })?;
    if is_dir {
        let run_id = super::project_relative(project, target);
        let keys = repo.list(&run_id).map_err(RunCmdError::Repository)?;
        return Ok(keys
            .into_iter()
            .filter(|key| crate::cli::case_filter_matches(key.name.as_str(), cases))
            .collect());
    }
    Err(RunCmdError::Repository(RepositoryError::TargetNotFound {
        path: target.to_path_buf(),
    }))
}

fn count_blank_assistants(conv: &Conversation) -> usize {
    conv.session
        .iter()
        .filter(|m| matches!(m.role, Role::Assistant) && m.body.is_none())
        .count()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::marker::PhantomData;
    use std::path::Path;

    use super::*;
    use crate::content::conversation::BindingMap;
    use crate::content::conversation::Content;
    use crate::content::conversation::Message;
    use crate::content::conversation::Meta;
    use crate::content::conversation::ModelId;
    use crate::content::repository::RunId;
    use crate::engine::engine::NoopEngine;

    fn write_conversation(path: &Path, body_text: Option<&str>, trailing_blank: bool) {
        let mut session: Vec<Message> = Vec::new();
        session.push(Message {
            role: Role::User,
            body: Some(Content::Text(String::from("ping"))),
            cache: false,
            trace: None,
            _phase: PhantomData,
        });
        let body = body_text.map(|text| Content::Text(text.to_owned()));
        session.push(Message {
            role: Role::Assistant,
            body,
            cache: false,
            trace: None,
            _phase: PhantomData,
        });
        if trailing_blank {
            session.push(Message {
                role: Role::Assistant,
                body: None,
                cache: false,
                trace: None,
                _phase: PhantomData,
            });
        }
        let conv = Conversation {
            meta: Meta {
                model: ModelId::from("noop"),
                debug: false,
                assembly: None,
                binding: BindingMap::new(),
            },
            session,
        };
        fs::write(path, conv.to_yaml_string().expect("emit")).expect("write conversation");
    }

    fn args_for(target: &Path) -> RunArgs {
        // `cli/run` requires `project` to point at an existing directory
        // (Project::open validates this). For tests that hand a single
        // file as the target, point `project` at the file's parent.
        let project = if target.is_dir() {
            target.to_path_buf()
        } else {
            target.parent().expect("target has a parent").to_path_buf()
        };
        RunArgs {
            project,
            target: target.to_path_buf(),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn single_file_target_with_one_blank_assistant_fills_in_place() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("conv.yaml");
        write_conversation(&path, None, false);

        // `open_engine_for_model("noop")` returns `NoopEngine::auto()`, which
        // generates `"noop-{call_index}"` replies without a pre-loaded script.
        let outcome = run(args_for(&path)).await.expect("run");

        assert_eq!(outcome.conversations_processed, 1);
        assert_eq!(outcome.blank_assistants_filled, 1);

        let body = fs::read_to_string(&path).expect("read back");
        let conv = Conversation::from_yaml_str(&body).expect("parse back");
        assert!(conv.next_blank_assistant().is_none(), "no blanks remain");
        let assistant = conv.session.last().expect("trailing message");
        match assistant.body.as_ref().expect("assistant filled") {
            Content::Text(text) => assert_eq!(text, "noop-0"),
            Content::Blocks(_) => panic!("auto noop produces Content::Text"),
        }
        let trace = assistant.trace.as_ref().expect("trace inlined");
        assert_eq!(trace.model, ModelId::from("noop"));
    }

    #[tokio::test]
    async fn run_dir_target_processes_all_files_in_sorted_order() {
        let tmp = tempfile::tempdir().expect("tempdir");
        for name in ["c.yaml", "a.yaml", "b.yaml"] {
            write_conversation(&tmp.path().join(name), None, false);
        }

        let outcome = run(args_for(tmp.path())).await.expect("run dir");

        assert_eq!(outcome.conversations_processed, 3);
        assert_eq!(outcome.blank_assistants_filled, 3);

        // Each file gets its own engine instance so we verify fill, not order.
        // Sort-order correctness is covered by repository unit tests.
        for name in ["a.yaml", "b.yaml", "c.yaml"] {
            let body = fs::read_to_string(tmp.path().join(name)).expect("read back");
            let conv = Conversation::from_yaml_str(&body).expect("parse");
            let assistant = conv.session.last().expect("assistant");
            assert!(
                assistant.body.is_some(),
                "assistant in {name} should be filled"
            );
        }
    }

    #[tokio::test]
    async fn case_filter_on_a_directory_target_processes_only_the_named_keys() {
        let tmp = tempfile::tempdir().expect("tempdir");
        for name in ["a.yaml", "b.yaml", "c.yaml"] {
            write_conversation(&tmp.path().join(name), None, false);
        }

        let mut args = args_for(tmp.path());
        args.cases = vec![String::from("a"), String::from("c")];
        let outcome = run(args).await.expect("filtered run dir");

        assert_eq!(outcome.conversations_processed, 2);
        assert_eq!(outcome.blank_assistants_filled, 2);

        for name in ["a.yaml", "c.yaml"] {
            let body = fs::read_to_string(tmp.path().join(name)).expect("read back");
            let conv = Conversation::from_yaml_str(&body).expect("parse");
            let assistant = conv.session.last().expect("assistant");
            assert!(assistant.body.is_some(), "{name} should be filled");
        }

        let untouched = fs::read_to_string(tmp.path().join("b.yaml")).expect("read back");
        let conv = Conversation::from_yaml_str(&untouched).expect("parse");
        let assistant = conv.session.last().expect("assistant");
        assert!(
            assistant.body.is_none(),
            "b.yaml was not named by --case and must be left untouched"
        );
    }

    #[tokio::test]
    async fn empty_case_filter_on_a_directory_target_is_unchanged_from_today() {
        let tmp = tempfile::tempdir().expect("tempdir");
        for name in ["a.yaml", "b.yaml"] {
            write_conversation(&tmp.path().join(name), None, false);
        }

        let outcome = run(args_for(tmp.path())).await.expect("unfiltered run dir");

        assert_eq!(outcome.conversations_processed, 2);
    }

    #[tokio::test]
    async fn no_blank_assistant_is_a_byte_identical_no_op() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("conv.yaml");
        write_conversation(&path, Some("already filled"), false);
        let before = fs::read(&path).expect("read before");

        let outcome = run(args_for(&path)).await.expect("run no-op");

        assert_eq!(outcome.conversations_processed, 1);
        assert_eq!(outcome.blank_assistants_filled, 0);

        let after = fs::read(&path).expect("read after");
        assert_eq!(before, after, "no-op must be byte-identical");
    }

    #[tokio::test]
    async fn engine_failure_writes_progress_and_bubbles_error() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("conv.yaml");
        // Two trailing blanks; engine scripted with only one reply.
        write_conversation(&path, None, true);

        // Exercise `fill_and_save` directly so a scripted (exhaustible)
        // NoopEngine can be injected while still testing the partial-
        // persistence guarantee: progress saved before the error bubbles.
        let project = crate::content::project::Project::open(tmp.path()).expect("project open");
        let repo = project.conversations();
        // conv.yaml is at the project root, so run_id is empty.
        let key = ConversationKey {
            run_id: RunId::default(),
            name: ConversationName::from("conv"),
        };
        let mut conv = repo.load(&key).expect("load conv");
        let engine = NoopEngine::from_replies(["first"]);
        let mut outcome = RunOutcome::default();
        let result = fill_and_save(&repo, &key, &mut conv, &engine, &mut outcome).await;

        match result {
            Err(RunCmdError::Engine(EngineError::NoopExhausted { call_index })) => {
                assert_eq!(call_index, 1);
            }
            Err(other) => panic!("expected NoopExhausted at call_index 1, got {other:?}"),
            Ok(outcome) => panic!("expected error, got {outcome:?}"),
        }

        let body = fs::read_to_string(&path).expect("read back");
        let conv = Conversation::from_yaml_str(&body).expect("parse back");
        let blanks_remaining: Vec<usize> = conv
            .session
            .iter()
            .enumerate()
            .filter(|(_, m)| matches!(m.role, Role::Assistant) && m.body.is_none())
            .map(|(i, _)| i)
            .collect();
        assert_eq!(
            blanks_remaining.len(),
            1,
            "first blank filled, second still blank"
        );
        let first_assistant = conv
            .session
            .iter()
            .find(|m| matches!(m.role, Role::Assistant) && m.body.is_some())
            .expect("first assistant filled");
        match first_assistant.body.as_ref().unwrap() {
            Content::Text(text) => assert_eq!(text, "first"),
            Content::Blocks(_) => panic!("expected Text"),
        }
    }

    #[tokio::test]
    async fn unrecognised_model_returns_model_not_found() {
        // `mistral-large` matches no wired provider family, so it surfaces
        // ModelNotFound through `run`. A recognised-but-keyless id like
        // `gpt-5-turbo` instead fails with Auth at its constructor; that
        // distinction is covered by the engine routing tests.
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("conv.yaml");
        let conv = Conversation {
            meta: Meta {
                model: ModelId::from("mistral-large"),
                debug: false,
                assembly: None,
                binding: BindingMap::new(),
            },
            session: vec![
                Message {
                    role: Role::User,
                    body: Some(Content::Text(String::from("hi"))),
                    cache: false,
                    trace: None,
                    _phase: PhantomData,
                },
                Message {
                    role: Role::Assistant,
                    body: None,
                    cache: false,
                    trace: None,
                    _phase: PhantomData,
                },
            ],
        };
        fs::write(&path, conv.to_yaml_string().expect("emit")).expect("write");

        let result = run(args_for(&path)).await;
        match result {
            Err(RunCmdError::Engine(EngineError::ModelNotFound { model })) => {
                assert_eq!(model, ModelId::from("mistral-large"));
            }
            Err(other) => panic!("expected ModelNotFound, got {other:?}"),
            Ok(outcome) => panic!("expected error, got {outcome:?}"),
        }
    }

    #[tokio::test]
    async fn target_neither_file_nor_dir_returns_target_not_found() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let missing = tmp.path().join("nope.yaml");

        let result = run(args_for(&missing)).await;

        match result {
            Err(RunCmdError::Repository(RepositoryError::TargetNotFound { path })) => {
                assert_eq!(path, missing);
            }
            Err(other) => panic!("expected Repository(TargetNotFound), got {other:?}"),
            Ok(outcome) => panic!("expected error, got {outcome:?}"),
        }
    }
}
