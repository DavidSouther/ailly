//! Handler for `ailly assemble <name>`.
//!
//! See `docs/developer/2026-05-23-A-cli-assemble/plan.md` Step 3. The
//! handler is the load → operate → save → commit shape of a Unit of Work
//! over three repositories. Real prefix-block resolution and templated turn
//! rendering land in Step 4.

use std::marker::PhantomData;
use std::path::Path;
use std::path::PathBuf;

use crate::content::assembly::Assembly;
use crate::content::assembly::Binding;
use crate::content::assembly::PrefixBlock;
use crate::content::assembly::RenderError;
use crate::content::assembly::prefix_cache;
use crate::content::assembly::substitute;
use crate::content::conversation::Content;
use crate::content::conversation::Conversation;
use crate::content::conversation::Message;
use crate::content::conversation::Meta;
use crate::content::conversation::Rendered;
use crate::content::conversation::Role;
use crate::content::repository::AssemblyRepository;
use crate::content::repository::ContextRepository;
use crate::content::repository::RepositoryError;
use crate::content::repository::RunRepository;
use crate::content::repository::open_fs_repositories;

/// Arguments for the assemble handler. Public field list is fixed by the
/// feature test's struct-literal call site.
#[derive(Clone, Debug)]
pub struct AssembleArgs {
    pub project: PathBuf,
    pub name: String,
}

/// Errors emitted by the assemble handler. Library-boundary error; the CLI
/// entry point reformats via `Display` for stderr.
#[derive(Debug, thiserror::Error)]
pub enum AssembleError {
    #[error("repository error: {0}")]
    Repository(#[from] RepositoryError),
    #[error("rendering failed: {0}")]
    Render(#[from] RenderError),
}

/// Owns one of each repository, scoped to a project root. Rollback is the
/// default: if the `UoW` is dropped without [`AssembleUnitOfWork::commit`],
/// staged writes are discarded and no run directory is created.
pub struct AssembleUnitOfWork {
    pub assemblies: Box<dyn AssemblyRepository>,
    pub context: Box<dyn ContextRepository>,
    pub runs: Box<dyn RunRepository>,
    committed: bool,
}

impl AssembleUnitOfWork {
    /// Open a `UoW` backed by `std::fs` adapters rooted at the project path.
    #[must_use]
    pub fn open(project: &Path) -> Self {
        let (assemblies, context, runs) = open_fs_repositories(project);
        Self {
            assemblies,
            context,
            runs,
            committed: false,
        }
    }

    /// Flush staged writes and return the run directory path.
    ///
    /// # Errors
    ///
    /// Returns [`AssembleError::Repository`] when the underlying repository
    /// flush fails.
    pub fn commit(&mut self, assembly_name: &str) -> Result<PathBuf, AssembleError> {
        let run_dir = self.runs.flush(assembly_name)?;
        self.committed = true;
        Ok(run_dir)
    }
}

impl Drop for AssembleUnitOfWork {
    fn drop(&mut self) {
        if !self.committed {
            self.runs.discard();
        }
    }
}

/// Drive the assemble pipeline against `args.project`.
///
/// # Errors
///
/// Returns [`AssembleError::Repository`] when an underlying repository call
/// fails. Returns successfully with the run directory path otherwise.
#[expect(
    clippy::needless_pass_by_value,
    reason = "by-value AssembleArgs is the call shape required by the feature test"
)]
pub fn run(args: AssembleArgs) -> Result<PathBuf, AssembleError> {
    let mut uow = AssembleUnitOfWork::open(&args.project);
    let assembly = uow.assemblies.get(&args.name)?;
    for binding in assembly.expand_matrix() {
        let conversation = render_conversation(&assembly, &binding, uow.context.as_ref())?;
        uow.runs.stage(&binding, &conversation)?;
    }
    uow.commit(&assembly.name)
}

/// Render an [`Assembly`] against one [`Binding`] into a full
/// [`Conversation`]: prefix system messages followed by the rendered
/// template turns, ending in a blank assistant slot for `ailly run` to fill.
///
/// # Errors
///
/// Returns [`AssembleError::Repository`] when a prefix block read fails or
/// [`AssembleError::Render`] when a template references an unknown variable
/// or a templated path cannot be read.
fn render_conversation(
    assembly: &Assembly,
    binding: &Binding,
    ctx: &dyn ContextRepository,
) -> Result<Conversation, AssembleError> {
    let mut session: Vec<Message<Rendered>> = Vec::new();
    for block in &assembly.prefix {
        let body = resolve_prefix_block(block, binding, ctx)?;
        session.push(Message {
            role: Role::System,
            body: Some(Content::Text(body)),
            cache: prefix_cache(block),
            trace: None,
            _phase: PhantomData,
        });
    }
    for turn in &assembly.conversation {
        session.push(turn.render(binding, ctx)?);
    }
    Ok(Conversation {
        meta: Meta {
            model: assembly.model.clone(),
            debug: false,
            assembly: Some(assembly.name.clone()),
            binding: binding.values.clone(),
        },
        session,
    })
}

fn resolve_prefix_block(
    block: &PrefixBlock,
    binding: &Binding,
    ctx: &dyn ContextRepository,
) -> Result<String, AssembleError> {
    match block {
        PrefixBlock::File { path, .. } => {
            let resolved = substitute(path, binding)?;
            Ok(ctx.read_file(&resolved)?)
        }
        PrefixBlock::System { path, .. }
        | PrefixBlock::Tools { path, .. }
        | PrefixBlock::Examples { path, .. } => {
            let resolved = substitute(path, binding)?;
            Ok(ctx.glob_concat(&resolved, None)?.body)
        }
        PrefixBlock::Context {
            source,
            glob,
            count,
            ..
        } => {
            let pattern = match glob {
                Some(g) => format!("{source}/{g}"),
                None => source.clone(),
            };
            let resolved = substitute(&pattern, binding)?;
            Ok(ctx.glob_concat(&resolved, *count)?.body)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::content::conversation::Conversation;

    const SINGLE_AXIS_ASSEMBLY: &str = "\
name: claim-handler
model: claude-opus-4-7
matrix:
  case: [alpha, beta, gamma]
";

    const TWO_AXIS_ASSEMBLY: &str = "\
name: claim-handler
model: claude-opus-4-7
matrix:
  alpha: [a1, a2]
  beta: [b1, b2]
";

    const EMPTY_MATRIX_ASSEMBLY: &str = "\
name: claim-handler
model: claude-opus-4-7
";

    fn project_with_assembly(yaml: &str) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().expect("tempdir");
        let assemblies = tmp.path().join("assemblies");
        fs::create_dir_all(&assemblies).expect("mkdir assemblies");
        fs::write(assemblies.join("claim-handler.yaml"), yaml).expect("write assembly");
        tmp
    }

    fn yaml_files(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("read run dir")
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "yaml"))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn single_axis_writes_one_file_per_binding() {
        let tmp = project_with_assembly(SINGLE_AXIS_ASSEMBLY);
        let run_dir = run(AssembleArgs {
            project: tmp.path().to_path_buf(),
            name: String::from("claim-handler"),
        })
        .expect("run");
        let names = yaml_files(&run_dir);
        assert_eq!(names, vec!["alpha.yaml", "beta.yaml", "gamma.yaml"]);
    }

    #[test]
    fn two_axis_writes_one_file_per_cross_product_point() {
        let tmp = project_with_assembly(TWO_AXIS_ASSEMBLY);
        let run_dir = run(AssembleArgs {
            project: tmp.path().to_path_buf(),
            name: String::from("claim-handler"),
        })
        .expect("run");
        let names = yaml_files(&run_dir);
        assert_eq!(
            names,
            vec!["a1-b1.yaml", "a1-b2.yaml", "a2-b1.yaml", "a2-b2.yaml"]
        );
    }

    #[test]
    fn empty_matrix_writes_single_default_file() {
        let tmp = project_with_assembly(EMPTY_MATRIX_ASSEMBLY);
        let run_dir = run(AssembleArgs {
            project: tmp.path().to_path_buf(),
            name: String::from("claim-handler"),
        })
        .expect("run");
        let names = yaml_files(&run_dir);
        assert_eq!(names, vec!["default.yaml"]);
    }

    #[test]
    fn emitted_files_parse_back_with_meta_and_binding_preserved() {
        let tmp = project_with_assembly(SINGLE_AXIS_ASSEMBLY);
        let run_dir = run(AssembleArgs {
            project: tmp.path().to_path_buf(),
            name: String::from("claim-handler"),
        })
        .expect("run");

        let alpha_body =
            fs::read_to_string(run_dir.join("alpha.yaml")).expect("read alpha conversation");
        let conv = Conversation::from_yaml_str(&alpha_body).expect("conversation parses");
        assert_eq!(conv.meta.assembly.as_deref(), Some("claim-handler"));
        let case = conv.meta.binding.get("case").expect("binding case");
        assert_eq!(case.as_str(), Some("alpha"));
        // Single-axis assembly has no prefix and no conversation turns, so
        // the rendered session is empty.
        assert!(conv.session.is_empty());
    }

    #[test]
    fn dropping_uow_without_commit_leaves_runs_directory_empty() {
        let tmp = project_with_assembly(SINGLE_AXIS_ASSEMBLY);
        {
            let mut uow = AssembleUnitOfWork::open(tmp.path());
            let assembly = uow.assemblies.get("claim-handler").expect("load");
            for binding in assembly.expand_matrix() {
                let conv =
                    render_conversation(&assembly, &binding, uow.context.as_ref()).expect("render");
                uow.runs.stage(&binding, &conv).expect("stage");
            }
            // intentionally drop without commit
        }
        let runs_dir = tmp.path().join("runs");
        assert!(
            !runs_dir.exists() || fs::read_dir(&runs_dir).unwrap().next().is_none(),
            "runs dir should be absent or empty after rollback"
        );
    }

    #[test]
    fn templated_prefix_and_user_turn_render_against_insurance_claim_fixture() {
        let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("e2e/insurance-claim");

        let run_dir = run(AssembleArgs {
            project,
            name: String::from("claim-handler"),
        })
        .expect("run");

        let missing = fs::read_to_string(run_dir.join("missing-fields.yaml"))
            .expect("read missing-fields conversation");
        let conv = Conversation::from_yaml_str(&missing).expect("parses");

        let system_count = conv
            .session
            .iter()
            .filter(|m| matches!(m.role, Role::System))
            .count();
        assert_eq!(system_count, 4, "one System message per prefix block");

        let user_count = conv
            .session
            .iter()
            .filter(|m| matches!(m.role, Role::User))
            .count();
        assert_eq!(user_count, 1);

        let blank_assistant_count = conv
            .session
            .iter()
            .filter(|m| matches!(m.role, Role::Assistant) && m.body.is_none())
            .count();
        assert_eq!(blank_assistant_count, 1);
    }

    #[test]
    fn insurance_claim_fixture_assigns_cache_flags_per_prefix_block() {
        let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("e2e/insurance-claim");

        let run_dir = run(AssembleArgs {
            project,
            name: String::from("claim-handler"),
        })
        .expect("run");

        let body = fs::read_to_string(run_dir.join("missing-fields.yaml"))
            .expect("read missing-fields conversation");
        let conv = Conversation::from_yaml_str(&body).expect("parses");

        let systems: Vec<&Message<Rendered>> = conv
            .session
            .iter()
            .filter(|m| matches!(m.role, Role::System))
            .collect();
        assert_eq!(
            systems.len(),
            4,
            "four prefix blocks → four System messages"
        );
        // Prefix block declaration order: file, system, tools, examples.
        // Cache assignments from the fixture: true, true, true, false.
        assert!(systems[0].cache, "file block: cache true");
        assert!(systems[1].cache, "system block: cache true");
        assert!(systems[2].cache, "tools block: cache true");
        assert!(!systems[3].cache, "examples block: cache false");
    }

    #[test]
    fn two_prefix_blocks_emit_two_system_messages_in_declaration_order() {
        let tmp = tempfile::tempdir().expect("tempdir");
        // Build a synthetic project with file + system glob prefix blocks.
        let assemblies = tmp.path().join("assemblies");
        fs::create_dir_all(&assemblies).expect("mkdir assemblies");
        fs::write(tmp.path().join("AGENTS.md"), "agents body").expect("write AGENTS");
        let sys_dir = tmp.path().join("ctx/system");
        fs::create_dir_all(&sys_dir).expect("mkdir ctx/system");
        fs::write(sys_dir.join("a.md"), "alpha").expect("write a");
        fs::write(sys_dir.join("b.md"), "beta").expect("write b");
        let assembly = "\
name: two-blocks
model: claude-opus-4-7
prefix:
  - { kind: file,   path: AGENTS.md,         cache: true }
  - { kind: system, path: ctx/system/*.md,   cache: false }
";
        fs::write(assemblies.join("two-blocks.yaml"), assembly).expect("write assembly");

        let run_dir = run(AssembleArgs {
            project: tmp.path().to_path_buf(),
            name: String::from("two-blocks"),
        })
        .expect("run");

        let body =
            fs::read_to_string(run_dir.join("default.yaml")).expect("read default conversation");
        let conv = Conversation::from_yaml_str(&body).expect("parses");

        let systems: Vec<&Message<Rendered>> = conv
            .session
            .iter()
            .filter(|m| matches!(m.role, Role::System))
            .collect();
        assert_eq!(systems.len(), 2);
        // Declaration order is preserved: file block first, system block second.
        assert!(systems[0].cache, "file block carries cache: true");
        assert!(!systems[1].cache, "system block carries cache: false");
        match &systems[0].body {
            Some(Content::Text(s)) => assert_eq!(s, "agents body"),
            other => panic!("file block body should be Text, got {other:?}"),
        }
        match &systems[1].body {
            Some(Content::Text(s)) => assert_eq!(s, "alpha\nbeta"),
            other => panic!("system block body should be Text, got {other:?}"),
        }
    }

    #[test]
    fn context_block_count_truncates_glob_after_sort() {
        use crate::content::assembly::Binding;
        use crate::content::repository::FsContextRepository;

        let tmp = tempfile::tempdir().expect("tempdir");
        let ctx_dir = tmp.path().join("ctx");
        fs::create_dir_all(&ctx_dir).expect("mkdir ctx");
        for (i, name) in ["01.md", "02.md", "03.md", "04.md", "05.md"]
            .iter()
            .enumerate()
        {
            fs::write(ctx_dir.join(name), format!("body{i}")).expect("write context file");
        }

        let repo = FsContextRepository::new(tmp.path().to_path_buf());
        let block = PrefixBlock::Context {
            source: String::from("ctx"),
            glob: Some(String::from("*.md")),
            count: Some(2),
            cache: false,
        };
        let binding = Binding::default();
        let body = resolve_prefix_block(&block, &binding, &repo).expect("resolve");

        // Filename-ascending sort puts 01.md, 02.md first; bodies are body0
        // (01.md) and body1 (02.md), joined by a single newline.
        assert_eq!(body, "body0\nbody1");
    }

    #[test]
    fn two_runs_against_the_same_project_emit_byte_identical_conversation_files() {
        let tmp = project_with_assembly(SINGLE_AXIS_ASSEMBLY);

        let first = run(AssembleArgs {
            project: tmp.path().to_path_buf(),
            name: String::from("claim-handler"),
        })
        .expect("first run");
        let second = run(AssembleArgs {
            project: tmp.path().to_path_buf(),
            name: String::from("claim-handler"),
        })
        .expect("second run");

        let first_names = yaml_files(&first);
        let second_names = yaml_files(&second);
        assert_eq!(first_names, second_names);

        for name in &first_names {
            let a = fs::read_to_string(first.join(name)).expect("read first");
            let b = fs::read_to_string(second.join(name)).expect("read second");
            assert_eq!(
                a, b,
                "conversation body for {name} should be byte-identical across runs",
            );
        }
    }
}
