//! Handler for `ailly assemble <name>`.
//!
//! Drives the Project aggregate's `RunTx` Unit of Work: open the project,
//! load the assembly, expand the matrix, render one conversation per
//! binding, stage each, commit.

use std::path::PathBuf;

use crate::content::assembly::AssemblyError;
use crate::content::assembly::RenderError;
use crate::content::project::Project;
use crate::content::project::ProjectError;
use crate::content::repository::AssemblyRepository;
use crate::content::repository::RepositoryError;

/// Arguments for the assemble handler. Public field list is fixed by the
/// feature test's struct-literal call site.
#[derive(Clone, Debug, Default)]
pub struct AssembleArgs {
    pub project: PathBuf,
    pub name: String,
    /// Repeatable `--case <name>` filter. Empty means no filter: every
    /// matrix binding is staged, exactly as before this field existed.
    pub cases: Vec<String>,
}

/// Errors emitted by the assemble handler. Library-boundary error; the CLI
/// entry point reformats via `Display` for stderr.
#[derive(Debug, thiserror::Error)]
pub enum AssembleError {
    #[error("project error: {0}")]
    Project(#[from] ProjectError),
    #[error("repository error: {0}")]
    Repository(#[from] RepositoryError),
    #[error("rendering failed: {0}")]
    Render(#[from] RenderError),
    #[error("matrix error: {0}")]
    Matrix(#[from] AssemblyError),
    #[error("assembling '{name}': {source}")]
    Assembling {
        name: String,
        #[source]
        source: Box<AssembleError>,
    },
    #[error("--case {requested:?} matched nothing; available cases: {available:?}")]
    UnknownCase {
        requested: Vec<String>,
        available: Vec<String>,
    },
}

/// Drive the assemble pipeline against `args.project`.
///
/// # Errors
///
/// Returns [`AssembleError::Project`] when the project root cannot be
/// opened, [`AssembleError::Repository`] when an underlying repository
/// call fails, or [`AssembleError::Render`] when template rendering or a
/// templated file read fails.
#[expect(
    clippy::needless_pass_by_value,
    reason = "by-value AssembleArgs is the call shape required by the feature test"
)]
pub fn run(args: AssembleArgs) -> Result<PathBuf, AssembleError> {
    let project = Project::open(&args.project)?;
    let run_dir = run_with_project(&project, &args.name)?;
    // VfsPath → PathBuf at the CLI boundary. The vfs::PhysicalFS root is
    // not exposed by VfsPath, so the as_str() form is a vfs-rooted path
    // ("/runs/<id>"); join it onto the host project root to produce the
    // host-facing PathBuf downstream callers and the feature test expect.
    let vfs_str = run_dir.as_str().trim_start_matches('/');
    Ok(args.project.join(vfs_str))
}

/// Project-typed core of the assemble pipeline. `run` wraps this with the
/// host-path bookkeeping; tests use it directly to exercise the pipeline
/// against an in-memory project without touching the disk.
///
/// # Errors
///
/// See [`AssembleError`].
fn run_with_project(project: &Project, assembly_name: &str) -> Result<vfs::VfsPath, AssembleError> {
    let assembly = project.assemblies().get(assembly_name)?;
    let mut tx = project.begin_run();
    for binding in assembly.expand_matrix()? {
        let conversation =
            assembly
                .render(project, &binding)
                .map_err(|e| AssembleError::Assembling {
                    name: assembly_name.to_string(),
                    source: Box::new(AssembleError::Render(e)),
                })?;
        tx.stage(&binding, &conversation)?;
    }
    Ok(tx.commit(&assembly.name)?)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::content::conversation::Content;
    use crate::content::conversation::Conversation;
    use crate::content::conversation::Message;
    use crate::content::conversation::Rendered;
    use crate::content::conversation::Role;

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

    fn yaml_files(dir: &std::path::Path) -> Vec<String> {
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
            ..Default::default()
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
    fn templated_prefix_and_user_turn_render_against_insurance_claim_fixture() {
        let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("e2e/insurance-claim");

        let run_dir = run(AssembleArgs {
            project,
            name: String::from("claim-handler"),
            ..Default::default()
        })
        .expect("run");

        let missing = fs::read_to_string(run_dir.join("missing-fields.yaml"))
            .expect("read missing-fields conversation");
        let conv = Conversation::from_yaml_str(&missing).expect("parses");

        let systems: Vec<&Message<Rendered>> = conv
            .session
            .iter()
            .filter(|m| matches!(m.role, Role::System))
            .collect();
        assert_eq!(systems.len(), 5, "one System message per prefix block");
        match &systems[4].body {
            Some(Content::Text(t)) => assert!(
                t.is_empty(),
                "fifth (knowledge) prefix block body should be empty against the placeholder corpus, got {t:?}",
            ),
            other => {
                panic!("expected Some(Content::Text(\"\")) for knowledge block, got {other:?}")
            }
        }

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
            ..Default::default()
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
            5,
            "five prefix blocks → five System messages"
        );
        // Prefix block declaration order: file, system, tools, examples, context.
        // Cache assignments from the fixture: true, true, true, false, false.
        assert!(systems[0].cache, "file block: cache true");
        assert!(systems[1].cache, "system block: cache true");
        assert!(systems[2].cache, "tools block: cache true");
        assert!(!systems[3].cache, "examples block: cache false");
        assert!(!systems[4].cache, "context (knowledge) block: cache false");
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
            ..Default::default()
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

    fn assemble_patterns_eval(name: &str) -> PathBuf {
        let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("e2e/patterns-eval");
        run(AssembleArgs {
            project,
            name: String::from(name),
            ..Default::default()
        })
        .expect("assemble patterns-eval")
    }

    #[test]
    fn patterns_eval_invocation_positive_arm_produces_skill_named_files() {
        let run_dir = assemble_patterns_eval("invocation");
        assert_eq!(
            yaml_files(&run_dir),
            vec![
                "configuring-logging.yaml",
                "emitting-logs.yaml",
                "newtype.yaml"
            ]
        );
    }

    #[test]
    fn patterns_eval_invocation_baseline_arm_produces_skill_named_files() {
        let run_dir = assemble_patterns_eval("baseline");
        assert_eq!(
            yaml_files(&run_dir),
            vec![
                "configuring-logging.yaml",
                "emitting-logs.yaml",
                "newtype.yaml"
            ]
        );
    }

    fn system_message_count(run_dir: &std::path::Path, file: &str) -> usize {
        let body = fs::read_to_string(run_dir.join(file)).expect("read conversation");
        let conv = Conversation::from_yaml_str(&body).expect("parses");
        conv.session
            .iter()
            .filter(|m| matches!(m.role, Role::System))
            .count()
    }

    #[test]
    fn patterns_eval_invocation_positive_arm_loads_exactly_one_skill_per_run() {
        let run_dir = assemble_patterns_eval("invocation");
        assert_eq!(
            system_message_count(&run_dir, "newtype.yaml"),
            4,
            "positive arm: root AGENTS.md + context AGENTS.md + using-patterns + one skill = 4 system messages"
        );
    }

    #[test]
    fn patterns_eval_invocation_baseline_arm_loads_no_skills() {
        let run_dir = assemble_patterns_eval("baseline");
        assert_eq!(
            system_message_count(&run_dir, "newtype.yaml"),
            2,
            "baseline arm: root AGENTS.md + context AGENTS.md, no skills = 2 system messages"
        );
    }

    #[test]
    fn error_for_missing_context_includes_assembly_name_and_real_path() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let assemblies = tmp.path().join("assemblies");
        fs::create_dir_all(&assemblies).expect("mkdir assemblies");
        let assembly_yaml = "\
name: test-assembly
model: claude-opus-4-7
prefix:
  - { kind: system, path: context/missing-dir/*.md }
";
        fs::write(assemblies.join("test-assembly.yaml"), assembly_yaml).expect("write assembly");

        let err = run(AssembleArgs {
            project: tmp.path().to_path_buf(),
            name: String::from("test-assembly"),
            ..Default::default()
        })
        .expect_err("should fail on missing context dir");

        let msg = err.to_string();
        assert!(
            msg.contains("test-assembly"),
            "error should name the assembly: {msg}"
        );
        assert!(
            msg.contains(tmp.path().to_str().expect("tmp path is valid utf-8")),
            "error should show real path, not vfs-relative path: {msg}"
        );
    }

    fn seed_memory_assembly(project: &Project, yaml: &str) {
        use std::io::Write;
        let assemblies = project.root().join("assemblies").expect("join assemblies");
        assemblies.create_dir_all().expect("mkdir assemblies");
        let mut f = assemblies
            .join("claim-handler.yaml")
            .expect("join name")
            .create_file()
            .expect("create file");
        f.write_all(yaml.as_bytes()).expect("write");
    }

    fn vfs_yaml_files(dir: &vfs::VfsPath) -> Vec<String> {
        let mut names: Vec<String> = dir
            .read_dir()
            .expect("read run dir")
            .filter(|e| e.extension().is_some_and(|ext| ext == "yaml"))
            .map(|e| e.filename())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn two_runs_against_the_same_project_emit_byte_identical_conversation_files() {
        let project = Project::open_memory();
        seed_memory_assembly(&project, SINGLE_AXIS_ASSEMBLY);

        let first = run_with_project(&project, "claim-handler").expect("first run");
        let second = run_with_project(&project, "claim-handler").expect("second run");

        assert_ne!(
            first.as_str(),
            second.as_str(),
            "two successive assembles must mint distinct run directories",
        );

        let first_names = vfs_yaml_files(&first);
        let second_names = vfs_yaml_files(&second);
        assert_eq!(first_names, second_names);

        for name in &first_names {
            let a = first
                .join(name)
                .unwrap()
                .read_to_string()
                .expect("read first");
            let b = second
                .join(name)
                .unwrap()
                .read_to_string()
                .expect("read second");
            assert_eq!(
                a, b,
                "conversation body for {name} should be byte-identical across runs",
            );
        }
    }
}
