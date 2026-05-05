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
    #[error(
        "workflow input {name:?} value {value:?} does not match required pattern {pattern:?}"
    )]
    InputPatternMismatch {
        name: String,
        value: String,
        pattern: String,
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
    pub start: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, InputSpec>,
    pub tasks: Vec<Task>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Task {
    pub name: String,
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
