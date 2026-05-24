//! Feature test for `ailly run`.
//!
//! User story: an operator has already assembled the insurance-claim
//! project into `runs/<id>/`, producing four conversation files (one per
//! matrix binding), each ending in one blank assistant slot. The operator
//! runs `ailly run runs/<id>/`; every blank assistant slot is filled in
//! place, inline `trace` is attached to each filled turn, and the run
//! directory is the run artifact.
//!
//! This test drives the directory form of the `target` argument because
//! that is the form `e2e/insurance-claim/ci.sh` uses end-to-end. The
//! single-file form, the no-op case, the engine-failure case, the
//! missing-target case, and the non-claude `ModelNotFound` case are
//! covered by the unit tests inside `src/cli/run.rs` per the design doc.

use std::fs;
use std::path::PathBuf;

use ailly_two::cli::assemble::AssembleArgs;
use ailly_two::cli::assemble::run as assemble_run;
use ailly_two::cli::run::RunArgs;
use ailly_two::cli::run::run_with_engine;
use ailly_two::content::conversation::Content;
use ailly_two::content::conversation::Conversation;
use ailly_two::content::conversation::ModelId;
use ailly_two::content::conversation::Role;
use ailly_two::engine::engine::NoopEngine;

const REPLY_AMBIGUOUS: &str = "human-review: claim narrative is internally inconsistent.";
const REPLY_DEFAULT: &str =
    "auto-approve: claim is within policy threshold and required fields are present.";
const REPLY_MISSING_FIELDS: &str =
    "Could you share the date of loss and the policy number on file?";
const REPLY_OVER_LIMIT: &str = "human-review: claim amount exceeds auto-approval ceiling.";

#[tokio::test]
async fn run_fills_every_blank_assistant_across_the_assembled_run_dir() {
    // Arrange: assemble the insurance-claim project to produce a real run
    // directory of four blank-tailed conversation files.
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("e2e/insurance-claim");
    let run_dir = assemble_run(AssembleArgs {
        project: project.clone(),
        name: String::from("claim-handler"),
    })
    .expect("assemble succeeds against the insurance-claim project");

    let mut file_paths: Vec<PathBuf> = fs::read_dir(&run_dir)
        .expect("run dir is readable")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "yaml"))
        .collect();
    file_paths.sort();
    assert_eq!(
        file_paths.len(),
        4,
        "the claim-handler matrix expands to four bindings"
    );

    // Filename-ascending order maps to the matrix cases:
    //   ambiguous.yaml, default.yaml, missing-fields.yaml, over-limit.yaml.
    let scripted_replies = [
        REPLY_AMBIGUOUS,
        REPLY_DEFAULT,
        REPLY_MISSING_FIELDS,
        REPLY_OVER_LIMIT,
    ];
    let engine = NoopEngine::from_replies(scripted_replies);

    // Act: invoke the handler over the directory form of the target.
    let outcome = run_with_engine(
        RunArgs {
            project,
            target: run_dir.clone(),
        },
        Box::new(engine),
    )
    .await
    .expect("run_with_engine succeeds against the assembled run dir");

    // Assert: outcome counts.
    assert_eq!(outcome.conversations_processed, 4);
    assert_eq!(outcome.blank_assistants_filled, 4);

    // Assert: every file on disk has had its blank assistant filled with
    // the scripted reply for that binding, and carries an inline trace.
    for (path, expected_reply) in file_paths.iter().zip(scripted_replies) {
        let yaml =
            fs::read_to_string(path).unwrap_or_else(|err| panic!("read back {path:?}: {err}"));
        let conv = Conversation::from_yaml_str(&yaml)
            .unwrap_or_else(|err| panic!("re-parse {path:?}: {err}"));

        assert!(
            conv.next_blank_assistant().is_none(),
            "no blank assistant remains in {path:?}"
        );

        let assistant = conv
            .session
            .iter()
            .rfind(|m| m.role == Role::Assistant)
            .unwrap_or_else(|| panic!("{path:?} has a trailing assistant message"));

        match assistant
            .body
            .as_ref()
            .unwrap_or_else(|| panic!("{path:?} assistant body is filled"))
        {
            Content::Text(text) => assert_eq!(text, expected_reply, "reply for {path:?}"),
            Content::Blocks(_) => {
                panic!("NoopEngine::from_replies should produce Content::Text in {path:?}")
            }
        }

        let trace = assistant
            .trace
            .as_ref()
            .unwrap_or_else(|| panic!("{path:?} assistant carries inline trace"));
        assert_eq!(trace.model, ModelId::from("noop"));
    }

    // Assert: the run artifact is the run directory itself. No parallel
    // response.json / meta.yaml / window.txt files appear alongside.
    for entry in fs::read_dir(&run_dir).expect("re-list run dir") {
        let path = entry.expect("entry").path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        assert!(
            path.extension().is_some_and(|ext| ext == "yaml"),
            "no non-yaml artifact lands in the run dir; found {name:?}"
        );
    }
}
