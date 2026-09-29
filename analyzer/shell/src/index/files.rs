//! Attributes file accesses once during indexing. Each access preserves whether
//! it came from a harness field or command analysis; shell operands are not
//! resolved against the filesystem.

use crate::model::{Event, FileReference, SourceValue, ToolCall};
use shell_access::{AccessTarget, Classifier};

/// A path the harness named in a dedicated field.
const TOOL: &str = "tool";
/// A path read out of recorded command text.
const SHELL: &str = "shell";

/// Appends dedicated-field and command-derived accesses to existing recorded
/// accesses. Parse errors discard only the failing command fragment.
pub fn attributed_files(event: &Event) -> SourceValue<Vec<FileReference>> {
    let mut files = match &event.files {
        SourceValue::Recorded(recorded) => recorded.clone(),
        _ => Vec::new(),
    };
    if let SourceValue::Recorded(tool) = &event.tool_call {
        files.extend(from_dedicated_field(tool));
        files.extend(from_recorded_command(tool));
    }
    if files.is_empty() {
        SourceValue::Absent
    } else {
        SourceValue::Recorded(files)
    }
}

fn from_dedicated_field(tool: &ToolCall) -> Option<FileReference> {
    let SourceValue::Recorded(path) = &tool.path else {
        return None;
    };
    Some(FileReference {
        path: path.clone(),
        target: SourceValue::Recorded(AccessTarget::of_path(path).as_str().to_string()),
        operation: tool_operation(&tool.name),
        provenance: SourceValue::Recorded(TOOL.to_string()),
        ambiguity: SourceValue::Absent,
        cwd: match &tool.cwd {
            SourceValue::Recorded(cwd) => SourceValue::Recorded(cwd.clone()),
            _ => SourceValue::Absent,
        },
    })
}

fn from_recorded_command(tool: &ToolCall) -> Vec<FileReference> {
    let SourceValue::Recorded(command) = &tool.command else {
        return Vec::new();
    };
    let mut classifier = Classifier::POSIX;
    if let SourceValue::Recorded(cwd) = &tool.cwd {
        classifier = classifier.with_cwd(cwd);
    }
    classifier
        .classify(command)
        .flatten()
        .map(|access| FileReference {
            path: access.path,
            target: SourceValue::Recorded(access.target.as_str().to_string()),
            operation: SourceValue::Recorded(access.op.as_str().to_string()),
            provenance: SourceValue::Recorded(SHELL.to_string()),
            ambiguity: match access.ambiguity {
                Some(reason) => SourceValue::Recorded(reason.description().to_string()),
                None => SourceValue::Absent,
            },
            cwd: match access.cwd {
                Some(cwd) => SourceValue::Recorded(cwd.display().to_string()),
                None => SourceValue::Absent,
            },
        })
        .collect()
}

/// Which operation a dedicated file tool performs, from an additive table of
/// the names the three harnesses use. A name this does not know leaves the
/// operation unrecorded rather than guessed: the path is still evidence, and
/// "we do not know what it did with it" is the honest report.
fn tool_operation(name: &str) -> SourceValue<String> {
    let operation = match name {
        "Read" | "Glob" | "Grep" | "NotebookRead" | "read_file" | "read" => "read",
        "Write" | "Edit" | "MultiEdit" | "NotebookEdit" | "apply_patch" | "write" | "edit" => {
            "write"
        }
        _ => return SourceValue::Absent,
    };
    SourceValue::Recorded(operation.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{EventKind, Harness, Provenance};

    fn tool_call(name: &str, fields: impl FnOnce(&mut ToolCall)) -> Event {
        let mut tool = ToolCall {
            name: name.to_string(),
            call_id: SourceValue::Absent,
            input: SourceValue::Absent,
            command: SourceValue::Absent,
            path: SourceValue::Absent,
            url: SourceValue::Absent,
            cwd: SourceValue::Absent,
        };
        fields(&mut tool);
        Event {
            id: "evt".to_string(),
            session_id: "session".to_string(),
            kind: EventKind::ToolCall,
            source: Provenance {
                harness: Harness::ClaudeCode,
                path: "/p/session.jsonl".to_string(),
                line: 1,
                ordinal: 1,
            },
            native_id: SourceValue::Absent,
            response_id: SourceValue::Absent,
            model: SourceValue::Absent,
            timestamp: SourceValue::Absent,
            turn: SourceValue::Absent,
            tool_call: SourceValue::Recorded(tool),
            tool_result: SourceValue::Absent,
            token_usage: SourceValue::Absent,
            files: SourceValue::Absent,
            detail: SourceValue::Absent,
            subagent: SourceValue::Absent,
        }
    }

    fn files(event: &Event) -> Vec<FileReference> {
        match attributed_files(event) {
            SourceValue::Recorded(files) => files,
            other => panic!("expected recorded files, got {other:?}"),
        }
    }

    fn shell(path: &str, operation: &str) -> FileReference {
        FileReference {
            path: path.to_string(),
            target: SourceValue::Recorded("file".to_string()),
            operation: SourceValue::Recorded(operation.to_string()),
            provenance: SourceValue::Recorded(SHELL.to_string()),
            ambiguity: SourceValue::Absent,
            cwd: SourceValue::Absent,
        }
    }

    fn shell_at(path: &str, operation: &str, cwd: &str) -> FileReference {
        FileReference {
            path: path.to_string(),
            target: SourceValue::Recorded("file".to_string()),
            operation: SourceValue::Recorded(operation.to_string()),
            provenance: SourceValue::Recorded(SHELL.to_string()),
            ambiguity: SourceValue::Absent,
            cwd: SourceValue::Recorded(cwd.to_string()),
        }
    }

    #[test]
    fn an_indexed_shell_command_keeps_its_evidence_provenance() {
        let event = tool_call("Bash", |tool| {
            tool.command = SourceValue::Recorded("cat config/base.yml".to_string());
        });

        assert_eq!(files(&event), [shell("config/base.yml", "read")]);
    }

    #[test]
    fn a_dedicated_path_field_stays_tool_provenance() {
        let event = tool_call("Read", |tool| {
            tool.path = SourceValue::Recorded("packages/auth/src/session.ts".to_string());
        });

        assert_eq!(
            files(&event),
            [FileReference {
                path: "packages/auth/src/session.ts".to_string(),
                target: SourceValue::Recorded("file".to_string()),
                operation: SourceValue::Recorded("read".to_string()),
                provenance: SourceValue::Recorded(TOOL.to_string()),
                ambiguity: SourceValue::Absent,
                cwd: SourceValue::Absent,
            }]
        );
    }

    /// A recorded directory is context. It must not turn a relative operand
    /// into an absolute claim about someone's disk.
    #[test]
    fn a_recorded_directory_leaves_a_relative_operand_relative() {
        let event = tool_call("Bash", |tool| {
            tool.command = SourceValue::Recorded("cat config/base.yml".to_string());
            tool.cwd = SourceValue::Recorded("/work/app".to_string());
        });

        assert_eq!(files(&event)[0].path, "config/base.yml");
        assert_eq!(
            files(&event)[0],
            shell_at("config/base.yml", "read", "/work/app")
        );
    }

    #[test]
    fn an_unresolved_operand_records_why_it_stayed_a_fragment() {
        let event = tool_call("Bash", |tool| {
            tool.command = SourceValue::Recorded("cat logs/*.txt".to_string());
        });

        let files = files(&event);
        assert_eq!(files[0].path, "logs/*.txt");
        assert_eq!(
            files[0].ambiguity,
            SourceValue::Recorded("glob not expanded".to_string())
        );
    }

    /// Both sources are appended, so a call that names a path and runs a command
    /// reports both rather than one shadowing the other.
    #[test]
    fn a_call_with_both_a_path_and_a_command_reports_both() {
        let event = tool_call("Bash", |tool| {
            tool.path = SourceValue::Recorded("build/out.env".to_string());
            tool.command = SourceValue::Recorded("rm -f build/stale.env".to_string());
        });

        let files = files(&event);
        assert_eq!(files[0].provenance, SourceValue::Recorded(TOOL.to_string()));
        assert_eq!(files[0].operation, SourceValue::Absent);
        assert_eq!(files[1], shell("build/stale.env", "delete"));
    }

    /// Calls with no recorded command or path contribute no access.
    #[test]
    fn a_call_that_recorded_no_command_and_no_path_contributes_nothing() {
        let event = tool_call("exec", |tool| {
            tool.input =
                SourceValue::Recorded("await tools.exec_command({cmd: \"ls\"})".to_string());
        });

        assert_eq!(attributed_files(&event), SourceValue::Absent);
    }

    #[test]
    fn an_unparsable_command_contributes_nothing() {
        let event = tool_call("Bash", |tool| {
            tool.command = SourceValue::Recorded("cat 'config/base.yml".to_string());
        });

        assert_eq!(attributed_files(&event), SourceValue::Absent);
    }

    #[test]
    fn a_compound_parse_failure_keeps_accesses_from_the_prefix() {
        let event = tool_call("Bash", |tool| {
            tool.command =
                SourceValue::Recorded("cat config/base.yml && cat 'unterminated".to_string());
        });

        assert_eq!(files(&event), [shell("config/base.yml", "read")]);
    }

    #[test]
    fn a_shell_directory_is_not_indexed_as_a_file() {
        let event = tool_call("Bash", |tool| {
            tool.command = SourceValue::Recorded("ls .".to_string());
        });

        let access = &files(&event)[0];
        assert_eq!(access.path, ".");
        assert_eq!(
            access.target,
            SourceValue::Recorded("directory".to_string())
        );
    }

    #[test]
    fn a_tool_path_that_spells_a_directory_is_indexed_as_one() {
        let event = tool_call("Grep", |tool| {
            tool.path = SourceValue::Recorded("packages/auth/".to_string());
        });

        let access = &files(&event)[0];
        assert_eq!(access.path, "packages/auth/");
        assert_eq!(
            access.target,
            SourceValue::Recorded("directory".to_string())
        );
    }
}
