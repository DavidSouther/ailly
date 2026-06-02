//! `Project` aggregate over a `vfs`-backed root.
//!
//! The aggregate owns one [`vfs::VfsPath`] and vends typed sub-handles for the
//! four read-side folders (assemblies, evaluations, context, prompts) plus a
//! [`RunTx`] Unit of Work for the `runs/` subtree. Path validation goes
//! through [`Project::resolve`] or [`Project::child`], so untyped strings
//! cannot reach the I/O surface.

use std::path::PathBuf;

use crate::content::assembly::Assembly;
use crate::content::assembly::Binding;
use crate::content::assembly::RenderError;
use crate::content::conversation::Conversation;
use crate::content::evaluation::Evaluation;
use crate::content::repository::AssemblyRepository;
use crate::content::repository::ContextRepository;
use crate::content::repository::EvaluationRepository;
use crate::content::repository::GlobResult;
use crate::content::repository::RepositoryError;
use crate::content::repository::VfsAssemblyRepository;
use crate::content::repository::VfsContextRepository;
use crate::content::repository::VfsEvaluationRepository;

/// Aggregate root over a `vfs`-backed project root. Vends typed sub-handles
/// for the four read-side folders and a [`RunTx`] Unit of Work for the
/// `runs/` subtree. The [`vfs::VfsPath`] is the consistency boundary —
/// everything inside shares one root.
#[derive(Debug)]
pub struct Project {
    root: vfs::VfsPath,
    /// Physical filesystem root for error messages. `Some` only when opened
    /// via [`Project::open`]; `None` for in-memory and arbitrary-VFS roots.
    host_root: Option<PathBuf>,
}

/// A relative path that has been substituted against a [`Binding`] and
/// validated against a [`Project`] root. The newtype is the parse boundary:
/// no `From<&str>`, no `AsRef<Path>`. Holds both the relative form (for
/// filename derivation and `meta.binding` round-trips) and the absolute
/// [`vfs::VfsPath`] for I/O.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectPath {
    relative: String,
    absolute: vfs::VfsPath,
}

impl std::hash::Hash for ProjectPath {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Hashing on the relative path alone is consistent with the
        // documented identity (project-relative form) and avoids the lack
        // of `Hash` on `vfs::VfsPath`. Two `ProjectPath`s with the same
        // relative form from different projects still compare unequal via
        // `PartialEq` (their `absolute` differs); a hash collision in
        // that case is acceptable.
        self.relative.hash(state);
    }
}

/// Borrow over a [`Project`] plus the `assemblies/` subfolder. Implements
/// [`AssemblyRepository`] by delegating to [`VfsAssemblyRepository`] over
/// the project root.
pub struct Assemblies<'a>(pub(crate) &'a Project);

impl AssemblyRepository for Assemblies<'_> {
    fn get(&self, name: &str) -> Result<Assembly, RepositoryError> {
        VfsAssemblyRepository::new(self.0.root.clone()).get(name)
    }
}

/// Borrow over a [`Project`] plus the `evals/` subfolder. Implements
/// [`EvaluationRepository`] by delegating to [`VfsEvaluationRepository`]
/// over the project root.
pub struct Evaluations<'a>(pub(crate) &'a Project);

impl EvaluationRepository for Evaluations<'_> {
    fn get(&self, name: &str) -> Result<Evaluation, RepositoryError> {
        VfsEvaluationRepository::new(self.0.root.clone()).get(name)
    }
}

/// Borrow over a [`Project`] plus the `context/` subfolder. Accepts
/// [`ProjectPath`] arguments — all context reads must first pass through
/// [`Project::resolve`] or [`Project::child`], matching [`Prompts::read`].
pub struct Context<'a>(pub(crate) &'a Project);

impl Context<'_> {
    /// Read `path` as UTF-8 text.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Vfs`] when the underlying VFS read fails.
    pub fn read_file(&self, path: &ProjectPath) -> Result<String, RepositoryError> {
        self.repo().read_file(path.relative())
    }

    /// Expand the glob in `path` and concatenate matched files in
    /// filename-ascending order, separated by a single newline. When `limit`
    /// is `Some(n)`, only the first `n` paths after sorting are included.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Pattern`] if the glob is unsupported and
    /// [`RepositoryError::Vfs`] if a matched file is unreadable.
    pub fn glob_concat(
        &self,
        path: &ProjectPath,
        limit: Option<usize>,
    ) -> Result<GlobResult, RepositoryError> {
        self.repo().glob_concat(path.relative(), limit)
    }

    fn repo(&self) -> VfsContextRepository {
        match self.0.host_root() {
            Some(hr) => VfsContextRepository::with_host_root(self.0.root.clone(), hr.to_path_buf()),
            None => VfsContextRepository::new(self.0.root.clone()),
        }
    }
}

/// Borrow over a [`Project`] plus the `prompts/` subfolder. Exposes a
/// single inherent [`Prompts::read`] method scoped to typed
/// [`ProjectPath`] arguments. Does not host `glob_concat` — only
/// [`Context`] does globbing today. The `&Project` is held for future
/// prompt-specific divergence (caching, content addressing); today the
/// `ProjectPath` carries the absolute `VfsPath` directly.
pub struct Prompts<'a>(
    #[expect(
        dead_code,
        reason = "held for future prompt-specific divergence per design"
    )]
    pub(crate) &'a Project,
);

impl Prompts<'_> {
    /// Read the file pointed at by `path` as UTF-8 text.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Vfs`] when the underlying VFS read
    /// fails (missing file, permission denied, malformed UTF-8).
    pub fn read(&self, path: &ProjectPath) -> Result<String, RepositoryError> {
        path.as_vfs()
            .read_to_string()
            .map_err(|source| RepositoryError::Vfs {
                path: path.as_vfs().as_str().to_string(),
                source,
            })
    }
}

/// Unit of Work over the `runs/` subtree. `stage` buffers in memory;
/// `commit` flushes into a fresh `runs/<id>-<assembly>/` directory and
/// transitions one-shot to a committed state; `discard` is idempotent and
/// composes with `Drop`.
pub struct RunTx<'a> {
    project: &'a Project,
    staged: Vec<StagedFile>,
    committed: bool,
}

/// One conversation file buffered by [`RunTx::stage`] before [`RunTx::commit`]
/// flushes the lot to disk.
pub(crate) struct StagedFile {
    pub(crate) filename: String,
    pub(crate) body: String,
}

/// [`Project::open`] failure modes. Distinct from [`RepositoryError`] because
/// they describe project construction, not repository I/O.
#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("opening project root {path:?}: {source}")]
    Open {
        path: PathBuf,
        #[source]
        source: vfs::VfsError,
    },
    #[error("project root {path:?} is not a directory")]
    NotADirectory { path: PathBuf },
}

/// [`Project::child`] and [`ProjectPath`] construction failure modes.
/// Literal-segment rejection; no canonicalization. `a/../b` is rejected
/// even though it does not escape.
#[derive(Debug, thiserror::Error)]
pub enum PathError {
    #[error("path {path:?} is absolute; project paths must be relative")]
    AbsolutePath { path: String },
    #[error(
        "path {path:?} contains a `..` segment; project paths must not contain parent traversals"
    )]
    ParentEscape { path: String },
}

impl Project {
    /// Open a project rooted at a real on-disk path. Constructs a
    /// `vfs::PhysicalFS` mounted at `root.canonicalize()?` and verifies the
    /// path is an existing directory. Does not validate subfolder shape;
    /// missing `assemblies/` is discovered on first `assemblies().get(...)`.
    ///
    /// # Errors
    ///
    /// Returns [`ProjectError::Open`] if the path cannot be canonicalized or
    /// the underlying VFS mount fails, and [`ProjectError::NotADirectory`]
    /// when the path resolves to something that is not a directory.
    pub fn open(root: impl AsRef<std::path::Path>) -> Result<Self, ProjectError> {
        let root_path = root.as_ref();
        let canonical = root_path
            .canonicalize()
            .map_err(|source| ProjectError::Open {
                path: root_path.to_path_buf(),
                source: vfs::VfsError::from(source),
            })?;
        if !canonical.is_dir() {
            return Err(ProjectError::NotADirectory { path: canonical });
        }
        let fs = vfs::PhysicalFS::new(canonical.clone());
        Ok(Self {
            root: vfs::VfsPath::new(fs),
            host_root: Some(canonical),
        })
    }

    /// Open a project rooted at a fresh `vfs::MemoryFS`. Test seam. Every
    /// adapter test in `src/content/` switches to this after Step 4.
    #[must_use]
    pub fn open_memory() -> Self {
        Self {
            root: vfs::VfsPath::new(vfs::MemoryFS::new()),
            host_root: None,
        }
    }

    /// Open a project rooted at an arbitrary [`vfs::VfsPath`]. Composition-
    /// root seam for the CLI handlers in Step 6 that need to mount a host-
    /// rooted `PhysicalFS::new("/")` for absolute `--over` targets.
    #[must_use]
    pub fn from_root(root: vfs::VfsPath) -> Self {
        Self {
            root,
            host_root: None,
        }
    }

    /// Borrow the project root. Diagnostic use only; callers cannot
    /// construct a [`ProjectPath`] from this without going through
    /// [`Project::resolve`] or [`Project::child`].
    #[must_use]
    pub fn root(&self) -> &vfs::VfsPath {
        &self.root
    }

    /// Physical filesystem root, set only for projects opened via
    /// [`Project::open`]. Used to produce globally-rooted paths in error
    /// messages so they are clickable in editors and terminals.
    #[must_use]
    pub fn host_root(&self) -> Option<&std::path::Path> {
        self.host_root.as_deref()
    }

    /// Substitute `{{ var }}` placeholders against `binding` and produce a
    /// project-rooted [`ProjectPath`]. Single sanctioned constructor for
    /// [`ProjectPath`] from untrusted text. Whitespace inside the braces
    /// is tolerated; no conditionals, escaping, or nesting.
    ///
    /// # Errors
    ///
    /// Returns [`RenderError::UnknownVar`] when `binding` does not bind a
    /// referenced name and [`RenderError::UnterminatedPlaceholder`] when
    /// an open `{{` has no matching `}}`. Path validation against the
    /// project root is deferred to I/O time: any `..` segment or
    /// absolute path in the substituted form surfaces through
    /// [`vfs::VfsPath::join`] as a [`RepositoryError::Vfs`].
    pub fn resolve(&self, template: &str, binding: &Binding) -> Result<ProjectPath, RenderError> {
        let relative = substitute_template(template, binding)?;
        let absolute = self.root.join(&relative).map_err(|source| {
            RenderError::Repository(RepositoryError::Vfs {
                path: relative.clone(),
                source,
            })
        })?;
        Ok(ProjectPath { relative, absolute })
    }

    /// Construct a [`ProjectPath`] from a literal (non-templated) relative
    /// path. Rejects absolute paths and any `..` segment by literal match.
    ///
    /// # Errors
    ///
    /// Returns [`PathError::AbsolutePath`] when `relative` starts with `/`,
    /// and [`PathError::ParentEscape`] when any segment is exactly `..`.
    pub fn child(&self, relative: &str) -> Result<ProjectPath, PathError> {
        validate_relative(relative)?;
        // `vfs::VfsPath::join` operates on `&str` and produces a `VfsResult`.
        // After `validate_relative` rejects absolute and `..` paths, the
        // remaining failure modes (illegal segments inside the VFS) are
        // backend-specific; surface them through the same `ParentEscape`
        // variant rather than introducing a third variant for what is
        // effectively the same family of "path is outside the project".
        let absolute = self
            .root
            .join(relative)
            .map_err(|_| PathError::ParentEscape {
                path: relative.to_string(),
            })?;
        Ok(ProjectPath {
            relative: relative.to_string(),
            absolute,
        })
    }

    /// Borrow an [`Assemblies`] handle.
    #[must_use]
    pub fn assemblies(&self) -> Assemblies<'_> {
        Assemblies(self)
    }

    /// Borrow an [`Evaluations`] handle.
    #[must_use]
    pub fn evals(&self) -> Evaluations<'_> {
        Evaluations(self)
    }

    /// Borrow a [`Context`] handle.
    #[must_use]
    pub fn context(&self) -> Context<'_> {
        Context(self)
    }

    /// Borrow a [`Prompts`] handle.
    #[must_use]
    pub fn prompts(&self) -> Prompts<'_> {
        Prompts(self)
    }

    /// Resolve a host `path` to a [`vfs::VfsPath`]. Absolute paths mount onto
    /// a host-rooted `vfs::PhysicalFS::new("/")`; relative paths join onto
    /// `self.root()`. Returns `Err` when `path` contains non-UTF-8 bytes or
    /// the VFS join fails.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::Vfs`] for non-UTF-8 input or VFS join
    /// failures.
    pub fn resolve_host_path(
        &self,
        path: &std::path::Path,
    ) -> Result<vfs::VfsPath, RepositoryError> {
        let path_str = path.to_str().ok_or_else(|| RepositoryError::Vfs {
            path: path.to_string_lossy().into_owned(),
            source: vfs::VfsError::from(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "path is not valid UTF-8",
            )),
        })?;
        if path.is_absolute() {
            vfs::VfsPath::new(vfs::PhysicalFS::new("/"))
                .join(path_str.trim_start_matches('/'))
                .map_err(|source| RepositoryError::Vfs {
                    path: path_str.to_string(),
                    source,
                })
        } else {
            self.root
                .join(path_str)
                .map_err(|source| RepositoryError::Vfs {
                    path: path_str.to_string(),
                    source,
                })
        }
    }

    /// Open a [`RunTx`].
    #[must_use]
    pub fn begin_run(&self) -> RunTx<'_> {
        RunTx {
            project: self,
            staged: Vec::new(),
            committed: false,
        }
    }
}

impl ProjectPath {
    /// Project-relative form for filename derivation, display, and
    /// `meta.binding` round-trips.
    #[must_use]
    pub fn relative(&self) -> &str {
        &self.relative
    }

    /// Project-absolute [`vfs::VfsPath`] for I/O. Consumed by repository
    /// adapters starting in Step 2.
    #[must_use]
    pub fn as_vfs(&self) -> &vfs::VfsPath {
        &self.absolute
    }
}

impl RunTx<'_> {
    /// Stage one conversation under `binding`.
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::AlreadyCommitted`] if called after
    /// `commit`. Returns [`RepositoryError::Emit`] if YAML serialization
    /// fails.
    pub fn stage(&mut self, binding: &Binding, conv: &Conversation) -> Result<(), RepositoryError> {
        if self.committed {
            return Err(RepositoryError::AlreadyCommitted);
        }
        let filename = crate::content::repository::filename_for(binding);
        let body = conv
            .to_yaml_string()
            .map_err(|source| RepositoryError::Emit { source })?;
        self.staged.push(StagedFile { filename, body });
        Ok(())
    }

    /// Mint a fresh `runs/<id>-<assembly_name>/` directory under the
    /// project root, write every staged file, and transition the
    /// transaction to committed. One-shot — a second call returns
    /// [`RepositoryError::AlreadyCommitted`].
    ///
    /// # Errors
    ///
    /// Returns [`RepositoryError::AlreadyCommitted`] on a second call,
    /// [`RepositoryError::CreateDir`] or [`RepositoryError::Vfs`] when the
    /// underlying VFS operation fails, and [`RepositoryError::Write`] when
    /// a staged file cannot be written.
    pub fn commit(&mut self, assembly_name: &str) -> Result<vfs::VfsPath, RepositoryError> {
        if self.committed {
            return Err(RepositoryError::AlreadyCommitted);
        }
        let id = crate::content::repository::run_id(assembly_name);
        let run_dir = self
            .project
            .root
            .join("runs")
            .and_then(|p| p.join(&id))
            .map_err(|source| RepositoryError::Vfs {
                path: format!("runs/{id}"),
                source,
            })?;
        run_dir
            .create_dir_all()
            .map_err(|source| RepositoryError::Vfs {
                path: run_dir.as_str().to_string(),
                source,
            })?;
        for staged in self.staged.drain(..) {
            let file_path =
                run_dir
                    .join(&staged.filename)
                    .map_err(|source| RepositoryError::Vfs {
                        path: staged.filename.clone(),
                        source,
                    })?;
            let mut file = file_path
                .create_file()
                .map_err(|source| RepositoryError::Vfs {
                    path: file_path.as_str().to_string(),
                    source,
                })?;
            std::io::Write::write_all(&mut file, staged.body.as_bytes()).map_err(|source| {
                RepositoryError::Write {
                    path: PathBuf::from(file_path.as_str()),
                    source,
                }
            })?;
        }
        self.committed = true;
        Ok(run_dir)
    }

    /// Drop staged writes without touching disk. Idempotent; safe to call
    /// from `Drop` after a manual `discard`.
    pub fn discard(&mut self) {
        self.staged.clear();
        // committed is intentionally untouched: a discarded RunTx remains
        // un-committed (so a subsequent commit is still a fresh one-shot),
        // and a committed RunTx whose discard is called from Drop has
        // already flushed its staged buffer.
    }
}

impl Drop for RunTx<'_> {
    fn drop(&mut self) {
        // Drop is the cancellation path for an uncommitted RunTx; discard
        // is idempotent so calling it on a committed transaction is a
        // no-op against an already-empty buffer.
        self.discard();
    }
}

/// Replace every `{{ name }}` placeholder in `template` with the
/// YAML-stringified value bound to `name`. Whitespace inside the braces
/// is tolerated. No conditionals, escaping, or nesting.
fn substitute_template(template: &str, binding: &Binding) -> Result<String, RenderError> {
    let mut result = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        result.push_str(&rest[..start]);
        let after_open = &rest[start + 2..];
        let end = after_open
            .find("}}")
            .ok_or_else(|| RenderError::UnterminatedPlaceholder {
                in_path: template.to_string(),
            })?;
        let name = after_open[..end].trim();
        let value = binding
            .values
            .get(name)
            .ok_or_else(|| RenderError::UnknownVar {
                name: name.to_string(),
                in_path: template.to_string(),
            })?;
        result.push_str(&stringify_value(value));
        rest = &after_open[end + 2..];
    }
    result.push_str(rest);
    Ok(result)
}

fn stringify_value(value: &serde_yaml_ng::Value) -> String {
    match value {
        serde_yaml_ng::Value::String(s) => s.clone(),
        serde_yaml_ng::Value::Bool(b) => b.to_string(),
        serde_yaml_ng::Value::Number(n) => n.to_string(),
        serde_yaml_ng::Value::Null => String::new(),
        other => {
            let raw = serde_yaml_ng::to_string(other).unwrap_or_default();
            raw.trim().trim_matches('"').trim_matches('\'').to_string()
        }
    }
}

/// Reject absolute paths and any literal `..` segment. Literal-segment
/// rejection (not canonicalization): `a/../b` fails even though it does
/// not escape.
fn validate_relative(relative: &str) -> Result<(), PathError> {
    if relative.starts_with('/') {
        return Err(PathError::AbsolutePath {
            path: relative.to_string(),
        });
    }
    for segment in relative.split('/') {
        if segment == ".." {
            return Err(PathError::ParentEscape {
                path: relative.to_string(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::conversation::BindingMap;

    // Filesystem-backed Project::open tests live in
    // `tests/project_open.rs` so this module stays free of the temp-dir
    // crate the CI lint guard rejects. The integration tests cover the
    // canonicalize-and-validate behaviour Project::open requires a real
    // path for.

    #[test]
    fn child_accepts_a_valid_relative_path() {
        let project = Project::open_memory();
        let path = project.child("prompts/default.md").expect("valid child");
        assert_eq!(path.relative(), "prompts/default.md");
    }

    #[test]
    fn child_rejects_an_absolute_path() {
        let project = Project::open_memory();
        let err = project.child("/etc/passwd").expect_err("absolute");
        match err {
            PathError::AbsolutePath { path } => assert_eq!(path, "/etc/passwd"),
            PathError::ParentEscape { .. } => panic!("expected AbsolutePath, got ParentEscape"),
        }
    }

    #[test]
    fn child_rejects_a_parent_traversal_segment() {
        let project = Project::open_memory();
        let err = project.child("../escape").expect_err("parent escape");
        match err {
            PathError::ParentEscape { path } => assert_eq!(path, "../escape"),
            PathError::AbsolutePath { .. } => panic!("expected ParentEscape, got AbsolutePath"),
        }
    }

    #[test]
    fn child_rejects_a_middle_parent_traversal_segment() {
        // Literal-segment rejection: `a/../b` is rejected even though it
        // does not actually escape the project root.
        let project = Project::open_memory();
        let err = project.child("a/../b").expect_err("middle parent");
        assert!(matches!(err, PathError::ParentEscape { .. }), "got {err:?}");
    }

    #[test]
    fn resolve_substitutes_a_simple_placeholder() {
        let project = Project::open_memory();
        let mut binding = Binding::default();
        binding
            .values
            .insert(String::from("case"), serde_yaml_ng::Value::from("default"));
        let path = project
            .resolve("prompts/{{ case }}.md", &binding)
            .expect("resolve");
        assert_eq!(path.relative(), "prompts/default.md");
    }

    #[test]
    fn resolve_tolerates_whitespace_inside_braces() {
        let project = Project::open_memory();
        let mut binding = Binding::default();
        binding
            .values
            .insert(String::from("case"), serde_yaml_ng::Value::from("a"));
        let path = project.resolve("x/{{case}}.y", &binding).expect("resolve");
        assert_eq!(path.relative(), "x/a.y");
    }

    #[test]
    fn resolve_unterminated_placeholder_is_caught() {
        let project = Project::open_memory();
        let err = project
            .resolve("a/{{ case", &Binding::default())
            .expect_err("unterminated");
        assert!(
            matches!(err, RenderError::UnterminatedPlaceholder { .. }),
            "got {err:?}",
        );
    }

    #[test]
    fn resolve_propagates_unknown_var_errors() {
        let project = Project::open_memory();
        let binding = Binding {
            values: BindingMap::new(),
        };
        let err = project
            .resolve("prompts/{{ missing }}.md", &binding)
            .expect_err("missing var");
        match err {
            RenderError::UnknownVar { name, .. } => assert_eq!(name, "missing"),
            other => panic!("expected UnknownVar, got {other:?}"),
        }
    }

    #[test]
    fn project_path_round_trips_relative_and_absolute() {
        let project = Project::open_memory();
        let path = project.child("a/b/c").expect("child");
        assert_eq!(path.relative(), "a/b/c");
        assert_eq!(path.as_vfs().as_str(), "/a/b/c");
    }

    fn empty_conversation() -> crate::content::conversation::Conversation {
        use std::marker::PhantomData;

        use crate::content::conversation::Message;
        use crate::content::conversation::Meta;
        use crate::content::conversation::ModelId;
        use crate::content::conversation::Role;

        crate::content::conversation::Conversation {
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

    #[test]
    fn run_tx_stage_then_commit_writes_one_file_per_stage() {
        let project = Project::open_memory();
        let mut tx = project.begin_run();

        let mut b1 = Binding::default();
        b1.values
            .insert("case".to_string(), serde_yaml_ng::Value::from("alpha"));
        let mut b2 = Binding::default();
        b2.values
            .insert("case".to_string(), serde_yaml_ng::Value::from("beta"));

        let conv = empty_conversation();
        tx.stage(&b1, &conv).expect("stage 1");
        tx.stage(&b2, &conv).expect("stage 2");

        let run_dir = tx.commit("claim-handler").expect("commit");
        let entries: Vec<vfs::VfsPath> = run_dir.read_dir().expect("read run dir").collect();
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn run_tx_stage_then_drop_without_commit_writes_nothing() {
        let project = Project::open_memory();
        let runs_dir = project.root().join("runs").expect("join runs");
        {
            let mut tx = project.begin_run();
            let conv = empty_conversation();
            tx.stage(&Binding::default(), &conv).expect("stage");
            // Drop without committing; staged buffer is discarded.
        }
        assert!(!runs_dir.exists().unwrap_or(true));
    }

    #[test]
    fn run_tx_commit_then_stage_returns_already_committed() {
        let project = Project::open_memory();
        let mut tx = project.begin_run();
        let _ = tx.commit("claim-handler").expect("commit");
        let err = tx
            .stage(&Binding::default(), &empty_conversation())
            .expect_err("stage after commit");
        assert!(
            matches!(err, RepositoryError::AlreadyCommitted),
            "got {err:?}"
        );
    }

    #[test]
    fn run_tx_commit_then_commit_returns_already_committed() {
        let project = Project::open_memory();
        let mut tx = project.begin_run();
        let _ = tx.commit("claim-handler").expect("first commit");
        let err = tx.commit("claim-handler").expect_err("second commit");
        assert!(
            matches!(err, RepositoryError::AlreadyCommitted),
            "got {err:?}"
        );
    }
}
