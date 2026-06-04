//! Feature test for `ailly assemble`.
//!
//! User story: an operator runs `ailly assemble claim-handler` against
//! the insurance-claim project; one conversation file appears under
//! `runs/<id>/` per matrix binding.

use std::fs;
use std::path::Path;
use std::path::PathBuf;

use ailly_two::cli::assemble::AssembleArgs;
use ailly_two::cli::assemble::run as assemble_run;
use ailly_two::content::conversation::Conversation;

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

/// An assembly whose `provider:` axis carries YAML *maps*, crossed with a
/// scalar `domain:` axis. Mirrors the delegate-52 README's provider matrix:
/// each provider binding pairs a `name` (the label) with a `model` (the
/// per-binding override of the assembly-level default `model:`).
const MAP_PROVIDER_AXIS_ASSEMBLY: &str = "\
name: delegated-workflow
model: default-model
matrix:
  provider:
    - { name: anthropic, model: claude-opus-4-7 }
    - { name: openai,    model: gpt-5-turbo }
  domain: [prose-bio]
";

fn yaml_stems(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("run dir is readable")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "yaml"))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// User story: an operator declares a `provider:` matrix axis whose values
/// are maps of `{ name, model }`, crossed with a scalar `domain:` axis. They
/// run `ailly assemble`. Each map-valued binding must:
///   1. label its conversation file by the map's `name` field (so the file is
///      `prose-bio-anthropic.yaml`, never a stem containing a serialized map's
///      `:` or newline), and
///   2. carry the map's `model` field as the conversation's `meta.model`,
///      overriding the assembly-level default `model: default-model`.
#[test]
fn map_valued_provider_axis_labels_by_name_and_overrides_model_per_binding() {
    // Arrange: a tempdir project holding the map-valued provider assembly.
    let tmp = tempfile::tempdir().expect("tempdir");
    let project = tmp.path().to_path_buf();
    let assemblies = project.join("assemblies");
    fs::create_dir_all(&assemblies).expect("create assemblies/");
    fs::write(
        assemblies.join("delegated-workflow.yaml"),
        MAP_PROVIDER_AXIS_ASSEMBLY,
    )
    .expect("write assembly");

    // Act: assemble exactly as the binary will.
    let run_dir = assemble_run(AssembleArgs {
        project,
        name: String::from("delegated-workflow"),
    })
    .expect("assemble succeeds against the map-valued provider assembly");

    // Assert (1): filenames derive from the map's `name`, crossed with the
    // scalar domain. The map never serializes into the stem. The join order is
    // BTreeMap key order (`domain` < `provider`), so the scalar domain leads;
    // multi-axis filename ordering is a separate `filename_for` task.
    let stems = yaml_stems(&run_dir);
    assert_eq!(
        stems,
        vec!["prose-bio-anthropic.yaml", "prose-bio-openai.yaml"],
        "map-valued binding labels by `name`, never a serialized map",
    );
    for stem in &stems {
        assert!(
            !stem.contains(':') && !stem.contains('\n') && !stem.contains('{'),
            "filename `{stem}` must not contain a serialized map",
        );
    }

    // Assert (2): each conversation's `meta.model` is the map's `model`,
    // overriding the assembly-level default `model: default-model`.
    let anthropic = fs::read_to_string(run_dir.join("prose-bio-anthropic.yaml"))
        .expect("read anthropic conversation");
    let anthropic_conv = Conversation::from_yaml_str(&anthropic).expect("conversation parses");
    assert_eq!(
        anthropic_conv.meta.model.as_ref(),
        "claude-opus-4-7",
        "anthropic binding's meta.model is its map `model`, not the default",
    );

    let openai = fs::read_to_string(run_dir.join("prose-bio-openai.yaml"))
        .expect("read openai conversation");
    let openai_conv = Conversation::from_yaml_str(&openai).expect("conversation parses");
    assert_eq!(
        openai_conv.meta.model.as_ref(),
        "gpt-5-turbo",
        "openai binding's meta.model is its map `model`, not the default",
    );

    // Assert (3): the binding's identity records the scalar `name`, not the
    // map. This is what an eval `when: { provider: anthropic }` subset-matches.
    let provider = anthropic_conv
        .meta
        .binding
        .get("provider")
        .expect("provider axis recorded in meta.binding");
    assert_eq!(
        provider.as_str(),
        Some("anthropic"),
        "meta.binding stores the map's `name` as a scalar string",
    );
}
