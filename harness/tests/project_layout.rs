//! Feature test for the `Project` aggregate over a vfs-backed root.
//!
//! User story: an operator runs `ailly assemble claim-handler` twice in
//! succession against the on-disk `e2e/insurance-claim` project. Each run
//! mints a fresh `runs/<id>-claim-handler/` directory containing one
//! conversation file per matrix binding (`default`, `missing-fields`,
//! `ambiguous`, `over-limit`). Across the two runs the conversation file
//! bodies are byte-identical; only the `runs/<id>/` directory segment
//! differs.
//!
//! Locks in the headline guarantee of the project-layout slice: the
//! migration from `Fs<T>` + `std::fs` to `Vfs<T>` + `vfs::PhysicalFS`,
//! the `ProjectPath` resolution boundary, and the `RunTx` Unit of Work
//! preserve the existing cli-assemble determinism end to end. A
//! nondeterminism leak in any layer (e.g. iteration-order escape in the
//! new `read_dir`-driven `glob_concat`, or an unstaged write in `RunTx`)
//! makes this test fail.

use std::fs;
use std::path::Path;
use std::path::PathBuf;

use ailly_two::cli::assemble::AssembleArgs;
use ailly_two::cli::assemble::run as assemble_run;

#[test]
fn assemble_against_insurance_claim_is_byte_identical_across_runs() {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("e2e/insurance-claim");

    let first = assemble_run(AssembleArgs {
        project: project.clone(),
        name: String::from("claim-handler"),
        ..Default::default()
    })
    .expect("first assemble against the insurance-claim project");

    let second = assemble_run(AssembleArgs {
        project,
        name: String::from("claim-handler"),
        ..Default::default()
    })
    .expect("second assemble against the insurance-claim project");

    assert_ne!(
        first, second,
        "RunTx::commit must mint a fresh run directory per call",
    );

    let first_files = sorted_yaml_files(&first);
    let second_files = sorted_yaml_files(&second);
    assert_eq!(
        first_files,
        vec![
            String::from("ambiguous.yaml"),
            String::from("default.yaml"),
            String::from("missing-fields.yaml"),
            String::from("over-limit.yaml"),
        ],
        "the claim-handler matrix has four cases; expected one yaml per case",
    );
    assert_eq!(first_files, second_files);

    for name in &first_files {
        let a = fs::read_to_string(first.join(name)).expect("read first run conversation");
        let b = fs::read_to_string(second.join(name)).expect("read second run conversation");
        assert_eq!(
            a, b,
            "conversation body for {name} must be byte-identical across runs",
        );
    }
}

fn sorted_yaml_files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("run dir readable")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "yaml"))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}
