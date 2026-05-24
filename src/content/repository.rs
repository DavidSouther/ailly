//! Repository ports for `cli/assemble` and their `std::fs` adapters.
//!
//! See `docs/developer/2026-05-23-A-cli-assemble/plan.md` Step 2. The
//! `content/project.rs layout` slice replaces these adapters with a shared
//! `Project` value without changing the trait shapes.

use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

use crate::content::assembly::Assembly;
use crate::content::assembly::AssemblyError;
use crate::content::assembly::Binding;
use crate::content::conversation::Conversation;
use crate::content::conversation::ConversationError;

/// Loads parsed [`Assembly`] aggregates by name.
pub trait AssemblyRepository {
    /// Read `<project>/assemblies/<name>.yaml` and parse it.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Read`] if the file is missing or unreadable
    /// and [`RepositoryError::Parse`] if the file is not a valid assembly.
    fn get(&self, name: &str) -> Result<Assembly, RepositoryError>;
}

/// Reads and globs prefix-block content files relative to the project root.
pub trait ContextRepository {
    /// Read a single file as UTF-8 text.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Read`] if the file is missing or unreadable.
    fn read_file(&self, path: &str) -> Result<String, RepositoryError>;

    /// Expand `pattern` and concatenate matched files in filename-ascending
    /// order, separated by a single newline. When `limit` is `Some(n)`, only
    /// the first `n` paths (after sorting) are read and concatenated.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Glob`] if the pattern is malformed and
    /// [`RepositoryError::Read`] if a matched file is unreadable.
    fn glob_concat(
        &self,
        pattern: &str,
        limit: Option<usize>,
    ) -> Result<GlobResult, RepositoryError>;
}

/// Buffers per-binding conversations and writes them as a fresh run directory.
/// Writes are deferred until [`RunRepository::flush`]; before `flush`, nothing
/// touches disk.
pub trait RunRepository {
    /// Stage one conversation for a binding. The filename is derived from the
    /// binding by joining values in axis order with `-`. The empty binding
    /// maps to `default.yaml`.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Emit`] when the conversation cannot be
    /// serialized.
    fn stage(
        &mut self,
        binding: &Binding,
        conversation: &Conversation,
    ) -> Result<(), RepositoryError>;

    /// Create `<project>/runs/<id>/` and write every staged conversation.
    /// Returns the run directory path.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::CreateDir`] or [`RepositoryError::Write`]
    /// when the underlying filesystem call fails.
    fn flush(&mut self, assembly_name: &str) -> Result<PathBuf, RepositoryError>;

    /// Drop staged writes without touching disk.
    fn discard(&mut self);
}

/// Result of a glob expansion: matched paths and their concatenated content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlobResult {
    pub paths: Vec<PathBuf>,
    pub body: String,
}

/// Errors emitted by repository ports.
#[derive(Debug, thiserror::Error)]
pub enum RepositoryError {
    #[error("reading {path:?}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("parsing {path:?}: {source}")]
    Parse {
        path: PathBuf,
        #[source]
        source: AssemblyError,
    },
    #[error("glob pattern {pattern:?}: {source}")]
    Glob {
        pattern: String,
        #[source]
        source: glob::PatternError,
    },
    #[error("creating directory {path:?}: {source}")]
    CreateDir {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("writing {path:?}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("emitting conversation: {source}")]
    Emit {
        #[source]
        source: ConversationError,
    },
}

/// `std::fs`-backed [`AssemblyRepository`] rooted at a project directory.
pub struct FsAssemblyRepository {
    root: PathBuf,
}

impl FsAssemblyRepository {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn path_for(&self, name: &str) -> PathBuf {
        self.root.join("assemblies").join(format!("{name}.yaml"))
    }
}

impl AssemblyRepository for FsAssemblyRepository {
    fn get(&self, name: &str) -> Result<Assembly, RepositoryError> {
        let path = self.path_for(name);
        let body = fs::read_to_string(&path).map_err(|source| RepositoryError::Read {
            path: path.clone(),
            source,
        })?;
        Assembly::from_yaml_str(&body).map_err(|source| RepositoryError::Parse { path, source })
    }
}

/// `std::fs`-backed [`ContextRepository`] rooted at a project directory.
pub struct FsContextRepository {
    root: PathBuf,
}

impl FsContextRepository {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    fn resolve(&self, path: &str) -> PathBuf {
        self.root.join(path)
    }
}

impl ContextRepository for FsContextRepository {
    fn read_file(&self, path: &str) -> Result<String, RepositoryError> {
        let resolved = self.resolve(path);
        fs::read_to_string(&resolved).map_err(|source| RepositoryError::Read {
            path: resolved,
            source,
        })
    }

    fn glob_concat(
        &self,
        pattern: &str,
        limit: Option<usize>,
    ) -> Result<GlobResult, RepositoryError> {
        let resolved_pattern = self.resolve(pattern);
        let pattern_str = resolved_pattern.to_string_lossy().to_string();
        let entries = glob::glob(&pattern_str).map_err(|source| RepositoryError::Glob {
            pattern: pattern_str.clone(),
            source,
        })?;

        let mut paths: Vec<PathBuf> = Vec::new();
        for entry in entries {
            match entry {
                Ok(path) => paths.push(path),
                Err(err) => {
                    return Err(RepositoryError::Read {
                        path: err.path().to_path_buf(),
                        source: err.into_error(),
                    });
                }
            }
        }
        paths.sort();
        if let Some(n) = limit
            && paths.len() > n
        {
            paths.truncate(n);
        }

        let mut bodies: Vec<String> = Vec::with_capacity(paths.len());
        for path in &paths {
            let body = fs::read_to_string(path).map_err(|source| RepositoryError::Read {
                path: path.clone(),
                source,
            })?;
            bodies.push(body);
        }
        let body = bodies.join("\n");
        Ok(GlobResult { paths, body })
    }
}

/// `std::fs`-backed [`RunRepository`] rooted at a project directory.
pub struct FsRunRepository {
    root: PathBuf,
    staged: Vec<(String, String)>,
}

impl FsRunRepository {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            staged: Vec::new(),
        }
    }
}

impl RunRepository for FsRunRepository {
    fn stage(
        &mut self,
        binding: &Binding,
        conversation: &Conversation,
    ) -> Result<(), RepositoryError> {
        let filename = filename_for(binding);
        let body = conversation
            .to_yaml_string()
            .map_err(|source| RepositoryError::Emit { source })?;
        self.staged.push((filename, body));
        Ok(())
    }

    fn flush(&mut self, assembly_name: &str) -> Result<PathBuf, RepositoryError> {
        let run_dir = self.root.join("runs").join(run_id(assembly_name));
        fs::create_dir_all(&run_dir).map_err(|source| RepositoryError::CreateDir {
            path: run_dir.clone(),
            source,
        })?;
        for (filename, body) in self.staged.drain(..) {
            let path = run_dir.join(&filename);
            fs::write(&path, body).map_err(|source| RepositoryError::Write { path, source })?;
        }
        Ok(run_dir)
    }

    fn discard(&mut self) {
        self.staged.clear();
    }
}

/// Build the filename for a binding under a run directory. Binding values are
/// joined in axis order with `-`; the empty binding maps to `default.yaml`.
pub(crate) fn filename_for(binding: &Binding) -> String {
    if binding.values.is_empty() {
        return String::from("default.yaml");
    }
    let parts: Vec<String> = binding
        .values
        .values()
        .map(stringify_binding_value)
        .collect();
    format!("{}.yaml", parts.join("-"))
}

fn stringify_binding_value(value: &serde_yaml_ng::Value) -> String {
    match value {
        serde_yaml_ng::Value::String(s) => s.clone(),
        serde_yaml_ng::Value::Bool(b) => b.to_string(),
        serde_yaml_ng::Value::Number(n) => n.to_string(),
        serde_yaml_ng::Value::Null => String::from("null"),
        other => {
            let raw = serde_yaml_ng::to_string(other).unwrap_or_default();
            raw.trim().trim_matches('"').trim_matches('\'').to_string()
        }
    }
}

fn run_id(assembly_name: &str) -> String {
    let now = chrono::Utc::now();
    let ts = now.format("%Y-%m-%dT%H-%M-%SZ").to_string();
    format!("{ts}-{assembly_name}")
}

/// Open the three `std::fs`-backed adapters for `project_root` as boxed
/// trait objects so callers can hold them through a single value.
#[must_use]
pub fn open_fs_repositories(
    project_root: &Path,
) -> (
    Box<dyn AssemblyRepository>,
    Box<dyn ContextRepository>,
    Box<dyn RunRepository>,
) {
    (
        Box::new(FsAssemblyRepository::new(project_root.to_path_buf())),
        Box::new(FsContextRepository::new(project_root.to_path_buf())),
        Box::new(FsRunRepository::new(project_root.to_path_buf())),
    )
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::marker::PhantomData;

    use super::*;
    use crate::content::conversation::BindingMap;
    use crate::content::conversation::Message;
    use crate::content::conversation::Meta;
    use crate::content::conversation::ModelId;
    use crate::content::conversation::Role;

    const CLAIM_HANDLER_FIXTURE: &str = "\
name: claim-handler
model: claude-opus-4-7
matrix:
  case: [default]
prefix:
  - { kind: file, path: AGENTS.md, cache: true }
conversation:
  - { role: user, path: \"prompts/{{ case }}.md\" }
  - { role: assistant }
";

    fn write_fixture(root: &Path) {
        let assemblies = root.join("assemblies");
        fs::create_dir_all(&assemblies).expect("mkdir assemblies");
        fs::write(assemblies.join("claim-handler.yaml"), CLAIM_HANDLER_FIXTURE).expect("write");
    }

    fn empty_conversation() -> Conversation {
        Conversation {
            meta: Meta {
                model: ModelId::from("m"),
                debug: false,
                assembly: None,
                binding: BindingMap::new(),
            },
            session: vec![Message {
                role: Role::Assistant,
                body: None,
                cache: false,
                trace: None,
                _phase: PhantomData,
            }],
        }
    }

    #[test]
    fn fs_assembly_repository_reads_and_parses() {
        let tmp = tempfile::tempdir().expect("tempdir");
        write_fixture(tmp.path());

        let repo = FsAssemblyRepository::new(tmp.path().to_path_buf());
        let assembly = repo.get("claim-handler").expect("loads");
        assert_eq!(assembly.name, "claim-handler");
    }

    #[test]
    fn fs_assembly_repository_missing_returns_read_error() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo = FsAssemblyRepository::new(tmp.path().to_path_buf());
        let err = repo.get("nope").expect_err("missing");
        assert!(matches!(err, RepositoryError::Read { .. }), "got {err:?}");
    }

    #[test]
    fn fs_context_repository_reads_a_file_byte_for_byte() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let body = "hello\nworld\n";
        fs::write(tmp.path().join("a.md"), body).expect("write");

        let repo = FsContextRepository::new(tmp.path().to_path_buf());
        let got = repo.read_file("a.md").expect("read");
        assert_eq!(got, body);
    }

    #[test]
    fn fs_context_repository_glob_concat_sorts_and_joins_with_newline() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("ctx");
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(dir.join("b.md"), "second").expect("write b");
        fs::write(dir.join("a.md"), "first").expect("write a");

        let repo = FsContextRepository::new(tmp.path().to_path_buf());
        let result = repo.glob_concat("ctx/*.md", None).expect("glob");
        assert_eq!(result.paths.len(), 2);
        assert!(result.paths[0].ends_with("a.md"));
        assert!(result.paths[1].ends_with("b.md"));
        assert_eq!(result.body, "first\nsecond");
    }

    #[test]
    fn fs_context_repository_glob_concat_truncates_after_sort() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path().join("ctx");
        fs::create_dir_all(&dir).expect("mkdir");
        // Write in a different order than the desired post-sort order to
        // demonstrate that truncation happens after the sort, not before.
        fs::write(dir.join("c.md"), "third").expect("write c");
        fs::write(dir.join("a.md"), "first").expect("write a");
        fs::write(dir.join("b.md"), "second").expect("write b");

        let repo = FsContextRepository::new(tmp.path().to_path_buf());
        let result = repo.glob_concat("ctx/*.md", Some(2)).expect("glob");
        assert_eq!(result.paths.len(), 2);
        assert!(result.paths[0].ends_with("a.md"));
        assert!(result.paths[1].ends_with("b.md"));
        assert_eq!(result.body, "first\nsecond");
    }

    #[test]
    fn fs_run_repository_stage_then_flush_writes_one_file_per_stage() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut repo = FsRunRepository::new(tmp.path().to_path_buf());

        let mut b1 = Binding::default();
        b1.values
            .insert("case".to_string(), serde_yaml_ng::Value::from("alpha"));
        let mut b2 = Binding::default();
        b2.values
            .insert("case".to_string(), serde_yaml_ng::Value::from("beta"));

        let conv = empty_conversation();
        repo.stage(&b1, &conv).expect("stage 1");
        repo.stage(&b2, &conv).expect("stage 2");

        let run_dir = repo.flush("claim-handler").expect("flush");
        let entries: Vec<_> = fs::read_dir(&run_dir)
            .expect("read run dir")
            .filter_map(Result::ok)
            .collect();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn fs_run_repository_discard_writes_nothing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut repo = FsRunRepository::new(tmp.path().to_path_buf());

        let mut b = Binding::default();
        b.values
            .insert("case".to_string(), serde_yaml_ng::Value::from("a"));
        repo.stage(&b, &empty_conversation()).expect("stage");
        repo.discard();

        assert!(!tmp.path().join("runs").exists());
    }

    #[test]
    fn fs_run_repository_flush_returns_run_dir_path_shape() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut repo = FsRunRepository::new(tmp.path().to_path_buf());
        let run_dir = repo.flush("claim-handler").expect("flush");

        assert!(run_dir.starts_with(tmp.path().join("runs")));
        let basename = run_dir.file_name().and_then(|s| s.to_str()).expect("name");
        assert!(basename.ends_with("-claim-handler"), "got {basename}");
        // RFC3339-ish timestamp with `:` replaced by `-`, trailing `Z` before
        // the assembly suffix.
        assert!(basename.contains('T'));
        assert!(basename.contains("Z-"));
    }

    #[test]
    fn filename_for_empty_binding_is_default_yaml() {
        let binding = Binding::default();
        assert_eq!(filename_for(&binding), "default.yaml");
    }

    #[test]
    fn filename_for_single_axis_strips_yaml_quoting() {
        let mut binding = Binding::default();
        binding.values.insert(
            "case".to_string(),
            serde_yaml_ng::Value::from("missing-fields"),
        );
        assert_eq!(filename_for(&binding), "missing-fields.yaml");
    }

    #[test]
    fn filename_for_two_axes_joins_with_dash_in_axis_order() {
        let mut binding = Binding::default();
        binding
            .values
            .insert("alpha".to_string(), serde_yaml_ng::Value::from("a1"));
        binding
            .values
            .insert("beta".to_string(), serde_yaml_ng::Value::from("b1"));
        assert_eq!(filename_for(&binding), "a1-b1.yaml");
    }
}
