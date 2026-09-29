//! Each case specifies the complete set of accesses for a command; extra
//! accesses fail the test. `verify_corpus.py` separately validates corpus schema
//! and deidentification.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::Deserialize;
use shell_access::{
    AccessOperation, AccessTarget, AmbiguityReason, ClassificationError, Classifier, FileAccess,
};

const CORPUS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/corpus");

#[derive(Deserialize)]
struct Header {
    cwd: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    command: String,
    /// Which shapes this case pins. `verify_corpus.py` owns the vocabulary; this
    /// test reads the expectations rather than the labels.
    #[allow(dead_code)]
    tags: Vec<String>,
    cwd: Option<String>,
    /// Absent when the command should produce no access at all, which is the
    /// right answer for an unattributed utility or a reader taking stdin.
    #[serde(default)]
    expect: Expect,
}

/// Sparse by design: a key the case omits is an assertion that the classifier
/// emits nothing of that kind, not a default to be filled in.
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Expect {
    #[serde(default)]
    reads: Vec<String>,
    #[serde(default)]
    writes: Vec<String>,
    #[serde(default)]
    deletes: Vec<String>,
    #[serde(default)]
    ambiguous: Vec<Fragment>,
    #[serde(default)]
    directories: Vec<TargetAccess>,
    #[serde(default)]
    scripting: bool,
    error: Option<String>,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct Fragment {
    path: String,
    op: String,
    reason: String,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct TargetAccess {
    path: String,
    op: String,
}

fn corpus() -> Vec<(String, Case)> {
    let mut files: Vec<_> = fs::read_dir(CORPUS)
        .expect("the corpus directory")
        .map(|entry| entry.expect("a corpus directory entry").path())
        .filter(|path| path.extension().is_some_and(|kind| kind == "yaml"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no corpus files under {CORPUS}");

    let mut cases = Vec::new();
    for file in files {
        let text = fs::read_to_string(&file).expect("a readable corpus file");
        let mut documents = serde_yaml_ng::Deserializer::from_str(&text);
        let header = Header::deserialize(documents.next().expect("a header document"))
            .unwrap_or_else(|error| panic!("{}: header: {error}", file.display()));
        let named =
            BTreeMap::<String, Case>::deserialize(documents.next().expect("a document of cases"))
                .unwrap_or_else(|error| panic!("{}: cases: {error}", file.display()));

        for (id, mut case) in named {
            case.cwd = case.cwd.or_else(|| header.cwd.clone());
            cases.push((id, case));
        }
    }
    cases
}

fn case(id: &str) -> Case {
    let (_, case) = corpus()
        .into_iter()
        .find(|(name, _)| name == id)
        .unwrap_or_else(|| panic!("the corpus no longer holds the case `{id}`"));
    case
}

fn accesses(case: &Case) -> Vec<FileAccess> {
    let (accesses, errors) = classify(case);
    assert!(errors.is_empty(), "unexpected {errors:?}");
    accesses
}

fn classify(case: &Case) -> (Vec<FileAccess>, Vec<ClassificationError>) {
    let cwd = case
        .cwd
        .as_deref()
        .expect("a case's own cwd, or its file header's");
    let mut found = Vec::new();
    let mut errors = Vec::new();
    for result in Classifier::POSIX.with_cwd(cwd).classify(&case.command) {
        match result {
            Ok(access) => found.push(access),
            Err(error) => errors.push(error),
        }
    }
    (found, errors)
}

/// Ambiguous fragments are asserted separately.
fn paths(accesses: &[FileAccess], op: AccessOperation) -> Vec<String> {
    accesses
        .iter()
        .filter(|access| {
            access.target == AccessTarget::File && access.op == op && access.ambiguity.is_none()
        })
        .map(|access| access.path.clone())
        .collect()
}

fn fragments(accesses: &[FileAccess]) -> Vec<Fragment> {
    accesses
        .iter()
        .filter_map(|access| {
            Some(Fragment {
                path: access.path.clone(),
                op: access.op.as_str().to_string(),
                reason: reason(access.ambiguity?).to_string(),
            })
        })
        .collect()
}

/// `ambiguous` is the single home for every fragment, whatever it targets, so a
/// directory the shell would have expanded is named once rather than twice.
fn directories(accesses: &[FileAccess]) -> Vec<TargetAccess> {
    accesses
        .iter()
        .filter(|access| access.target == AccessTarget::Directory && access.ambiguity.is_none())
        .map(|access| TargetAccess {
            path: access.path.clone(),
            op: access.op.as_str().to_string(),
        })
        .collect()
}

fn reason(ambiguity: AmbiguityReason) -> &'static str {
    match ambiguity {
        AmbiguityReason::Glob => "glob",
        AmbiguityReason::Expansion => "expansion",
        AmbiguityReason::CommandSubstitution => "command_substitution",
        AmbiguityReason::ExpandedHeredoc => "expanded_heredoc",
    }
}

fn check(id: &str, case: &Case) {
    let (found, errors) = classify(case);

    if case.expect.error.as_deref() == Some("parse") {
        assert!(!errors.is_empty(), "{id}: expected a parse error, got none");
        assert!(
            errors
                .iter()
                .all(|error| *error == ClassificationError::Parse),
            "{id}: expected only parse errors, got {errors:?}"
        );
        assert!(
            found.is_empty(),
            "{id}: text that does not parse attributes nothing, got {found:?}"
        );
        return;
    }

    assert!(errors.is_empty(), "{id}: {errors:?}");
    assert_eq!(
        paths(&found, AccessOperation::Read),
        case.expect.reads,
        "{id}: reads"
    );
    assert_eq!(
        paths(&found, AccessOperation::Write),
        case.expect.writes,
        "{id}: writes"
    );
    assert_eq!(
        paths(&found, AccessOperation::Delete),
        case.expect.deletes,
        "{id}: deletes"
    );
    assert_eq!(fragments(&found), case.expect.ambiguous, "{id}: ambiguous");
    assert_eq!(
        directories(&found),
        case.expect.directories,
        "{id}: directories"
    );

    // The case names every access, so anything left over is an invented row.
    let named = case.expect.reads.len()
        + case.expect.writes.len()
        + case.expect.deletes.len()
        + case.expect.ambiguous.len()
        + case.expect.directories.len();
    assert_eq!(
        found.len(),
        named,
        "{id}: no access beyond the ones the case names, got {found:?}"
    );

    let cwd = case.cwd.as_deref().expect("a cwd");
    for access in &found {
        assert_eq!(
            access.scripting, case.expect.scripting,
            "{id}: scripting, on {access:?}"
        );
        // Recorded context, copied onto the access and never joined onto its
        // path — which the relative paths above are the other half of.
        assert_eq!(
            access.cwd.as_deref(),
            Some(Path::new(cwd)),
            "{id}: recorded working directory"
        );
    }
}

#[test]
fn every_corpus_case_classifies_the_way_it_says_it_should() {
    let cases = corpus();
    // A loader that silently found nothing, or only the first file, would
    // otherwise pass this test. The floor is well under the current count so
    // that adding cases is never blocked on editing it.
    assert!(cases.len() > 50, "read only {} cases", cases.len());

    for (id, case) in &cases {
        check(id, case);
    }
}

/// The two false positives this feature was built around, asserted as absences:
/// a script operand and a file descriptor are the words in a command that most
/// look like names and are not ones.
#[test]
fn neither_a_script_operand_nor_a_file_descriptor_is_a_path() {
    let script = accesses(&case("sed-script-first"));
    assert_eq!(paths(&script, AccessOperation::Read), ["src/lib.rs"]);
    assert!(
        !script.iter().any(|access| access.path.contains("220p")),
        "a script operand came back as a path: {script:?}"
    );

    let merged = accesses(&case("fd-merge"));
    assert_eq!(paths(&merged, AccessOperation::Read), ["src/lib.rs"]);
    assert!(
        !merged
            .iter()
            .any(|access| access.path.contains('&') || access.path.len() == 1),
        "a file descriptor came back as a path: {merged:?}"
    );
}

/// tree-sitter-bash issue #233: the word after a redirect destination parses as
/// part of the redirect. It is an argument, and `printf`'s arguments are not
/// files.
#[test]
fn a_word_after_a_redirect_destination_is_not_a_second_destination() {
    let found = accesses(&case("issue-233-word-after-destination"));

    assert_eq!(paths(&found, AccessOperation::Write), ["build/out.env"]);
    assert_eq!(found.len(), 1, "{found:?}");
}

/// A heredoc delimiter decides whether the body is a literal: unquoted, the
/// shell expands it, so what the utility read is not what the transcript shows.
#[test]
fn an_unquoted_heredoc_body_is_ambiguous_and_a_quoted_one_is_not() {
    let expanded = accesses(&case("unquoted-heredoc-body-is-expanded"));
    assert_eq!(
        fragments(&expanded)
            .iter()
            .map(|fragment| fragment.reason.clone())
            .collect::<Vec<_>>(),
        ["expanded_heredoc"]
    );

    let literal = accesses(&case("quoted-heredoc-body-is-literal"));
    assert_eq!(fragments(&literal), []);
    assert_eq!(paths(&literal, AccessOperation::Write), ["build/out.env"]);
}

/// A utility the operand table does not cover is reported as itself: no file
/// row, however plainly a path sits in its arguments. The table is additive, and
/// guessing here is what would make every row less trustworthy.
#[test]
fn a_utility_outside_the_table_invents_no_file_row() {
    for id in [
        "git-naming-a-path-has-no-file-row",
        "gh-list-has-no-file-row",
    ] {
        assert_eq!(accesses(&case(id)), [], "{id}");
    }
}

/// A directory is its own claim: these commands take a directory as their
/// subject, and saying "file" of `src` would say something the command did not.
/// `find`'s expression is the other half — `-type f` names no path at all.
#[test]
fn a_directory_operand_is_reported_as_a_directory() {
    assert_eq!(
        directories(&accesses(&case("find-stops-at-its-expression"))),
        [
            TargetAccess {
                path: "src".to_string(),
                op: "read".to_string(),
            },
            TargetAccess {
                path: "tests".to_string(),
                op: "read".to_string(),
            },
        ]
    );

    let listed = accesses(&case("ls-reads-the-directory-it-lists"));
    assert_eq!(paths(&listed, AccessOperation::Read), Vec::<String>::new());
    assert_eq!(
        directories(&listed),
        [TargetAccess {
            path: "src".to_string(),
            op: "read".to_string(),
        }]
    );
}
