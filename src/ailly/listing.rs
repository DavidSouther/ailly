//! Output formatting helpers for `--list-workflows`, `--list-skills`,
//! and `--list-tools`. Discovery lives on `Project`; this module renders
//! the value objects to byte-stable strings the e2e tests grep.

use crate::project::{SkillEntry, ToolEntry, WorkflowEntry};

const NO_DESCRIPTION: &str = "(no description)";

/// Render the `--list-workflows` body. Two-column with the source path
/// on a continuation line indented two spaces. Trailing hint line names
/// the runner verb. Empty case prints the documented hint.
pub fn render_workflow_listing(entries: &[WorkflowEntry]) -> String {
    if entries.is_empty() {
        return "Available workflows: (none found)\nAdd a `workflow.toml` to the project root or a `workflows/<name>.toml` under a knowledge root.\n".to_string();
    }
    let mut out = String::from("Available workflows:\n");
    let gutter = name_gutter(entries.iter().map(|e| e.name().as_str()));
    for entry in entries {
        push_row(&mut out, entry.name().as_str(), entry.description(), gutter);
        out.push_str(&format!("    {}\n", entry.source_path()));
    }
    out.push_str("Run a workflow with `ailly -w <NAME>`.\n");
    out
}

/// Render the `-w UNKNOWN` preface that prefixes the workflow listing.
/// Goes to stderr; the listing body goes to stdout.
pub fn render_unknown_workflow_preface(unknown: &str, searched: &[String]) -> String {
    let mut out = format!("Unknown workflow: {unknown}\nSearched:\n");
    for path in searched {
        out.push_str(&format!("  {path}\n"));
    }
    out
}

/// Render the `--list-skills` body. Same two-column shape as workflows.
pub fn render_skill_listing(entries: &[SkillEntry]) -> String {
    if entries.is_empty() {
        return "Available skills: (none found)\nAdd a `skills/<name>/SKILL.md` under the project or a knowledge root.\n".to_string();
    }
    let mut out = String::from("Available skills:\n");
    let gutter = name_gutter(entries.iter().map(|e| e.name.as_str()));
    for entry in entries {
        push_row(&mut out, &entry.name, &entry.description, gutter);
        out.push_str(&format!("    {}\n", entry.source_path));
    }
    out
}

/// Render the `--list-tools` body. Tools have no source line.
pub fn render_tool_listing(entries: &[ToolEntry]) -> String {
    if entries.is_empty() {
        return "Registered tools: (none)\n".to_string();
    }
    let mut out = String::from("Registered tools:\n");
    let gutter = name_gutter(entries.iter().map(|e| e.name.as_str()));
    for entry in entries {
        push_row(&mut out, &entry.name, &entry.description, gutter);
    }
    out
}

fn push_row(out: &mut String, name: &str, description: &str, gutter: usize) {
    let desc = if description.is_empty() {
        NO_DESCRIPTION
    } else {
        description
    };
    let pad = " ".repeat(gutter - name.len());
    out.push_str(&format!("  {name}{pad}  {desc}\n"));
}

fn name_gutter<'a>(names: impl Iterator<Item = &'a str>) -> usize {
    names.map(str::len).max().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_workflow_listing_matches_two_column_fixture() {
        let entries = vec![
            WorkflowEntry::new(
                "build",
                "build the project".to_string(),
                "workflow.toml".to_string(),
            ),
            WorkflowEntry::new(
                "deploy".to_string(),
                "deploy to prod".to_string(),
                "workflows/deploy.toml".to_string(),
            ),
        ];

        let out = render_workflow_listing(&entries);

        assert_eq!(
            out,
            "Available workflows:\n  build   build the project\n    workflow.toml\n  deploy  deploy to prod\n    workflows/deploy.toml\nRun a workflow with `ailly -w <NAME>`.\n"
        );
    }

    #[test]
    fn render_workflow_listing_empty_case_matches_fixture() {
        let out = render_workflow_listing(&[]);

        assert_eq!(
            out,
            "Available workflows: (none found)\nAdd a `workflow.toml` to the project root or a `workflows/<name>.toml` under a knowledge root.\n"
        );
    }

    #[test]
    fn render_unknown_workflow_preface_names_unknown_and_searched_paths() {
        let preface = render_unknown_workflow_preface(
            "blueprint",
            &[
                "/x/project/workflow.toml".to_string(),
                "/x/project/workflows".to_string(),
            ],
        );

        assert_eq!(
            preface,
            "Unknown workflow: blueprint\nSearched:\n  /x/project/workflow.toml\n  /x/project/workflows\n"
        );
    }

    #[test]
    fn render_skill_listing_matches_two_column_fixture() {
        let entries = vec![
            SkillEntry {
                name: "greet".to_string(),
                description: "greet the user".to_string(),
                source_path: "skills/greet/SKILL.md".to_string(),
            },
            SkillEntry {
                name: "summarise".to_string(),
                description: "summarise text".to_string(),
                source_path: "skills/summarise/SKILL.md".to_string(),
            },
        ];

        let out = render_skill_listing(&entries);

        assert_eq!(
            out,
            "Available skills:\n  greet      greet the user\n    skills/greet/SKILL.md\n  summarise  summarise text\n    skills/summarise/SKILL.md\n"
        );
    }

    #[test]
    fn render_skill_listing_empty_case_matches_two_line_fixture() {
        let out = render_skill_listing(&[]);

        assert_eq!(
            out,
            "Available skills: (none found)\nAdd a `skills/<name>/SKILL.md` under the project or a knowledge root.\n"
        );
    }

    #[test]
    fn render_tool_listing_matches_fixture() {
        let entries = vec![
            ToolEntry {
                name: "bash".to_string(),
                description: "run a bash command".to_string(),
            },
            ToolEntry {
                name: "fs.read".to_string(),
                description: "read a file".to_string(),
            },
        ];

        let out = render_tool_listing(&entries);

        assert_eq!(
            out,
            "Registered tools:\n  bash     run a bash command\n  fs.read  read a file\n"
        );
    }
}
