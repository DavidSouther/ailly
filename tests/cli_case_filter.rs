//! Feature test for `--case <name>` filtering on `assemble`/`run`/`eval`.
//!
//! User story: a developer edited one skill's SKILL.md in a multi-skill
//! suite and wants to assemble, run, and eval just that skill's case,
//! without processing every other skill in the matrix/run directory.
//!
//! Given: a tempdir project with a 3-skill assembly (`skills.yaml`,
//! `model: noop`, matrix axis `skill: [newtype, logging, tracing]`) and a
//! matching 3-case eval suite (`skills-eval.yaml`).
//! When: `assemble --case newtype` targets one skill; a separate unfiltered
//! `assemble` produces all three; `run --case logging --case tracing`
//! (the repeatable flag) targets two of the three already-assembled files;
//! then `eval --case newtype` targets one of the three.
//! Then:
//!   - the filtered assemble writes exactly one conversation file, not three;
//!   - the unfiltered assemble is unchanged (regression guard): all three;
//!   - the filtered, repeatable-flag run processes exactly the two named
//!     cases and leaves the third file's blank assistant turn untouched;
//!   - the filtered eval matches and scores exactly one conversation, not
//!     three, against only the suite case that names it.
//!
//! This is RED until `cases: Vec<String>` and the shared exact-match filter
//! predicate are threaded through `assemble::run`, `run::run`, and
//! `eval::run`.

use std::fs;

use ailly_two::cli::assemble::AssembleArgs;
use ailly_two::cli::assemble::run as assemble_run;
use ailly_two::cli::eval::EvalCmdArgs;
use ailly_two::cli::eval::run as eval_run;
use ailly_two::cli::run::RunArgs;
use ailly_two::cli::run::run as run_run;
use ailly_two::content::conversation::Conversation;
use ailly_two::content::conversation::Role;

const ASSEMBLY_YAML: &str = "\
name: skills
model: noop

matrix:
  skill: [newtype, logging, tracing]

conversation:
  - { role: user, path: \"prompts/{{ skill }}.md\" }
  - { role: assistant }
";

const SUITE_YAML: &str = "\
name: skills-eval
cases:
  - name: newtype
    assertions:
      - { type: text_contains, value: \"noop\" }
  - name: logging
    assertions:
      - { type: text_contains, value: \"noop\" }
  - name: tracing
    assertions:
      - { type: text_contains, value: \"noop\" }
";

fn write_project() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = tmp.path();

    let assemblies = root.join("assemblies");
    fs::create_dir_all(&assemblies).expect("mkdir assemblies");
    fs::write(assemblies.join("skills.yaml"), ASSEMBLY_YAML).expect("write assembly");

    let prompts = root.join("prompts");
    fs::create_dir_all(&prompts).expect("mkdir prompts");
    for skill in ["newtype", "logging", "tracing"] {
        fs::write(
            prompts.join(format!("{skill}.md")),
            format!("Explain {skill}."),
        )
        .expect("write prompt");
    }

    let evals = root.join("evals");
    fs::create_dir_all(&evals).expect("mkdir evals");
    fs::write(evals.join("skills-eval.yaml"), SUITE_YAML).expect("write suite");

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

fn has_blank_assistant(dir: &std::path::Path, file: &str) -> bool {
    let body = fs::read_to_string(dir.join(file)).expect("read conversation");
    let conv = Conversation::from_yaml_str(&body).expect("parses");
    conv.session
        .iter()
        .any(|m| matches!(m.role, Role::Assistant) && m.body.is_none())
}

#[tokio::test]
async fn case_filter_scopes_assemble_run_and_eval_to_selected_cases() {
    let tmp = write_project();
    let project = tmp.path().to_path_buf();

    // Act 1: assemble filtered to one case.
    let filtered_run_dir = assemble_run(AssembleArgs {
        project: project.clone(),
        name: String::from("skills"),
        cases: vec![String::from("newtype")],
    })
    .expect("filtered assemble");

    // Assert: only the requested case's file was written.
    assert_eq!(
        yaml_files(&filtered_run_dir),
        vec!["newtype.yaml"],
        "assemble --case newtype writes exactly one conversation file",
    );

    // Act 2: assemble with no filter — regression guard for existing behavior.
    let full_run_dir = assemble_run(AssembleArgs {
        project: project.clone(),
        name: String::from("skills"),
        cases: vec![],
    })
    .expect("unfiltered assemble");

    assert_eq!(
        yaml_files(&full_run_dir),
        vec!["logging.yaml", "newtype.yaml", "tracing.yaml"],
        "omitting --case is unchanged: every matrix binding is still written",
    );

    // Act 3: run filtered to two of the three cases, via the repeatable flag.
    let run_outcome = run_run(RunArgs {
        project: project.clone(),
        target: full_run_dir.clone(),
        cases: vec![String::from("logging"), String::from("tracing")],
    })
    .await
    .expect("filtered run");

    assert_eq!(
        run_outcome.conversations_processed, 2,
        "run --case logging --case tracing processes exactly the two named cases",
    );
    assert!(
        !has_blank_assistant(&full_run_dir, "logging.yaml"),
        "logging.yaml's blank assistant turn was filled",
    );
    assert!(
        !has_blank_assistant(&full_run_dir, "tracing.yaml"),
        "tracing.yaml's blank assistant turn was filled",
    );
    assert!(
        has_blank_assistant(&full_run_dir, "newtype.yaml"),
        "newtype.yaml was not named by --case and must be left untouched",
    );

    // Act 4: fill the remaining case directly (still via --case, naming the
    // one left over) so eval has a fully-run directory to filter over,
    // matching the real workflow where eval runs against a directory the
    // whole suite has already populated.
    run_run(RunArgs {
        project: project.clone(),
        target: full_run_dir.clone(),
        cases: vec![String::from("newtype")],
    })
    .await
    .expect("fill remaining case");

    // Act 5: eval filtered to one case out of the fully-run directory.
    let eval_outcome = eval_run(EvalCmdArgs {
        project: project.clone(),
        suite: String::from("skills-eval"),
        over: full_run_dir.clone(),
        cases: vec![String::from("newtype")],
    })
    .await
    .expect("filtered eval");

    assert_eq!(
        eval_outcome.conversations_matched, 1,
        "eval --case newtype matches exactly one conversation, not all three",
    );
    assert_eq!(
        eval_outcome.assertions_passed, 1,
        "only the newtype suite case's assertion runs, and it passes against the noop reply",
    );
    assert_eq!(eval_outcome.assertions_failed, 0);
    assert_eq!(
        eval_outcome.assertions_malformed, 0,
        "the logging and tracing suite cases (named, but excluded by --case newtype) must not \
         synthesize a 'no conversation found for case name' malformed outcome — the suite's own \
         case list is filtered alongside the conversation list, not just the conversation list",
    );
    assert!(eval_outcome.report_path.exists(), "report file written");
}
