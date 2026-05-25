//! Repository ports for `cli/assemble` and their `std::fs` adapters.
//!
//! See `docs/developer/2026-05-23-A-cli-assemble/plan.md` Step 2. The
//! `content/project.rs layout` slice replaces these adapters with a shared
//! `Project` value without changing the trait shapes.

use std::io;
use std::path::PathBuf;

use crate::content::assembly::Assembly;
use crate::content::assembly::AssemblyError;
use crate::content::assembly::Binding;
use crate::content::conversation::Conversation;
use crate::content::conversation::ConversationError;
use crate::content::evaluation::Evaluation;
use crate::content::evaluation::EvaluationError;

/// Lists, loads, and saves single conversation files. Independent of the
/// per-binding [`RunRepository`], which buffers a many-file write; `ailly run`
/// operates one file at a time, in place, so it has no `stage`/`flush`
/// ceremony.
pub trait ConversationRepository {
    /// Resolve `target` to one or more conversation file paths.
    ///
    /// - target is a file → `[target.clone()]`.
    /// - target is a directory → entries with extension `yaml`, sorted
    ///   ascending.
    /// - Neither → [`RepositoryError::TargetNotFound`].
    ///
    /// # Errors
    /// Returns [`RepositoryError::Vfs`] if directory iteration fails or
    /// [`RepositoryError::TargetNotFound`] when neither branch matches.
    fn list(&self, target: &vfs::VfsPath) -> Result<Vec<vfs::VfsPath>, RepositoryError>;

    /// Read and parse one conversation file.
    ///
    /// # Errors
    /// [`RepositoryError::Vfs`] for I/O failures;
    /// [`RepositoryError::ParseConversation`] when the YAML does not parse
    /// as a [`Conversation`].
    fn load(&self, path: &vfs::VfsPath) -> Result<Conversation, RepositoryError>;

    /// Serialize `conv` and write atomically to `path` via temp-then-rename.
    /// Best-effort cleanup of `<path>.tmp` on serialization or rename failure.
    /// The same-file collision case (two `ailly run` instances against one
    /// target) is undefined behaviour and not protected against.
    ///
    /// # Errors
    /// [`RepositoryError::Emit`] when serialization fails;
    /// [`RepositoryError::Write`] or [`RepositoryError::Vfs`] when the
    /// write or rename fails.
    fn save(&self, path: &vfs::VfsPath, conv: &Conversation) -> Result<(), RepositoryError>;
}

/// `vfs`-backed [`ConversationRepository`]. Unit-style: holds no state,
/// resolves every path argument as-given without rerooting through a project
/// directory, because `ailly run` resolves `target` against the current
/// working directory (the CLI handler joins it onto a host-rooted VFS).
pub struct VfsConversationRepository;

impl ConversationRepository for VfsConversationRepository {
    fn list(&self, target: &vfs::VfsPath) -> Result<Vec<vfs::VfsPath>, RepositoryError> {
        let is_file = target.is_file().map_err(|source| RepositoryError::Vfs {
            path: target.as_str().to_string(),
            source,
        })?;
        if is_file {
            return Ok(vec![target.clone()]);
        }
        let is_dir = target.is_dir().map_err(|source| RepositoryError::Vfs {
            path: target.as_str().to_string(),
            source,
        })?;
        if is_dir {
            let entries = target.read_dir().map_err(|source| RepositoryError::Vfs {
                path: target.as_str().to_string(),
                source,
            })?;
            let mut paths: Vec<vfs::VfsPath> = entries
                .filter(|entry| entry.extension().is_some_and(|ext| ext == "yaml"))
                .collect();
            paths.sort_by_key(vfs::VfsPath::filename);
            return Ok(paths);
        }
        Err(RepositoryError::TargetNotFound {
            path: PathBuf::from(target.as_str()),
        })
    }

    fn load(&self, path: &vfs::VfsPath) -> Result<Conversation, RepositoryError> {
        let body = path
            .read_to_string()
            .map_err(|source| RepositoryError::Vfs {
                path: path.as_str().to_string(),
                source,
            })?;
        Conversation::from_yaml_str(&body).map_err(|source| RepositoryError::ParseConversation {
            path: PathBuf::from(path.as_str()),
            source,
        })
    }

    fn save(&self, path: &vfs::VfsPath, conv: &Conversation) -> Result<(), RepositoryError> {
        let body = conv
            .to_yaml_string()
            .map_err(|source| RepositoryError::Emit { source })?;
        let tmp_path = vfs_tmp_path(path).map_err(|source| RepositoryError::Vfs {
            path: format!("{}.tmp", path.as_str()),
            source,
        })?;
        let write_result = (|| -> Result<(), RepositoryError> {
            let mut file = tmp_path
                .create_file()
                .map_err(|source| RepositoryError::Vfs {
                    path: tmp_path.as_str().to_string(),
                    source,
                })?;
            std::io::Write::write_all(&mut file, body.as_bytes()).map_err(|source| {
                RepositoryError::Write {
                    path: PathBuf::from(tmp_path.as_str()),
                    source,
                }
            })
        })();
        if let Err(err) = write_result {
            let _ = tmp_path.remove_file();
            return Err(err);
        }
        // `vfs::VfsPath::move_file` refuses to overwrite an existing
        // destination (unlike `std::fs::rename` on Unix, which is atomic
        // overwrite). For the `ailly run` update path the destination
        // usually exists, so remove it first. This narrows the atomic
        // window (a crash between remove and move leaves no file) but
        // matches the "best effort" guarantee the trait already
        // documents, and preserves the partial-write protection
        // temp-then-rename was actually for.
        if path.exists().unwrap_or(false) {
            let _ = path.remove_file();
        }
        if let Err(source) = tmp_path.move_file(path) {
            let _ = tmp_path.remove_file();
            return Err(RepositoryError::Vfs {
                path: path.as_str().to_string(),
                source,
            });
        }
        Ok(())
    }
}

/// Compute the `<path>.tmp` companion for atomic temp-then-rename `save`.
fn vfs_tmp_path(path: &vfs::VfsPath) -> Result<vfs::VfsPath, vfs::VfsError> {
    let parent = path.parent();
    let filename = format!("{}.tmp", path.filename());
    parent.join(filename)
}

/// Loads parsed [`Evaluation`] suites by name. Mirrors [`AssemblyRepository`];
/// resolves `<project>/evals/<name>.yaml`.
pub trait EvaluationRepository {
    /// Read `<project>/evals/<name>.yaml` and parse it.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Read`] if the file is missing or unreadable
    /// and [`RepositoryError::ParseEvaluation`] if the file is not a valid
    /// suite.
    fn get(&self, name: &str) -> Result<Evaluation, RepositoryError>;
}

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
    #[error("parsing conversation {path:?}: {source}")]
    ParseConversation {
        path: PathBuf,
        #[source]
        source: ConversationError,
    },
    #[error("parsing evaluation {path:?}: {source}")]
    ParseEvaluation {
        path: PathBuf,
        #[source]
        source: EvaluationError,
    },
    #[error("target {path:?} is neither a file nor a directory")]
    TargetNotFound { path: PathBuf },
    #[error("pattern {pattern:?}: {message}")]
    Pattern { pattern: String, message: String },
    #[error("vfs error at {path:?}: {source}")]
    Vfs {
        path: String,
        #[source]
        source: vfs::VfsError,
    },
    #[error("run already committed")]
    AlreadyCommitted,
}

/// `vfs`-backed [`AssemblyRepository`] rooted at a project directory.
pub struct VfsAssemblyRepository {
    root: vfs::VfsPath,
}

impl VfsAssemblyRepository {
    #[must_use]
    pub fn new(root: vfs::VfsPath) -> Self {
        Self { root }
    }

    fn path_for(&self, name: &str) -> Result<vfs::VfsPath, vfs::VfsError> {
        self.root.join("assemblies")?.join(format!("{name}.yaml"))
    }
}

impl AssemblyRepository for VfsAssemblyRepository {
    fn get(&self, name: &str) -> Result<Assembly, RepositoryError> {
        let path = self.path_for(name).map_err(|source| RepositoryError::Vfs {
            path: format!("assemblies/{name}.yaml"),
            source,
        })?;
        let body = path
            .read_to_string()
            .map_err(|source| RepositoryError::Vfs {
                path: path.as_str().to_string(),
                source,
            })?;
        Assembly::from_yaml_str(&body).map_err(|source| RepositoryError::Parse {
            path: PathBuf::from(path.as_str()),
            source,
        })
    }
}

/// `vfs`-backed [`EvaluationRepository`] rooted at a project directory.
pub struct VfsEvaluationRepository {
    root: vfs::VfsPath,
}

impl VfsEvaluationRepository {
    #[must_use]
    pub fn new(root: vfs::VfsPath) -> Self {
        Self { root }
    }

    fn path_for(&self, name: &str) -> Result<vfs::VfsPath, vfs::VfsError> {
        self.root.join("evals")?.join(format!("{name}.yaml"))
    }
}

impl EvaluationRepository for VfsEvaluationRepository {
    fn get(&self, name: &str) -> Result<Evaluation, RepositoryError> {
        let path = self.path_for(name).map_err(|source| RepositoryError::Vfs {
            path: format!("evals/{name}.yaml"),
            source,
        })?;
        let body = path
            .read_to_string()
            .map_err(|source| RepositoryError::Vfs {
                path: path.as_str().to_string(),
                source,
            })?;
        Evaluation::from_yaml_str(&body).map_err(|source| RepositoryError::ParseEvaluation {
            path: PathBuf::from(path.as_str()),
            source,
        })
    }
}

/// `vfs`-backed [`ContextRepository`] rooted at a project directory.
pub struct VfsContextRepository {
    root: vfs::VfsPath,
}

impl VfsContextRepository {
    #[must_use]
    pub fn new(root: vfs::VfsPath) -> Self {
        Self { root }
    }

    fn resolve(&self, path: &str) -> Result<vfs::VfsPath, RepositoryError> {
        self.root.join(path).map_err(|source| RepositoryError::Vfs {
            path: path.to_string(),
            source,
        })
    }
}

impl ContextRepository for VfsContextRepository {
    fn read_file(&self, path: &str) -> Result<String, RepositoryError> {
        let resolved = self.resolve(path)?;
        resolved
            .read_to_string()
            .map_err(|source| RepositoryError::Vfs {
                path: resolved.as_str().to_string(),
                source,
            })
    }

    fn glob_concat(
        &self,
        pattern: &str,
        limit: Option<usize>,
    ) -> Result<GlobResult, RepositoryError> {
        if pattern.contains("**") {
            return Err(RepositoryError::Pattern {
                pattern: pattern.to_string(),
                message: String::from("recursive patterns are not supported"),
            });
        }
        let (parent_segment, last_segment) = match pattern.rsplit_once('/') {
            Some((parent, last)) => (parent, last),
            None => ("", pattern),
        };
        // Patterns we support today are single-segment globs in the last
        // path component: a literal directory prefix (possibly empty) plus
        // a filename with at most one `*`. The earlier segments must not
        // contain `*` themselves.
        if parent_segment.contains('*') {
            return Err(RepositoryError::Pattern {
                pattern: pattern.to_string(),
                message: String::from("wildcards in non-trailing segments are not supported"),
            });
        }
        let parent_vfs = if parent_segment.is_empty() {
            self.root.clone()
        } else {
            self.resolve(parent_segment)?
        };
        let entries = parent_vfs
            .read_dir()
            .map_err(|source| RepositoryError::Vfs {
                path: parent_vfs.as_str().to_string(),
                source,
            })?;
        let mut matched: Vec<vfs::VfsPath> = entries
            .filter(|entry| matches_glob(&entry.filename(), last_segment))
            .collect();
        matched.sort_by_key(vfs::VfsPath::filename);
        if let Some(n) = limit
            && matched.len() > n
        {
            matched.truncate(n);
        }

        let mut bodies: Vec<String> = Vec::with_capacity(matched.len());
        let mut paths: Vec<PathBuf> = Vec::with_capacity(matched.len());
        for entry in &matched {
            let body = entry
                .read_to_string()
                .map_err(|source| RepositoryError::Vfs {
                    path: entry.as_str().to_string(),
                    source,
                })?;
            bodies.push(body);
            paths.push(PathBuf::from(entry.as_str()));
        }
        let body = bodies.join("\n");
        Ok(GlobResult { paths, body })
    }
}

/// Match a single filename against a glob with at most one `*` wildcard.
/// Supported shapes: `*.md`, `prefix-*.json`, `name`, `*name`, `name*`.
/// Returns false for any pattern containing more than one `*`.
fn matches_glob(name: &str, pattern: &str) -> bool {
    let mut parts = pattern.split('*');
    let prefix = parts.next().unwrap_or("");
    let suffix = parts.next();
    if parts.next().is_some() {
        // More than one `*` — unsupported. Reject by failing every match;
        // surfaces as an empty glob expansion rather than an error to keep
        // the existing call shapes ergonomic.
        return false;
    }
    match suffix {
        None => name == prefix,
        Some(suffix) => {
            name.starts_with(prefix)
                && name.ends_with(suffix)
                && name.len() >= prefix.len() + suffix.len()
        }
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

pub(crate) fn run_id(assembly_name: &str) -> String {
    let now = chrono::Utc::now();
    let ts = now.format("%Y-%m-%dT%H-%M-%SZ").to_string();
    // v7 prefixes 48 bits of millisecond timestamp; slicing the *leading*
    // chars of `simple()` would be deterministic within one millisecond
    // (a plan-Step-1 deviation, see commit message). Slice from offset 17
    // instead — past the timestamp (chars 0..12), version nibble (12),
    // rand_a (13..16), and variant nibble (16) — landing in rand_b, which
    // is pure randomness. That gives a six-hex-char disambiguator with
    // 24 bits of entropy per call.
    let uuid_hex = uuid::Uuid::now_v7().simple().to_string();
    let uuid6: String = uuid_hex.chars().skip(17).take(6).collect();
    format!("{ts}-{uuid6}-{assembly_name}")
}

#[cfg(test)]
mod tests {
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

    fn write_assembly_fixture_vfs(project: &crate::content::project::Project) {
        use std::io::Write;
        let assemblies = project.root().join("assemblies").expect("join assemblies");
        assemblies.create_dir_all().expect("mkdir assemblies");
        let mut f = assemblies
            .join("claim-handler.yaml")
            .expect("join name")
            .create_file()
            .expect("create file");
        f.write_all(CLAIM_HANDLER_FIXTURE.as_bytes())
            .expect("write");
    }

    fn write_eval_fixture_vfs(project: &crate::content::project::Project, name: &str, body: &str) {
        use std::io::Write;
        let evals = project.root().join("evals").expect("join evals");
        evals.create_dir_all().expect("mkdir evals");
        let mut f = evals
            .join(format!("{name}.yaml"))
            .expect("join name")
            .create_file()
            .expect("create file");
        f.write_all(body.as_bytes()).expect("write");
    }

    #[test]
    fn vfs_assembly_repository_reads_and_parses() {
        let project = crate::content::project::Project::open_memory();
        write_assembly_fixture_vfs(&project);

        let assembly = project.assemblies().get("claim-handler").expect("loads");
        assert_eq!(assembly.name, "claim-handler");
    }

    const REGRESSION_SUITE_FIXTURE: &str = "\
name: regression
cases:
  - name: missing-fields
    assertions:
      - { type: text_contains, value: \"policy number\" }
";

    #[test]
    fn vfs_evaluation_repository_reads_and_parses() {
        let project = crate::content::project::Project::open_memory();
        write_eval_fixture_vfs(&project, "regression", REGRESSION_SUITE_FIXTURE);

        let suite = project.evals().get("regression").expect("loads");
        assert_eq!(suite.name, "regression");
        assert_eq!(suite.cases.len(), 1);
    }

    #[test]
    fn vfs_evaluation_repository_missing_returns_vfs_error() {
        let project = crate::content::project::Project::open_memory();
        let err = project.evals().get("nope").expect_err("missing");
        assert!(matches!(err, RepositoryError::Vfs { .. }), "got {err:?}");
    }

    #[test]
    fn vfs_evaluation_repository_malformed_returns_parse_evaluation_error() {
        let project = crate::content::project::Project::open_memory();
        write_eval_fixture_vfs(&project, "broken", "::: not yaml :::");

        let err = project.evals().get("broken").expect_err("bad yaml");
        match err {
            RepositoryError::ParseEvaluation { path, .. } => {
                // vfs paths use '/' uniformly; PathBuf round-trips that
                // representation on both Unix and Windows because
                // `vfs::VfsPath::as_str` is platform-neutral.
                assert_eq!(path, PathBuf::from("/evals/broken.yaml"));
            }
            other => panic!("expected ParseEvaluation, got {other:?}"),
        }
    }

    #[test]
    fn vfs_assembly_repository_missing_returns_vfs_error() {
        let project = crate::content::project::Project::open_memory();
        let err = project.assemblies().get("nope").expect_err("missing");
        assert!(matches!(err, RepositoryError::Vfs { .. }), "got {err:?}");
    }

    fn write_vfs_file(root: &vfs::VfsPath, rel: &str, body: &str) {
        use std::io::Write;
        let path = root.join(rel).expect("join rel");
        if let Some((parent, _)) = rel.rsplit_once('/') {
            root.join(parent)
                .expect("join parent")
                .create_dir_all()
                .expect("mkdir parent");
        }
        let mut f = path.create_file().expect("create file");
        f.write_all(body.as_bytes()).expect("write");
    }

    #[test]
    fn vfs_context_repository_reads_a_file_byte_for_byte() {
        let project = crate::content::project::Project::open_memory();
        let body = "hello\nworld\n";
        write_vfs_file(project.root(), "a.md", body);

        let got = project.context().read_file("a.md").expect("read");
        assert_eq!(got, body);
    }

    #[test]
    fn vfs_context_repository_glob_concat_sorts_and_joins_with_newline() {
        let project = crate::content::project::Project::open_memory();
        write_vfs_file(project.root(), "ctx/b.md", "second");
        write_vfs_file(project.root(), "ctx/a.md", "first");

        let result = project
            .context()
            .glob_concat("ctx/*.md", None)
            .expect("glob");
        assert_eq!(result.paths.len(), 2);
        assert!(result.paths[0].ends_with("a.md"));
        assert!(result.paths[1].ends_with("b.md"));
        assert_eq!(result.body, "first\nsecond");
    }

    #[test]
    fn vfs_context_repository_glob_concat_truncates_after_sort() {
        let project = crate::content::project::Project::open_memory();
        // Write in a different order than the desired post-sort order to
        // demonstrate that truncation happens after the sort, not before.
        write_vfs_file(project.root(), "ctx/c.md", "third");
        write_vfs_file(project.root(), "ctx/a.md", "first");
        write_vfs_file(project.root(), "ctx/b.md", "second");

        let result = project
            .context()
            .glob_concat("ctx/*.md", Some(2))
            .expect("glob");
        assert_eq!(result.paths.len(), 2);
        assert!(result.paths[0].ends_with("a.md"));
        assert!(result.paths[1].ends_with("b.md"));
        assert_eq!(result.body, "first\nsecond");
    }

    #[test]
    fn vfs_context_repository_rejects_recursive_glob() {
        let project = crate::content::project::Project::open_memory();
        let err = project
            .context()
            .glob_concat("ctx/**/*.md", None)
            .expect_err("recursive glob");
        assert!(
            matches!(err, RepositoryError::Pattern { .. }),
            "got {err:?}"
        );
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

    fn blank_assistant_conversation() -> Conversation {
        Conversation {
            meta: Meta {
                model: ModelId::from("noop"),
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

    fn seed_conversation(root: &vfs::VfsPath, rel: &str) -> vfs::VfsPath {
        use std::io::Write;
        let body = blank_assistant_conversation()
            .to_yaml_string()
            .expect("emit");
        let path = root.join(rel).expect("join");
        let mut f = path.create_file().expect("create file");
        f.write_all(body.as_bytes()).expect("write");
        path
    }

    #[test]
    fn vfs_conversation_repository_list_returns_single_file_when_target_is_a_file() {
        let project = crate::content::project::Project::open_memory();
        let path = seed_conversation(project.root(), "one.yaml");

        let repo = VfsConversationRepository;
        let paths = repo.list(&path).expect("list single file");
        assert_eq!(paths, vec![path]);
    }

    #[test]
    fn vfs_conversation_repository_list_returns_yaml_entries_in_sorted_order() {
        let project = crate::content::project::Project::open_memory();
        for name in ["c.yaml", "a.yaml", "b.yaml"] {
            seed_conversation(project.root(), name);
        }

        let repo = VfsConversationRepository;
        let paths = repo.list(project.root()).expect("list dir");
        let names: Vec<String> = paths.iter().map(vfs::VfsPath::filename).collect();
        assert_eq!(names, vec!["a.yaml", "b.yaml", "c.yaml"]);
    }

    #[test]
    fn vfs_conversation_repository_list_filters_non_yaml_entries() {
        use std::io::Write;
        let project = crate::content::project::Project::open_memory();
        seed_conversation(project.root(), "keep.yaml");
        let mut md = project
            .root()
            .join("skip.md")
            .unwrap()
            .create_file()
            .expect("create md");
        md.write_all(b"ignore").expect("write md");
        let mut json = project
            .root()
            .join("skip.json")
            .unwrap()
            .create_file()
            .expect("create json");
        json.write_all(b"{}").expect("write json");
        project
            .root()
            .join("subdir")
            .unwrap()
            .create_dir_all()
            .expect("mkdir subdir");

        let repo = VfsConversationRepository;
        let paths = repo.list(project.root()).expect("list dir");
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0].filename(), "keep.yaml");
    }

    #[test]
    fn vfs_conversation_repository_list_returns_target_not_found_when_neither() {
        let project = crate::content::project::Project::open_memory();
        let missing = project.root().join("nope.yaml").unwrap();

        let repo = VfsConversationRepository;
        let err = repo.list(&missing).expect_err("missing target");
        match err {
            RepositoryError::TargetNotFound { path } => {
                assert_eq!(path, PathBuf::from("/nope.yaml"));
            }
            other => panic!("expected TargetNotFound, got {other:?}"),
        }
    }

    #[test]
    fn vfs_conversation_repository_load_round_trips_save() {
        let project = crate::content::project::Project::open_memory();
        let path = project.root().join("conv.yaml").unwrap();
        let conv = blank_assistant_conversation();

        let repo = VfsConversationRepository;
        repo.save(&path, &conv).expect("save");
        let loaded = repo.load(&path).expect("load");
        assert_eq!(loaded.meta.model, conv.meta.model);
        assert_eq!(loaded.session.len(), conv.session.len());
        assert_eq!(loaded.session[0].role, Role::Assistant);
        assert!(loaded.session[0].body.is_none());
    }

    #[test]
    fn vfs_conversation_repository_save_is_atomic_temp_then_rename() {
        let project = crate::content::project::Project::open_memory();
        let path = project.root().join("conv.yaml").unwrap();
        let conv = blank_assistant_conversation();

        let repo = VfsConversationRepository;
        repo.save(&path, &conv).expect("save");

        assert!(
            path.exists().unwrap_or(false),
            "target file should exist after save"
        );
        let tmp_path = project.root().join("conv.yaml.tmp").unwrap();
        assert!(
            !tmp_path.exists().unwrap_or(true),
            "tmp companion file should be gone after successful rename",
        );
    }

    #[test]
    fn vfs_conversation_repository_load_returns_parse_conversation_on_bad_yaml() {
        use std::io::Write;
        let project = crate::content::project::Project::open_memory();
        let path = project.root().join("broken.yaml").unwrap();
        let mut f = path.create_file().expect("create file");
        f.write_all(b"not a conversation").expect("write");

        let repo = VfsConversationRepository;
        let err = repo.load(&path).expect_err("bad yaml");
        match err {
            RepositoryError::ParseConversation { path: got, .. } => {
                assert_eq!(got, PathBuf::from("/broken.yaml"));
            }
            other => panic!("expected ParseConversation, got {other:?}"),
        }
    }
}
