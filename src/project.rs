//! Typed roots that distinguish the three semantically distinct uses of a
//! filesystem path in this codebase: the project the agent edits, the
//! directory conversation artifacts live under, and the directories knowledge
//! is searched in.
//!
//! Construction goes through [`ProjectRoot::try_from`], [`ConversationRoot::try_from`],
//! or [`KnowledgeRoot::try_from`]. Each rejects non-directories. Conversation
//! and knowledge roots may also be derived from a [`ProjectRoot`] via [`From`]
//! when the caller wants the project root to play that role.

use vfs::VfsPath;

#[derive(Debug, thiserror::Error)]
pub enum RootError {
    #[error("root path {path:?} is not a directory")]
    NotDirectory { path: String },
    #[error("root path {path:?} could not be inspected")]
    Inspect {
        path: String,
        #[source]
        source: vfs::VfsError,
    },
}

#[derive(Debug, Clone)]
pub struct ProjectRoot(VfsPath);

impl ProjectRoot {
    pub fn try_from(path: VfsPath) -> Result<Self, RootError> {
        require_directory(&path)?;
        Ok(Self(path))
    }

    pub fn as_path(&self) -> &VfsPath {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct ConversationRoot(VfsPath);

impl ConversationRoot {
    pub fn try_from(path: VfsPath) -> Result<Self, RootError> {
        require_directory(&path)?;
        Ok(Self(path))
    }

    pub fn as_path(&self) -> &VfsPath {
        &self.0
    }

    /// Join a turn-file name onto this conversation root. Used by callers
    /// that previously synthesised paths by hand. Panics on a malformed
    /// `name` because turn names are produced by the runtime, not user
    /// input.
    pub fn turn_path(&self, name: &str) -> VfsPath {
        self.0
            .join(name)
            .expect("turn name joins onto conversation root")
    }
}

impl From<ProjectRoot> for ConversationRoot {
    fn from(project: ProjectRoot) -> Self {
        Self(project.0)
    }
}

#[derive(Debug, Clone)]
pub struct KnowledgeRoot(VfsPath);

impl KnowledgeRoot {
    pub fn try_from(path: VfsPath) -> Result<Self, RootError> {
        require_directory(&path)?;
        Ok(Self(path))
    }

    pub fn as_path(&self) -> &VfsPath {
        &self.0
    }
}

impl From<ProjectRoot> for KnowledgeRoot {
    fn from(project: ProjectRoot) -> Self {
        Self(project.0)
    }
}

#[derive(Debug, Clone)]
pub struct Project {
    pub root: ProjectRoot,
    pub conversations: ConversationRoot,
    pub knowledge: Vec<KnowledgeRoot>,
}

fn require_directory(path: &VfsPath) -> Result<(), RootError> {
    let is_dir = path.is_dir().map_err(|source| RootError::Inspect {
        path: path.as_str().to_string(),
        source,
    })?;
    if !is_dir {
        return Err(RootError::NotDirectory {
            path: path.as_str().to_string(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem_fs;

    #[test]
    fn project_root_rejects_non_directory() {
        let fs = mem_fs! {
            "project": {
                "AGENTS.md": "# project agents\n",
            },
        };
        let file_path = fs.join("project/AGENTS.md").unwrap();

        let err = ProjectRoot::try_from(file_path).unwrap_err();

        assert!(
            matches!(err, RootError::NotDirectory { .. }),
            "expected NotDirectory, got {err:?}"
        );
    }

    #[test]
    fn conversation_root_under_project_constructs() {
        let fs = mem_fs! {
            "project": {
                "AGENTS.md": "# agents\n",
            },
        };
        let project_path = fs.join("project").unwrap();
        let project = ProjectRoot::try_from(project_path).unwrap();

        let conversation = ConversationRoot::from(project.clone());

        assert_eq!(conversation.as_path().as_str(), project.as_path().as_str());
    }

    #[test]
    fn knowledge_root_from_path_constructs() {
        let fs = mem_fs! {
            "extra_kb": {
                "AGENTS.md": "# extra agents\n",
            },
        };
        let kb_path = fs.join("extra_kb").unwrap();

        let knowledge = KnowledgeRoot::try_from(kb_path.clone()).unwrap();

        assert_eq!(knowledge.as_path().as_str(), kb_path.as_str());
    }

    #[test]
    fn roots_first_knowledge_is_project() {
        let fs = mem_fs! {
            "project": { "AGENTS.md": "# agents\n" },
            "extra_kb": { "AGENTS.md": "# extra agents\n" },
        };
        let project_root = ProjectRoot::try_from(fs.join("project").unwrap()).unwrap();
        let extra = KnowledgeRoot::try_from(fs.join("extra_kb").unwrap()).unwrap();

        let project = Project {
            root: project_root.clone(),
            conversations: ConversationRoot::from(project_root.clone()),
            knowledge: vec![KnowledgeRoot::from(project_root.clone()), extra],
        };

        assert_eq!(project.knowledge.len(), 2);
        assert_eq!(
            project.knowledge[0].as_path().as_str(),
            project.root.as_path().as_str()
        );
    }
}
