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
//!
//! The insurance-claim assembly pins `model: claude-sonnet-4-6`. To keep
//! this test hermetic and deterministic, each assembled conversation's
//! `meta.model` is rewritten to `"noop"` before the run, so that
//! `open_engine_for_model` routes to `NoopEngine::auto()` rather than a
//! live provider. `run` builds one engine per conversation, so each file's
//! single blank fills with the first auto reply, `"noop-0"`.

use std::fs;
use std::path::PathBuf;

use ailly_two::cli::assemble::AssembleArgs;
use ailly_two::cli::assemble::run as assemble_run;
use ailly_two::cli::run::RunArgs;
use ailly_two::cli::run::run;
use ailly_two::content::conversation::Content;
use ailly_two::content::conversation::Conversation;
use ailly_two::content::conversation::ModelId;
use ailly_two::content::conversation::Role;

#[tokio::test]
async fn run_fills_every_blank_assistant_across_the_assembled_run_dir() {
    // Arrange: assemble the insurance-claim project to produce a real run
    // directory of four blank-tailed conversation files.
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("e2e/insurance-claim");
    let run_dir = assemble_run(AssembleArgs {
        project: project.clone(),
        name: String::from("claim-handler"),
        ..Default::default()
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

    // Rewrite each binding's model to `"noop"` so the run routes to
    // `NoopEngine::auto()` and stays hermetic (see module docs). Only engine
    // routing changes; the assembled prefix and user turns are untouched.
    for path in &file_paths {
        let yaml = fs::read_to_string(path).unwrap_or_else(|err| panic!("read {path:?}: {err}"));
        let mut conv = Conversation::from_yaml_str(&yaml)
            .unwrap_or_else(|err| panic!("parse {path:?}: {err}"));
        conv.meta.model = ModelId::from("noop");
        fs::write(
            path,
            conv.to_yaml_string()
                .unwrap_or_else(|err| panic!("emit {path:?}: {err}")),
        )
        .unwrap_or_else(|err| panic!("write {path:?}: {err}"));
    }

    // Act: invoke the handler over the directory form of the target.
    let outcome = run(RunArgs {
        project,
        target: run_dir.clone(),
        ..Default::default()
    })
    .await
    .expect("run succeeds against the assembled run dir");

    // Assert: outcome counts.
    assert_eq!(outcome.conversations_processed, 4);
    assert_eq!(outcome.blank_assistants_filled, 4);

    // Assert: every file on disk has had its blank assistant filled. Each
    // conversation gets its own `NoopEngine::auto()`, so the single blank in
    // each fills with the first auto reply, `"noop-0"`, and carries an inline
    // trace stamped with the `"noop"` model id.
    for path in &file_paths {
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
            Content::Text(text) => assert_eq!(text, "noop-0", "reply for {path:?}"),
            Content::Blocks(_) => {
                panic!("NoopEngine::auto should produce Content::Text in {path:?}")
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
