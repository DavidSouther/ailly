//! What the classifier reports for one POSIX command, exercised through the
//! public API a caller actually has.
//!
//! The shapes here are the ones a naive walk gets confidently wrong: a script
//! operand read as a file, a file descriptor read as a path, a word after a
//! redirect read as a second destination.

use shell_access::{
    AccessOperation::{self, Delete, Read, Write},
    AccessTarget::{Directory, File},
    AmbiguityReason, ClassificationError, Classifier, FileAccess, ShellLanguage,
};

fn accesses(command: &str) -> Vec<FileAccess> {
    Classifier::POSIX
        .classify(command)
        .collect::<Result<Vec<_>, _>>()
        .expect("the command parses")
}

fn attributed(command: &str) -> Vec<(AccessOperation, String)> {
    accesses(command)
        .into_iter()
        .map(|access| (access.op, access.path))
        .collect()
}

#[test]
fn classifies_literal_reader_and_redirects() {
    let classifier = Classifier::POSIX.with_cwd("/work/app");

    let accesses: Vec<_> = classifier
        .classify("cat config/base.yml >> build/out.env")
        .collect::<Result<Vec<_>, _>>()
        .expect("the command parses");

    assert_eq!(
        accesses,
        [
            FileAccess {
                op: Read,
                target: File,
                path: "config/base.yml".to_string(),
                cwd: Some("/work/app".into()),
                ambiguity: None,
                scripting: false,
            },
            FileAccess {
                op: Write,
                target: File,
                path: "build/out.env".to_string(),
                cwd: Some("/work/app".into()),
                ambiguity: None,
                scripting: false,
            },
        ]
    );
}

#[test]
fn a_script_operand_is_not_a_file() {
    assert_eq!(
        attributed("sed -n '1,220p' src/lib.rs"),
        [(Read, "src/lib.rs".to_string())]
    );
}

#[test]
fn a_file_descriptor_is_not_a_path() {
    assert_eq!(
        attributed("rg needle src/lib.rs 2>&1"),
        [(Read, "src/lib.rs".to_string())]
    );
    assert_eq!(
        attributed("cargo build 2> build/build.err"),
        [(Write, "build/build.err".to_string())]
    );
}

/// tree-sitter-bash issue #233: `file_redirect` takes repeated destinations, so
/// the grammar itself offers `'done'` as a second one.
#[test]
fn a_word_after_a_redirect_destination_is_not_another_destination() {
    assert_eq!(
        attributed("printf > build/out.env 'done'"),
        [(Write, "build/out.env".to_string())]
    );
}

#[test]
fn reports_deletes_distinctly_from_writes() {
    assert_eq!(
        attributed("rm -f build/stale.env"),
        [(Delete, "build/stale.env".to_string())]
    );
}

#[test]
fn classifies_each_pipeline_stage_independently() {
    assert_eq!(
        attributed("cat config/base.yml | sed -n '1,10p' | tee build/head.env"),
        [
            (Read, "config/base.yml".to_string()),
            (Write, "build/head.env".to_string()),
        ]
    );
}

#[test]
fn peels_wrappers_down_to_the_command_they_run() {
    assert_eq!(
        attributed("sudo -u builder rm -f build/stale.env"),
        [(Delete, "build/stale.env".to_string())]
    );
    assert_eq!(
        attributed("env RUST_LOG=debug cat config/base.yml"),
        [(Read, "config/base.yml".to_string())]
    );
    assert_eq!(
        attributed("bash -lc 'cat config/base.yml >> build/out.env'"),
        [
            (Read, "config/base.yml".to_string()),
            (Write, "build/out.env".to_string()),
        ]
    );
}

#[test]
fn copies_read_every_operand_but_the_last_and_write_the_last() {
    assert_eq!(
        attributed("cp config/base.yml build/base.yml"),
        [
            (Read, "config/base.yml".to_string()),
            (Write, "build/base.yml".to_string()),
        ]
    );
}

#[test]
fn an_in_place_flag_turns_a_read_operand_into_a_write() {
    assert_eq!(
        attributed("sed -i 's/alpha/beta/' src/main.rs"),
        [(Write, "src/main.rs".to_string())]
    );
}

#[test]
fn unresolvable_operands_stay_visible_with_their_reason() {
    let glob = accesses("cat logs/*.txt");
    assert_eq!(glob[0].op, Read);
    assert_eq!(glob[0].path, "logs/*.txt");
    assert_eq!(glob[0].ambiguity, Some(AmbiguityReason::Glob));

    let expansion = accesses("cat \"$CONFIG_DIR/base.yml\"");
    assert_eq!(expansion[0].path, "$CONFIG_DIR/base.yml");
    assert_eq!(expansion[0].ambiguity, Some(AmbiguityReason::Expansion));

    let substitution = accesses("wc -l $(git ls-files)");
    assert_eq!(substitution[0].path, "$(git ls-files)");
    assert_eq!(
        substitution[0].ambiguity,
        Some(AmbiguityReason::CommandSubstitution)
    );
}

/// An unquoted delimiter means the shell expanded the body, so nothing in it can
/// be read literally. A quoted delimiter leaves the body alone, and a literal
/// body names no file.
#[test]
fn an_unquoted_heredoc_body_is_ambiguous_and_a_quoted_one_is_not() {
    let expanded = accesses("cat <<EOF > build/out.env\ngenerated for $TARGET\nEOF\n");
    assert_eq!(
        expanded
            .iter()
            .map(|access| (access.op, access.path.as_str(), access.ambiguity))
            .collect::<Vec<_>>(),
        [
            (Read, "<<EOF", Some(AmbiguityReason::ExpandedHeredoc)),
            (Write, "build/out.env", None),
        ]
    );

    let literal = accesses("cat <<'LIMIT' > build/out.env\ngenerated for $TARGET\nLIMIT\n");
    assert_eq!(attributed_of(&literal), [(Write, "build/out.env")]);
}

fn attributed_of(accesses: &[FileAccess]) -> Vec<(AccessOperation, &str)> {
    accesses
        .iter()
        .map(|access| (access.op, access.path.as_str()))
        .collect()
}

#[test]
fn stdin_only_readers_and_unknown_utilities_report_nothing() {
    assert_eq!(attributed("cat"), []);
    assert_eq!(attributed("cat config/base.yml | wc -l").len(), 1);
    assert_eq!(attributed("git diff --stat config/base.yml"), []);
}

#[test]
fn an_inline_interpreter_is_scripting_and_its_redirects_are_still_recorded() {
    let accesses = accesses("python3 -c 'print(1)' > build/out.env");
    assert_eq!(attributed_of(&accesses), [(Write, "build/out.env")]);
    assert!(accesses[0].scripting);

    assert_eq!(
        attributed("python3 -c 'open(\"build/out.env\", \"w\")'"),
        []
    );
    assert_eq!(attributed("python3 scripts/report.py"), []);
}

#[test]
fn a_compound_command_reports_both_of_its_commands_in_order() {
    assert_eq!(
        attributed("cat config/base.yml && rm -f build/stale.env"),
        [
            (Read, "config/base.yml".to_string()),
            (Delete, "build/stale.env".to_string()),
        ]
    );
}

#[test]
fn unparsable_text_is_an_error_rather_than_a_partial_attribution() {
    let records: Vec<_> = Classifier::POSIX.classify("cat 'config/base.yml").collect();
    assert_eq!(records, [Err(ClassificationError::Parse)]);
}

#[test]
fn a_compound_command_with_a_later_parse_error_keeps_the_prefix() {
    let records: Vec<_> = Classifier::POSIX
        .classify("cat config/base.yml && echo 'unterminated")
        .collect();
    assert!(matches!(
        records.last(),
        Some(Err(ClassificationError::Parse))
    ));
    assert_eq!(
        records
            .iter()
            .filter_map(|record| record.as_ref().ok())
            .map(|access| (access.op, access.path.as_str()))
            .collect::<Vec<_>>(),
        [(Read, "config/base.yml")]
    );
}

/// A language this crate does not implement is refused rather than read under
/// POSIX rules, so a later fish or PowerShell implementation changes nothing a
/// caller already consumes.
#[test]
fn an_unimplemented_language_is_refused_rather_than_read_as_posix() {
    let records: Vec<_> = Classifier::new()
        .language(ShellLanguage::Unsupported("powershell".to_string()))
        .classify("Get-Content config/base.yml")
        .collect();
    assert_eq!(
        records,
        [Err(ClassificationError::UnsupportedLanguage {
            language: "powershell".to_string()
        })]
    );

    let detected: Vec<_> = Classifier::new()
        .classify("#!/usr/bin/env fish\ncat config/base.yml")
        .collect();
    assert_eq!(
        detected,
        [Err(ClassificationError::UnsupportedLanguage {
            language: "fish".to_string()
        })]
    );
}

#[test]
fn a_recorded_directory_is_never_joined_onto_an_operand() {
    let access = &Classifier::POSIX
        .with_cwd("/work/other")
        .classify("cat notes/todo.md")
        .collect::<Result<Vec<_>, _>>()
        .expect("the command parses")[0];

    assert_eq!(access.path, "notes/todo.md");
    assert_eq!(
        access.cwd.as_deref(),
        Some(std::path::Path::new("/work/other"))
    );
}

#[test]
fn sort_output_flag_is_a_write_in_separate_and_attached_forms() {
    assert_eq!(
        attributed("sort -o build/out.env src/a.txt"),
        [
            (Write, "build/out.env".to_string()),
            (Read, "src/a.txt".to_string()),
        ]
    );
    assert_eq!(
        attributed("sort --output=build/out.env src/a.txt"),
        [
            (Write, "build/out.env".to_string()),
            (Read, "src/a.txt".to_string()),
        ]
    );
    assert_eq!(
        attributed("sort --output build/out.env src/a.txt"),
        [
            (Write, "build/out.env".to_string()),
            (Read, "src/a.txt".to_string()),
        ]
    );
}

/// A flag's value is an operand like any other, so the reason it stayed a
/// fragment has to travel with it. Without this the crate resolves `$OUT` into a
/// path it has no evidence for, which every positional already refuses to do.
#[test]
fn a_flag_value_keeps_the_reason_it_stayed_a_fragment() {
    let separate = accesses("sort -o $OUT src/a.txt");
    assert_eq!(separate[0].path, "$OUT");
    assert_eq!(separate[0].ambiguity, Some(AmbiguityReason::Expansion));

    let attached = accesses("sort --output=$OUT src/a.txt");
    assert_eq!(attached[0].path, "$OUT");
    assert_eq!(attached[0].ambiguity, Some(AmbiguityReason::Expansion));

    let glob = accesses("sort -o build/out*.env src/a.txt");
    assert_eq!(glob[0].ambiguity, Some(AmbiguityReason::Glob));

    let script = accesses("sed -f $SCRIPT src/a.txt");
    assert_eq!(script[0].path, "$SCRIPT");
    assert_eq!(script[0].ambiguity, Some(AmbiguityReason::Expansion));
}

/// BSD `sed -i ''` vs GNU attached suffix; unconsumed empty word steals the
/// script slot.
#[test]
fn the_bsd_in_place_suffix_is_not_an_operand() {
    assert_eq!(
        attributed("sed -i '' 's/old/new/' src/a.txt"),
        [(Write, "src/a.txt".to_string())]
    );
    assert_eq!(
        attributed("sed -i.bak 's/old/new/' src/a.txt"),
        [(Write, "src/a.txt".to_string())]
    );
}

/// A declared utility whose flag list is missing a value-taking flag is worse
/// than an undeclared one: the flag's value falls through as a positional, so
/// `root` and `wheel` are reported as files, and the extra operands also shift
/// which one is called the destination.
#[test]
fn install_mode_and_ownership_values_are_not_paths() {
    assert_eq!(
        attributed("install -o root -g wheel -m 755 build/app /usr/local/bin/app"),
        [
            (Read, "build/app".to_string()),
            (Write, "/usr/local/bin/app".to_string()),
        ]
    );

    let made = accesses("install -d build/logs");
    assert_eq!(made[0].op, Write);
    assert_eq!(made[0].target, Directory);
    assert_eq!(made[0].path, "build/logs");
}

/// `ack` and `ag` spell `-r` as recursion, taking no value, while ripgrep's
/// `-r` is `--replace` and takes one. Sharing ripgrep's declaration ate the
/// search pattern and left the command reporting nothing at all.
#[test]
fn ack_and_ag_do_not_borrow_ripgreps_replace_flag() {
    assert_eq!(attributed("ack -r needle src"), [(Read, "src".to_string())]);
    assert_eq!(attributed("ag -r needle src"), [(Read, "src".to_string())]);
    assert_eq!(
        attributed("rg -r replacement needle src"),
        [(Read, "src".to_string())]
    );
}

#[test]
fn grep_file_flag_is_a_read() {
    assert_eq!(
        attributed("grep -f patterns.txt src/a.txt"),
        [
            (Read, "patterns.txt".to_string()),
            (Read, "src/a.txt".to_string()),
        ]
    );
}

#[test]
fn cp_target_directory_flag_is_a_write() {
    // Slash-free, so the flag's own policy is what makes this a directory.
    let accesses = accesses("cp -t build config/base.yml");
    assert_eq!(accesses[0].op, Write);
    assert_eq!(accesses[0].target, Directory);
    assert_eq!(accesses[0].path, "build");
    assert_eq!(accesses[1].target, File);
    assert_eq!(accesses[1].path, "config/base.yml");
}

/// Every operand here is spelled as a bare name, so the utility's own operand
/// policy is the only thing that can be reporting a directory.
#[test]
fn directory_operands_are_not_files() {
    let listed = accesses("ls src");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].op, Read);
    assert_eq!(listed[0].target, Directory);
    assert_eq!(listed[0].path, "src");

    let created = accesses("mkdir build");
    assert_eq!(created[0].target, Directory);
    assert_eq!(created[0].op, Write);

    let removed = accesses("rmdir build");
    assert_eq!(removed[0].target, Directory);
    assert_eq!(removed[0].op, Delete);

    // A directory reader can still be told to write a file.
    let listing = accesses("tree -o build/tree.txt src");
    assert_eq!(listing[0].op, Write);
    assert_eq!(listing[0].target, File);
    assert_eq!(listing[0].path, "build/tree.txt");
    assert_eq!(listing[1].target, Directory);
    assert_eq!(listing[1].path, "src");
}

/// `find` walks roots and then evaluates an expression. The roots are
/// directories it read; the expression's words are tests and their arguments,
/// and `'*.rs'` is not a name however much it looks like one.
#[test]
fn find_reads_its_roots_and_attributes_nothing_in_its_expression() {
    let walked = accesses("find src tests -name '*.rs' -exec rm {} ;");
    assert_eq!(walked.len(), 2);
    assert_eq!(walked[0].path, "src");
    assert_eq!(walked[0].target, Directory);
    assert_eq!(walked[0].op, Read);
    assert_eq!(walked[1].path, "tests");
    assert_eq!(walked[1].target, Directory);

    // A leading option is declared, so it does not end the roots early.
    let followed = accesses("find -L src -type d");
    assert_eq!(followed.len(), 1);
    assert_eq!(followed[0].path, "src");
    assert_eq!(followed[0].target, Directory);

    // Grouping and negation have to be escaped to survive the shell, so they
    // arrive as ordinary words. Reading one as a root would name a directory no
    // command wrote.
    for grouped in [
        "find . \\( -name '*.rs' -o -name '*.md' \\)",
        "find . ! -name '*.rs'",
    ] {
        assert_eq!(attributed(grouped), [(Read, ".".to_string())], "{grouped}");
    }
}

#[test]
fn a_directory_spelling_outranks_the_utility_operand_shape() {
    let searched = accesses("rg TODO .");
    assert_eq!(searched.len(), 1);
    assert_eq!(searched[0].path, ".");
    assert_eq!(searched[0].target, Directory);
    assert_eq!(searched[0].op, Read);

    let recursed = accesses("grep -rn TODO src/");
    assert_eq!(recursed[0].path, "src/");
    assert_eq!(recursed[0].target, Directory);

    let copied = accesses("cp src/main.rs build/");
    assert_eq!(copied[1].path, "build/");
    assert_eq!(copied[1].target, Directory);
    assert_eq!(copied[1].op, Write);
    assert_eq!(copied[0].target, File);
}
