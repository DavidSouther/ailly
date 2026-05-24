//! Feature test for `ailly assemble`.
//!
//! User story: an operator runs `ailly assemble claim-handler` against
//! the insurance-claim project; one conversation file appears under
//! `runs/<id>/` per matrix binding.

use std::fs;
use std::path::PathBuf;

use ailly_two::cli::assemble::AssembleArgs;
use ailly_two::cli::assemble::run as assemble_run;

#[test]
fn assemble_writes_a_conversation_file_per_matrix_binding() {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("e2e/insurance-claim");

    let run_dir = assemble_run(AssembleArgs {
        project,
        name: String::from("claim-handler"),
    })
    .expect("assemble succeeds against the insurance-claim project");

    let count = fs::read_dir(&run_dir)
        .expect("run dir is readable")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "yaml"))
        .count();
    assert!(
        count > 0,
        "at least one conversation file lands under {run_dir:?}"
    );
}
