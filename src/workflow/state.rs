use std::collections::{BTreeMap, VecDeque};

use serde::{Deserialize, Serialize};
use vfs::VfsPath;

use crate::workflow::schema::Workflow;

pub const WORKFLOW_STATE_FILENAME: &str = "workflow.state.toml";

#[derive(Debug, thiserror::Error)]
pub enum WorkflowStateError {
    #[error("resolving {path}")]
    ResolvePath {
        path: String,
        #[source]
        source: vfs::VfsError,
    },

    #[error("checking existence of {path}")]
    CheckExists {
        path: String,
        #[source]
        source: vfs::VfsError,
    },

    #[error("reading {path}")]
    Read {
        path: String,
        #[source]
        source: vfs::VfsError,
    },

    #[error("opening {path} for write")]
    OpenForWrite {
        path: String,
        #[source]
        source: vfs::VfsError,
    },

    #[error("writing {path}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("parsing {path}")]
    ParseToml {
        path: String,
        #[source]
        source: toml::de::Error,
    },

    #[error("serializing {path}")]
    SerializeToml {
        path: String,
        #[source]
        source: toml::ser::Error,
    },
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct ContextSeed {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub today: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_dir: Option<String>,
}

fn is_default_context_seed(seed: &ContextSeed) -> bool {
    seed.today.is_none() && seed.session_dir.is_none()
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct WorkflowState {
    pub workflow: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub inputs: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "is_default_context_seed")]
    pub context_seed: ContextSeed,
    #[serde(default)]
    pub queue: VecDeque<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_result: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub history: Vec<HistoryEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct HistoryEntry {
    pub task: String,
    pub turn: String,
    pub result: String,
}

impl WorkflowState {
    /// Build a fresh state with the start task queued.
    pub fn initial(workflow: &Workflow) -> Self {
        Self {
            workflow: workflow.name.clone(),
            inputs: BTreeMap::new(),
            context_seed: ContextSeed::default(),
            queue: VecDeque::from(vec![workflow.start.clone()]),
            last_result: None,
            history: Vec::new(),
        }
    }

    /// Read `workflow.state.toml` at `root`. Returns `Ok(None)` when the
    /// file is absent.
    pub fn read(root: &VfsPath) -> Result<Option<Self>, WorkflowStateError> {
        let path = root.join(WORKFLOW_STATE_FILENAME).map_err(|source| {
            WorkflowStateError::ResolvePath {
                path: root.as_str().to_string(),
                source,
            }
        })?;
        let exists = path
            .exists()
            .map_err(|source| WorkflowStateError::CheckExists {
                path: path.as_str().to_string(),
                source,
            })?;
        if !exists {
            return Ok(None);
        }
        let text = path
            .read_to_string()
            .map_err(|source| WorkflowStateError::Read {
                path: path.as_str().to_string(),
                source,
            })?;
        let state: WorkflowState =
            toml::from_str(&text).map_err(|source| WorkflowStateError::ParseToml {
                path: path.as_str().to_string(),
                source,
            })?;
        Ok(Some(state))
    }

    /// Serialize state and write it to `workflow.state.toml` at `root`.
    pub fn write(&self, root: &VfsPath) -> Result<(), WorkflowStateError> {
        let path = root.join(WORKFLOW_STATE_FILENAME).map_err(|source| {
            WorkflowStateError::ResolvePath {
                path: root.as_str().to_string(),
                source,
            }
        })?;
        let text = toml::to_string(self).map_err(|source| WorkflowStateError::SerializeToml {
            path: path.as_str().to_string(),
            source,
        })?;
        let mut writer = path
            .create_file()
            .map_err(|source| WorkflowStateError::OpenForWrite {
                path: path.as_str().to_string(),
                source,
            })?;
        std::io::Write::write_all(&mut writer, text.as_bytes()).map_err(|source| {
            WorkflowStateError::Write {
                path: path.as_str().to_string(),
                source,
            }
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem_fs;

    #[test]
    fn round_trips_state_with_history_and_empty_queue() {
        let original = WorkflowState {
            workflow: "basic".to_string(),
            inputs: BTreeMap::new(),
            context_seed: ContextSeed::default(),
            queue: VecDeque::new(),
            last_result: Some("end_turn".to_string()),
            history: vec![
                HistoryEntry {
                    task: "first".to_string(),
                    turn: "01_first.toml".to_string(),
                    result: "end_turn".to_string(),
                },
                HistoryEntry {
                    task: "second".to_string(),
                    turn: "02_second.toml".to_string(),
                    result: "end_turn".to_string(),
                },
            ],
        };

        let text = toml::to_string(&original).unwrap();
        let parsed: WorkflowState = toml::from_str(&text).unwrap();

        assert_eq!(parsed, original);
    }

    #[test]
    fn read_returns_none_when_state_file_absent() {
        let fs = mem_fs! { "root": {} };
        let root = fs.join("root").unwrap();

        let state = WorkflowState::read(&root).unwrap();

        assert!(state.is_none());
    }

    #[test]
    fn read_returns_some_state_when_file_present() {
        let fs = mem_fs! {
            "root": {
                "workflow.state.toml": "workflow = \"basic\"\nqueue = [\"second\"]\nlast_result = \"end_turn\"\n",
            },
        };
        let root = fs.join("root").unwrap();

        let state = WorkflowState::read(&root).unwrap().expect("state present");

        assert_eq!(state.workflow, "basic");
        assert_eq!(state.queue, VecDeque::from(vec!["second".to_string()]));
        assert_eq!(state.last_result.as_deref(), Some("end_turn"));
        assert!(state.history.is_empty());
    }

    #[test]
    fn write_then_read_round_trips_through_filesystem() {
        let fs = mem_fs! { "root": {} };
        let root = fs.join("root").unwrap();

        let mut state = WorkflowState {
            workflow: "basic".to_string(),
            inputs: BTreeMap::new(),
            context_seed: ContextSeed::default(),
            queue: VecDeque::new(),
            last_result: None,
            history: Vec::new(),
        };
        state.queue.push_back("first".to_string());
        state.write(&root).unwrap();

        let reloaded = WorkflowState::read(&root).unwrap().expect("written");
        assert_eq!(reloaded, state);
    }

    #[test]
    fn read_state_with_partial_fields_uses_defaults() {
        let fs = mem_fs! {
            "root": {
                "workflow.state.toml": "workflow = \"basic\"\n",
            },
        };
        let root = fs.join("root").unwrap();

        let state = WorkflowState::read(&root).unwrap().expect("state present");

        assert_eq!(state.workflow, "basic");
        assert!(state.queue.is_empty());
        assert!(state.last_result.is_none());
        assert!(state.history.is_empty());
    }
}
