//! Feature test for the runner-matrix e2e slice.
//!
//! User story: a developer preparing a release-readiness check wants one
//! committed Ailly matrix covering every runner named in domain-driven-design
//! issue #29. Offline, they can assemble the matrix and verify the structure:
//! one conversation per runner, each with provider metadata in the source row,
//! a scalar runner binding, and a provider-routed `meta.model`. With
//! credentials, the same run directory becomes the live confirmation input.
//!
//! The fixture stays useful even without credentials: the offline assertions
//! catch missing rows, broken case filtering, and model ids that no longer
//! route through the intended provider family.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use ailly_two::cli::assemble::AssembleArgs;
use ailly_two::cli::assemble::run as assemble_run;
use ailly_two::content::conversation::Conversation;
use serde_yaml_ng::Mapping;
use serde_yaml_ng::Value;

#[derive(Clone, Copy, Debug)]
struct ExpectedRunner {
    slug: &'static str,
    provider: &'static str,
    display: &'static str,
    model_prefix: &'static str,
}

const EXPECTED_RUNNERS: &[ExpectedRunner] = &[
    ExpectedRunner {
        slug: "anthropic-haiku-4-5",
        provider: "anthropic",
        display: "Haiku 4.5",
        model_prefix: "claude-",
    },
    ExpectedRunner {
        slug: "anthropic-sonnet-4-6",
        provider: "anthropic",
        display: "Sonnet 4.6",
        model_prefix: "claude-",
    },
    ExpectedRunner {
        slug: "anthropic-sonnet-5",
        provider: "anthropic",
        display: "Sonnet 5",
        model_prefix: "claude-",
    },
    ExpectedRunner {
        slug: "anthropic-opus-4-8",
        provider: "anthropic",
        display: "Opus 4.8",
        model_prefix: "claude-",
    },
    ExpectedRunner {
        slug: "anthropic-fable",
        provider: "anthropic",
        display: "Fable",
        model_prefix: "claude-",
    },
    ExpectedRunner {
        slug: "openai-gpt-5-5",
        provider: "openai",
        display: "GPT-5.5",
        model_prefix: "gpt-",
    },
    ExpectedRunner {
        slug: "openai-gpt-5-4",
        provider: "openai",
        display: "GPT-5.4",
        model_prefix: "gpt-",
    },
    ExpectedRunner {
        slug: "openai-gpt-5-4-mini",
        provider: "openai",
        display: "GPT-5.4-mini",
        model_prefix: "gpt-",
    },
    ExpectedRunner {
        slug: "google-gemini-3-5-flash",
        provider: "google",
        display: "Gemini 3.5 Flash",
        model_prefix: "gemini-",
    },
    ExpectedRunner {
        slug: "google-gemini-3-1-pro-preview",
        provider: "google",
        display: "Gemini 3.1 Pro (Preview)",
        model_prefix: "gemini-",
    },
    ExpectedRunner {
        slug: "google-gemini-3-1-flash-lite",
        provider: "google",
        display: "Gemini 3.1 Flash-Lite",
        model_prefix: "gemini-",
    },
    ExpectedRunner {
        slug: "google-gemini-2-5-pro",
        provider: "google",
        display: "Gemini 2.5 Pro",
        model_prefix: "gemini-",
    },
    ExpectedRunner {
        slug: "bedrock-llama-3-3",
        provider: "bedrock",
        display: "Llama 3.3",
        model_prefix: "bedrock:",
    },
    ExpectedRunner {
        slug: "bedrock-llama-4-scout",
        provider: "bedrock",
        display: "Llama 4 Scout",
        model_prefix: "bedrock:",
    },
    ExpectedRunner {
        slug: "bedrock-mistral-large-3",
        provider: "bedrock",
        display: "Mistral Large 3",
        model_prefix: "bedrock:",
    },
    ExpectedRunner {
        slug: "bedrock-cohere-r-plus",
        provider: "bedrock",
        display: "Cohere R+",
        model_prefix: "bedrock:",
    },
];

#[test]
fn runner_matrix_assembles_every_issue_29_runner_with_provider_metadata() {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("e2e")
        .join("runner-matrix");
    let assembly_path = project.join("assemblies").join("live-confirmation.yaml");

    let assembly_yaml = fs::read_to_string(&assembly_path)
        .unwrap_or_else(|err| panic!("read runner matrix assembly {assembly_path:?}: {err}"));
    let assembly_doc: Value =
        serde_yaml_ng::from_str(&assembly_yaml).expect("runner matrix assembly parses as YAML");

    let rows = runner_rows(&assembly_doc);
    assert_eq!(
        rows.len(),
        EXPECTED_RUNNERS.len(),
        "the source matrix must contain every issue #29 runner row"
    );

    for expected in EXPECTED_RUNNERS {
        let row = rows
            .get(expected.slug)
            .unwrap_or_else(|| panic!("missing runner row {}", expected.slug));
        assert_eq!(
            row.get("provider").map(String::as_str),
            Some(expected.provider),
            "{} provider metadata",
            expected.slug
        );
        assert_eq!(
            row.get("display").map(String::as_str),
            Some(expected.display),
            "{} display name",
            expected.slug
        );
        let model = row
            .get("model")
            .unwrap_or_else(|| panic!("{} must include explicit model id", expected.slug));
        assert!(
            model.starts_with(expected.model_prefix),
            "{} model id {model:?} must route via provider prefix {:?}",
            expected.slug,
            expected.model_prefix
        );
    }

    let run_dir = assemble_run(AssembleArgs {
        project: project.clone(),
        name: String::from("live-confirmation"),
        ..Default::default()
    })
    .expect("unfiltered runner matrix assembles offline");

    let files = yaml_files(&run_dir);
    assert_eq!(
        files.len(),
        EXPECTED_RUNNERS.len(),
        "unfiltered assemble writes one conversation per runner"
    );

    for expected in EXPECTED_RUNNERS {
        let filename = format!("{}.yaml", expected.slug);
        assert!(
            files.contains(&filename),
            "unfiltered assemble should write {filename}"
        );
        let yaml = fs::read_to_string(run_dir.join(&filename))
            .unwrap_or_else(|err| panic!("read assembled {filename}: {err}"));
        let conv = Conversation::from_yaml_str(&yaml)
            .unwrap_or_else(|err| panic!("parse assembled {filename}: {err}"));
        assert!(
            conv.meta.model.as_ref().starts_with(expected.model_prefix),
            "{filename} meta.model {:?} must route through {:?}",
            conv.meta.model.as_ref(),
            expected.model_prefix
        );
        assert_eq!(
            conv.meta.binding.get("runner").and_then(Value::as_str),
            Some(expected.slug),
            "{filename} records scalar runner binding"
        );
    }

    let filtered_run_dir = assemble_run(AssembleArgs {
        project,
        name: String::from("live-confirmation"),
        cases: vec![String::from("openai-gpt-5-4-mini")],
    })
    .expect("runner matrix supports --case for one row");
    assert_eq!(
        yaml_files(&filtered_run_dir),
        vec![String::from("openai-gpt-5-4-mini.yaml")],
        "--case scopes the matrix to the selected runner"
    );
}

fn runner_rows(doc: &Value) -> BTreeMap<String, BTreeMap<String, String>> {
    let root = as_mapping(doc, "assembly root");
    let matrix = as_mapping(
        root.get(Value::from("matrix"))
            .expect("assembly has matrix"),
        "matrix",
    );
    let runner = matrix
        .get(Value::from("runner"))
        .and_then(Value::as_sequence)
        .expect("matrix.runner is a sequence");

    let mut rows = BTreeMap::new();
    for value in runner {
        let row = as_mapping(value, "runner row");
        let name = required_str(row, "name").to_string();
        let fields = ["provider", "display", "model"]
            .into_iter()
            .map(|key| (key.to_string(), required_str(row, key).to_string()))
            .collect();
        rows.insert(name, fields);
    }
    rows
}

fn as_mapping<'a>(value: &'a Value, label: &str) -> &'a Mapping {
    match value {
        Value::Mapping(map) => map,
        other => panic!("{label} must be a YAML mapping, got {other:?}"),
    }
}

fn required_str<'a>(map: &'a Mapping, key: &str) -> &'a str {
    map.get(Value::from(key))
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("runner row must include string field {key:?}"))
}

fn yaml_files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("read run dir")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "yaml"))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}
