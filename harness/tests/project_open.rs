//! Filesystem-backed tests for `Project::open`.
//!
//! `Project::open` validates that the input path canonicalizes to an
//! existing directory; that contract requires a real path on disk and
//! therefore lives outside the `src/content/project.rs` test module so
//! that module stays tempfile-free under the CI lint guard.

use std::fs;

use ailly_two::content::project::Project;
use ailly_two::content::project::ProjectError;

#[test]
fn open_accepts_an_existing_directory() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let project = Project::open(tmp.path()).expect("open");
    assert!(project.root().exists().unwrap_or(false));
}

#[test]
fn open_rejects_a_missing_path() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let missing = tmp.path().join("does-not-exist");
    let err = Project::open(&missing).expect_err("missing path");
    assert!(matches!(err, ProjectError::Open { .. }), "got {err:?}");
}

#[test]
fn open_rejects_a_path_that_is_a_file() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let file = tmp.path().join("file.txt");
    fs::write(&file, "hi").expect("write file");
    let err = Project::open(&file).expect_err("file path");
    assert!(
        matches!(err, ProjectError::NotADirectory { .. }),
        "got {err:?}",
    );
}
