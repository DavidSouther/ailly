use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum WorkflowError {
    #[error("workflow {workflow:?} has no task named {name:?}")]
    UnknownTask { workflow: String, name: String },
    #[error("task {task:?} references unresolved template placeholder {placeholder:?}")]
    UnresolvedTemplate { task: String, placeholder: String },
    #[error("workflow input {name:?} is required but the harness supplied no value")]
    MissingInput { name: String },
    #[error("workflow input {name:?} value {value:?} does not match required pattern {pattern:?}")]
    InputPatternMismatch {
        name: String,
        value: String,
        pattern: String,
    },
    #[error(
        "task {task:?} uses TaskAction::ToolCall, which is not yet supported as a `task` action (only as `evaluation`)"
    )]
    ToolCallTaskNotImplemented { task: String },
    #[error("task {task:?} evaluation references unknown tool {tool:?}")]
    UnknownEvalTool { task: String, tool: String },
    #[error("task {task:?} evaluation tool {tool:?} could not encode args as JSON: {source}")]
    EvalArgsEncode {
        task: String,
        tool: String,
        #[source]
        source: serde_json::Error,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InputSpec {
    pub description: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Workflow {
    pub name: String,
    /// Optional human-readable description rendered by `--list-workflows`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub start: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, InputSpec>,
    pub tasks: Vec<Task>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Task {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skills: Vec<String>,
    pub task: TaskAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evaluation: Option<TaskAction>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub next: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskAction {
    Prompt { text: String },
    ToolCall { tool: String, args: toml::Value },
}

impl Workflow {
    /// Look up a task by name. Returns an error when no task matches.
    pub fn find_task(&self, name: &str) -> Result<&Task, WorkflowError> {
        // Tasks are small so this is cheap. As workflows grow, this may need to become a Map.
        self.tasks
            .iter()
            .find(|t| t.name == name)
            .ok_or_else(|| WorkflowError::UnknownTask {
                workflow: self.name.clone(),
                name: name.to_string(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_skills_round_trip_preserves_declared_order() {
        let toml_in = r#"name = "design"
skills = ["developer:design", "developer:thinking"]

[task]
kind = "prompt"
text = "Produce design.md."
"#;
        let task: Task = toml::from_str(toml_in).expect("parse");
        assert_eq!(
            task.skills,
            vec![
                "developer:design".to_string(),
                "developer:thinking".to_string(),
            ]
        );

        let serialized = toml::to_string(&task).expect("serialize");
        let reparsed: Task = toml::from_str(&serialized).expect("reparse");
        assert_eq!(reparsed.skills, task.skills);
        let dev_design_idx = serialized
            .find("developer:design")
            .expect("developer:design present");
        let dev_thinking_idx = serialized
            .find("developer:thinking")
            .expect("developer:thinking present");
        assert!(
            dev_design_idx < dev_thinking_idx,
            "skills must serialize in declared order: {serialized}"
        );
    }

    #[test]
    fn task_omits_empty_skills_field_in_serialized_form() {
        let task = Task {
            name: "bare".to_string(),
            skills: Vec::new(),
            task: TaskAction::Prompt {
                text: "Run.".to_string(),
            },
            evaluation: None,
            next: BTreeMap::new(),
        };

        let serialized = toml::to_string(&task).expect("serialize");
        assert!(
            !serialized.contains("skills ="),
            "empty skills must be omitted: {serialized}"
        );
    }

    #[test]
    fn task_evaluation_tool_call_round_trips_through_toml() {
        let toml_in = r#"name = "design"

[task]
kind = "prompt"
text = "Produce design.md."

[evaluation]
kind = "tool_call"
tool = "fs.absent"

[evaluation.args]
path = "design.md"
needle = "*Draft"
"#;
        let task: Task = toml::from_str(toml_in).expect("parse");
        match task.evaluation.as_ref().expect("evaluation present") {
            TaskAction::ToolCall { tool, args } => {
                assert_eq!(tool, "fs.absent");
                let table = args.as_table().expect("args is a table");
                assert_eq!(
                    table.get("path").and_then(|v| v.as_str()),
                    Some("design.md")
                );
                assert_eq!(table.get("needle").and_then(|v| v.as_str()), Some("*Draft"));
            }
            other => panic!("expected ToolCall, got {other:?}"),
        }

        let serialized = toml::to_string(&task).expect("serialize");
        let reparsed: Task = toml::from_str(&serialized).expect("reparse");
        match reparsed.evaluation.as_ref().expect("evaluation present") {
            TaskAction::ToolCall { tool, args } => {
                assert_eq!(tool, "fs.absent");
                let table = args.as_table().expect("args is a table");
                assert_eq!(
                    table.get("path").and_then(|v| v.as_str()),
                    Some("design.md")
                );
                assert_eq!(table.get("needle").and_then(|v| v.as_str()), Some("*Draft"));
            }
            other => panic!("expected ToolCall, got {other:?}"),
        }
    }

    #[test]
    fn workflow_omits_description_when_none_in_serialized_form() {
        let workflow = Workflow {
            name: "bare".to_string(),
            description: None,
            start: "go".to_string(),
            inputs: BTreeMap::new(),
            tasks: vec![Task {
                name: "go".to_string(),
                skills: Vec::new(),
                task: TaskAction::Prompt {
                    text: "Run.".to_string(),
                },
                evaluation: None,
                next: BTreeMap::new(),
            }],
        };

        let serialized = toml::to_string(&workflow).expect("serialize");

        assert!(
            !serialized.contains("description"),
            "absent description must be omitted: {serialized}"
        );
    }

    #[test]
    fn workflow_round_trips_description_when_present() {
        let toml_in = r#"name = "demo"
description = "A demo workflow."
start = "go"

[[tasks]]
name = "go"
[tasks.task]
kind = "prompt"
text = "Run."
"#;

        let workflow: Workflow = toml::from_str(toml_in).expect("parse");
        assert_eq!(workflow.description.as_deref(), Some("A demo workflow."));

        let serialized = toml::to_string(&workflow).expect("serialize");
        let reparsed: Workflow = toml::from_str(&serialized).expect("reparse");
        assert_eq!(reparsed.description.as_deref(), Some("A demo workflow."));
    }

    #[test]
    fn task_defaults_skills_to_empty_when_absent_from_toml() {
        let toml_in = r#"name = "bare"

[task]
kind = "prompt"
text = "No skills."
"#;
        let task: Task = toml::from_str(toml_in).expect("parse");
        assert!(task.skills.is_empty());
    }
}
