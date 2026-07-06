//! Feature test for the `kind: external` prefix-block.
//!
//! User story: an operator's assembly prefix points at a file in a *sibling*
//! repo — outside the project root — via `{ kind: external, path: ../sib/x.md
//! }`. Running `ailly assemble` produces a conversation whose system prefix
//! carries that sibling file's text, with no vendored copy living inside the
//! project.
//!
//! Design: docs/developer/2026-06-07-B-external-prefix-block/design.md
//! (Metric 1, "Sibling read works", and Metric 4, "Replay stays hermetic" — the
//! resolved text is pinned inline in the committed run artifact, which this
//! test reads back from disk.)

use std::fs;

use ailly_two::cli::assemble::AssembleArgs;
use ailly_two::cli::assemble::run as assemble_run;
use ailly_two::content::conversation::Content;
use ailly_two::content::conversation::Conversation;
use ailly_two::content::conversation::Message;
use ailly_two::content::conversation::Rendered;
use ailly_two::content::conversation::Role;

/// The sibling file's body. Distinctive so the prefix assertion is unambiguous
/// and cannot accidentally match boilerplate.
const SIBLING_BODY: &str = "# Sibling SKILL\n\nLoaded from outside the harness root.\n";

/// An assembly whose only prefix block escapes the project root with `..` to
/// read a sibling file. Empty matrix, so `assemble` writes one `default.yaml`.
const EXTERNAL_PREFIX_ASSEMBLY: &str = "\
name: skill-harness
model: claude-opus-4-7
prefix:
  - { kind: external, path: ../sib/SKILL.md, cache: true }
";

/// An assembly prefix names a file in a sibling repo via `kind: external`;
/// `ailly assemble` renders that sibling's text into the conversation's system
/// prefix, and the committed run artifact carries the text inline.
#[test]
fn external_prefix_block_renders_sibling_file_text_into_the_system_prefix() {
    // Arrange: one parent dir holding two true siblings — the project root and
    // the external `sib` repo. `Project::open` canonicalizes the root, so the
    // siblings must share a real (post-symlink-resolution) parent; placing both
    // under the same tempdir guarantees that.
    let parent = tempfile::tempdir().expect("tempdir");
    let project = parent.path().join("project");
    let sibling = parent.path().join("sib");
    fs::create_dir_all(project.join("assemblies")).expect("create project/assemblies/");
    fs::create_dir_all(&sibling).expect("create sibling repo dir");

    fs::write(sibling.join("SKILL.md"), SIBLING_BODY).expect("write sibling SKILL.md");
    fs::write(
        project.join("assemblies/skill-harness.yaml"),
        EXTERNAL_PREFIX_ASSEMBLY,
    )
    .expect("write assembly");

    // Act: assemble exactly as the binary will.
    let run_dir = assemble_run(AssembleArgs {
        project: project.clone(),
        name: String::from("skill-harness"),
        ..Default::default()
    })
    .expect("assemble succeeds with an external prefix block pointing at a sibling repo");

    // Assert: the committed conversation's single system prefix message carries
    // the sibling file's text verbatim. Reading it back from disk is what proves
    // the resolved text is pinned inline in the run artifact (hermetic replay).
    let body = fs::read_to_string(run_dir.join("default.yaml")).expect("read conversation file");
    let conv = Conversation::from_yaml_str(&body).expect("conversation parses");

    let systems: Vec<&Message<Rendered>> = conv
        .session
        .iter()
        .filter(|m| matches!(m.role, Role::System))
        .collect();
    assert_eq!(
        systems.len(),
        1,
        "the one external prefix block renders exactly one system message",
    );
    match &systems[0].body {
        Some(Content::Text(text)) => assert_eq!(
            text, SIBLING_BODY,
            "system prefix carries the sibling file's text verbatim",
        ),
        other => panic!("expected Some(Content::Text(..)) for the external block, got {other:?}"),
    }
    assert!(
        systems[0].cache,
        "the external block's `cache: true` flows onto its system message",
    );

    // Assert: no vendored copy of the sibling file lives inside the project. The
    // text reached the window by escaping the root, not by being copied in.
    assert!(
        !project.join("context/SKILL.md").exists(),
        "the sibling file is read in place, never vendored into the project",
    );
}
