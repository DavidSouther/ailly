//! Handler for `ailly run <conversation.yaml | run-dir>`.
//!
//! Walks each conversation in `target`, asks the engine to fill any blank
//! assistant slot, and writes the result back in place. The conversation file
//! is the run artifact; no parallel `response.json`, `meta.yaml`, or
//! `window.txt` is produced. See `docs/developer/2026-05-24-A-run-cmd/`.

use std::path::PathBuf;

use crate::content::conversation::Conversation;
use crate::content::conversation::ConversationError;
use crate::content::conversation::Role;
use crate::content::conversation::RunError;
use crate::content::repository::ConversationRepository;
use crate::content::repository::RepositoryError;
use crate::content::repository::VfsConversationRepository;
use crate::engine::engine::EngineError;
use crate::engine::engine::EngineProvider;
use crate::engine::engine::open_engine_for_model;

/// Arguments for the run handler. `project` is accepted for symmetry with
/// `AssembleArgs` and forward compatibility; the handler resolves `target`
/// against the current working directory.
#[derive(Clone, Debug)]
pub struct RunArgs {
    pub project: PathBuf,
    pub target: PathBuf,
}

/// Outcome counters returned to the library caller. The CLI binary does not
/// display the value; tests use it as a structured assertion seam.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunOutcome {
    pub conversations_processed: usize,
    pub blank_assistants_filled: usize,
}

/// Closed library-boundary error type. Disambiguated from
/// `content::conversation::RunError` (the aggregate-side error) by the
/// `Cmd` suffix; `From` impls peel inner errors into the variants below.
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
}

impl From<RunError> for RunCmdError {
    fn from(err: RunError) -> Self {
        match err {
            RunError::Engine(inner) => Self::Engine(inner),
            RunError::Conversation(inner) => Self::Conversation(inner),
        }
    }
}

/// Drive the run pipeline against `args.target`, constructing one engine per
/// conversation from its `meta.model`. The duplicated loop body relative to
/// [`run_with_engine`] is deliberate: heterogeneous models across bindings
/// work without further refactoring because each conversation's engine is
/// resolved against its own `meta.model`.
///
/// # Errors
/// Returns [`RunCmdError::Repository`] when listing, loading, or saving fails;
/// [`RunCmdError::Engine`] when the model id has no factory branch or the
/// engine cannot serve a slot; and [`RunCmdError::Conversation`] when the
/// aggregate rejects a fill.
pub async fn run(args: RunArgs) -> Result<RunOutcome, RunCmdError> {
    let project = crate::content::project::Project::open(&args.project)?;
    let repo = VfsConversationRepository;
    let target = resolve_target(&project, &args.target)?;
    let paths = repo.list(&target)?;
    let mut outcome = RunOutcome::default();
    for path in &paths {
        let mut conv = repo.load(path)?;
        let engine = open_engine_for_model(&conv.meta.model)?;
        fill_and_save(&repo, path, &mut conv, engine.as_ref(), &mut outcome).await?;
    }
    Ok(outcome)
}

/// Drive the run pipeline against `args.target` using a caller-supplied
/// engine.
///
/// Lists conversation files under `args.target`, loads each in turn, asks the
/// engine to fill any blank assistant slot, and writes the result back in
/// place. Engine failures bubble after the in-memory progress is persisted,
/// so a partial run leaves earlier slots filled on disk.
///
/// # Errors
/// Returns [`RunCmdError::Repository`] when listing, loading, or saving fails;
/// [`RunCmdError::Engine`] when the engine cannot serve a slot; and
/// [`RunCmdError::Conversation`] when the aggregate rejects a fill.
pub async fn run_with_engine(
    args: RunArgs,
    engine: Box<dyn EngineProvider>,
) -> Result<RunOutcome, RunCmdError> {
    let project = crate::content::project::Project::open(&args.project)?;
    let repo = VfsConversationRepository;
    let target = resolve_target(&project, &args.target)?;
    let paths = repo.list(&target)?;
    let mut outcome = RunOutcome::default();
    for path in &paths {
        let mut conv = repo.load(path)?;
        fill_and_save(&repo, path, &mut conv, engine.as_ref(), &mut outcome).await?;
    }
    Ok(outcome)
}

async fn fill_and_save(
    repo: &VfsConversationRepository,
    path: &vfs::VfsPath,
    conv: &mut Conversation,
    engine: &dyn EngineProvider,
    outcome: &mut RunOutcome,
) -> Result<(), RunCmdError> {
    let blanks_before = count_blank_assistants(conv);
    let engine_result = conv.run(engine).await;
    let blanks_after = count_blank_assistants(conv);
    repo.save(path, conv)?;
    engine_result?;
    outcome.conversations_processed += 1;
    outcome.blank_assistants_filled += blanks_before - blanks_after;
    Ok(())
}

/// Resolve `target` to a [`vfs::VfsPath`]. Absolute paths mount onto a
/// host-rooted `vfs::PhysicalFS::new("/")`; relative paths join onto
/// `project.root()` (the project's vfs mount). UTF-8 invalid bytes in
/// the input are an explicit error at the CLI argument boundary.
fn resolve_target(
    project: &crate::content::project::Project,
    target: &std::path::Path,
) -> Result<vfs::VfsPath, RunCmdError> {
    let target_str = target.to_str().ok_or_else(|| RunCmdError::NonUtf8Path {
        path: target.to_path_buf(),
    })?;
    if target.is_absolute() {
        let host = vfs::VfsPath::new(vfs::PhysicalFS::new("/"));
        host.join(target_str.trim_start_matches('/'))
            .map_err(|source| RepositoryError::Vfs {
                path: target_str.to_string(),
                source,
            })
            .map_err(RunCmdError::from)
    } else {
        project
            .root()
            .join(target_str)
            .map_err(|source| RepositoryError::Vfs {
                path: target_str.to_string(),
                source,
            })
            .map_err(RunCmdError::from)
    }
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
        }
    }

    #[tokio::test]
    async fn single_file_target_with_one_blank_assistant_fills_in_place() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("conv.yaml");
        write_conversation(&path, None, false);

        let engine = NoopEngine::from_replies(["hello"]);
        let outcome = run_with_engine(args_for(&path), Box::new(engine))
            .await
            .expect("run");

        assert_eq!(outcome.conversations_processed, 1);
        assert_eq!(outcome.blank_assistants_filled, 1);

        let body = fs::read_to_string(&path).expect("read back");
        let conv = Conversation::from_yaml_str(&body).expect("parse back");
        assert!(conv.next_blank_assistant().is_none(), "no blanks remain");
        let assistant = conv.session.last().expect("trailing message");
        match assistant.body.as_ref().expect("assistant filled") {
            Content::Text(text) => assert_eq!(text, "hello"),
            Content::Blocks(_) => panic!("from_replies produces Content::Text"),
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

        let engine = NoopEngine::from_replies(["alpha", "beta", "gamma"]);
        let outcome = run_with_engine(args_for(tmp.path()), Box::new(engine))
            .await
            .expect("run dir");

        assert_eq!(outcome.conversations_processed, 3);
        assert_eq!(outcome.blank_assistants_filled, 3);

        for (name, reply) in [("a.yaml", "alpha"), ("b.yaml", "beta"), ("c.yaml", "gamma")] {
            let body = fs::read_to_string(tmp.path().join(name)).expect("read back");
            let conv = Conversation::from_yaml_str(&body).expect("parse");
            let assistant = conv.session.last().expect("assistant");
            match assistant.body.as_ref().expect("filled") {
                Content::Text(text) => assert_eq!(text, reply, "reply for {name}"),
                Content::Blocks(_) => panic!("expected Text in {name}"),
            }
        }
    }

    #[tokio::test]
    async fn no_blank_assistant_is_a_byte_identical_no_op() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("conv.yaml");
        write_conversation(&path, Some("already filled"), false);
        let before = fs::read(&path).expect("read before");

        let engine = NoopEngine::from_replies::<[&str; 0], &str>([]);
        let outcome = run_with_engine(args_for(&path), Box::new(engine))
            .await
            .expect("run no-op");

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

        let engine = NoopEngine::from_replies(["first"]);
        let result = run_with_engine(args_for(&path), Box::new(engine)).await;

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
    async fn non_claude_model_returns_model_not_found() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let path = tmp.path().join("conv.yaml");
        let conv = Conversation {
            meta: Meta {
                model: ModelId::from("gpt-5-turbo"),
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
                assert_eq!(model, ModelId::from("gpt-5-turbo"));
            }
            Err(other) => panic!("expected ModelNotFound, got {other:?}"),
            Ok(outcome) => panic!("expected error, got {outcome:?}"),
        }
    }

    #[tokio::test]
    async fn target_neither_file_nor_dir_returns_target_not_found() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let missing = tmp.path().join("nope.yaml");

        let engine = NoopEngine::from_replies::<[&str; 0], &str>([]);
        let result = run_with_engine(args_for(&missing), Box::new(engine)).await;

        match result {
            Err(RunCmdError::Repository(RepositoryError::TargetNotFound { path })) => {
                assert_eq!(path, missing);
            }
            Err(other) => panic!("expected Repository(TargetNotFound), got {other:?}"),
            Ok(outcome) => panic!("expected error, got {outcome:?}"),
        }
    }
}
