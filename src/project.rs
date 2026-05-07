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

use crate::content::AILLY_DIR;
use crate::knowledge::base::WorkflowName;

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
    #[error("could not create directory {path:?}")]
    CreateDirectory {
        path: String,
        #[source]
        source: vfs::VfsError,
    },
    #[error("could not resolve {path:?}")]
    ResolvePath {
        path: String,
        #[source]
        source: vfs::VfsError,
    },
}

#[derive(Debug, Clone)]
pub struct ProjectRoot {
    path: VfsPath,
    display: String,
}

impl ProjectRoot {
    pub fn try_from(path: VfsPath) -> Result<Self, RootError> {
        require_directory(&path)?;
        let display = path.as_str().to_string();
        Ok(Self { path, display })
    }

    /// Construct a [`ProjectRoot`] backed by [`vfs::PhysicalFS`] rooted at
    /// `raw`. The native filesystem path is captured separately for display.
    pub fn from_physical(raw: &std::path::Path) -> Result<Self, RootError> {
        use vfs::{PhysicalFS, VfsPath};
        let vfs_path = VfsPath::new(PhysicalFS::new(raw));
        require_directory(&vfs_path)?;
        Ok(Self {
            path: vfs_path,
            display: raw.display().to_string(),
        })
    }

    pub fn as_path(&self) -> &VfsPath {
        &self.path
    }

    pub fn display_path(&self) -> &str {
        &self.display
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

    /// Resolve the conversation root for a workflow run: `<project>/.ailly`.
    /// The directory is created when absent so the runtime can immediately
    /// write turn files and `workflow.state.toml`. The project root is left
    /// in place; only the conversation artifacts move under `.ailly`.
    pub fn workflow_subdir(project: &ProjectRoot) -> Result<Self, RootError> {
        let joined =
            project
                .as_path()
                .join(AILLY_DIR)
                .map_err(|source| RootError::ResolvePath {
                    path: format!("{}/{}", project.as_path().as_str(), AILLY_DIR),
                    source,
                })?;
        let exists = joined.exists().map_err(|source| RootError::Inspect {
            path: joined.as_str().to_string(),
            source,
        })?;
        if !exists {
            joined
                .create_dir()
                .map_err(|source| RootError::CreateDirectory {
                    path: joined.as_str().to_string(),
                    source,
                })?;
        }
        Self::try_from(joined)
    }
}

impl From<ProjectRoot> for ConversationRoot {
    fn from(project: ProjectRoot) -> Self {
        Self(project.path)
    }
}

#[derive(Debug, Clone)]
pub struct KnowledgeRoot {
    path: VfsPath,
    display: String,
}

impl KnowledgeRoot {
    pub fn try_from(path: VfsPath) -> Result<Self, RootError> {
        require_directory(&path)?;
        let display = path.as_str().to_string();
        Ok(Self { path, display })
    }

    /// Construct a [`KnowledgeRoot`] backed by [`vfs::PhysicalFS`] rooted
    /// at `raw`. The native filesystem path is captured separately for
    /// display so error messages and listings see a meaningful path even
    /// though `VfsPath::as_str()` of a `PhysicalFS` root is empty.
    pub fn from_physical(raw: &std::path::Path) -> Result<Self, RootError> {
        use vfs::{PhysicalFS, VfsPath};
        let vfs_path = VfsPath::new(PhysicalFS::new(raw));
        require_directory(&vfs_path)?;
        Ok(Self {
            path: vfs_path,
            display: raw.display().to_string(),
        })
    }

    pub fn as_path(&self) -> &VfsPath {
        &self.path
    }

    pub fn display_path(&self) -> &str {
        &self.display
    }
}

impl std::fmt::Display for KnowledgeRoot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display)
    }
}

impl From<ProjectRoot> for KnowledgeRoot {
    fn from(project: ProjectRoot) -> Self {
        Self {
            path: project.path,
            display: project.display,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Project {
    pub root: ProjectRoot,
    pub conversations: ConversationRoot,
    pub knowledge: Vec<KnowledgeRoot>,
    /// Native filesystem path used as `Bash`'s working directory. Sourced
    /// from `cli.root()` at construction time and held as a `PathBuf`
    /// because `Bash` shells out and cannot use `VfsPath`.
    pub bash_cwd: std::path::PathBuf,
}

impl Project {
    /// Build the workflow listing for this project. Each knowledge root
    /// contributes a bare `workflow.toml` (if present) and every entry
    /// under `workflows/`. The first knowledge root is the project root,
    /// so a project-root `workflow.toml` takes precedence. Same-name
    /// entries are deduped first-wins. Sorted alphabetically by `name`.
    /// Per-file parse failures log one warning line to stderr and the
    /// entry is skipped.
    pub fn list_workflows(&self) -> Result<Vec<WorkflowEntry>, ListingError> {
        let mut entries: Vec<WorkflowEntry> = Vec::new();

        for root in &self.knowledge {
            let bare_path = root
                .as_path()
                .join(crate::knowledge::base::ROOT_WORKFLOW_FILE)
                .map_err(|source| ListingError::Vfs {
                    path: root.as_path().as_str().to_string(),
                    source,
                })?;
            let bare_exists = bare_path.exists().map_err(|source| ListingError::Vfs {
                path: bare_path.as_str().to_string(),
                source,
            })?;
            if bare_exists {
                match read_workflow(&bare_path) {
                    Ok(wf) => {
                        let name: WorkflowName = wf.name.clone().into();
                        if !entries.iter().any(|e| e.name == name) {
                            entries.push(WorkflowEntry {
                                name,
                                description: wf.description.unwrap_or_default(),
                                source_path: crate::knowledge::base::ROOT_WORKFLOW_FILE.to_string(),
                            });
                        }
                    }
                    Err(err) => {
                        eprintln!("warning: skipping {}: {err}", bare_path.as_str());
                    }
                }
            }

            for entry in crate::knowledge::base::iter_workflow_files(root)? {
                let (name, path) = entry?;
                if entries.iter().any(|e| e.name == name) {
                    continue;
                }
                match read_workflow(&path) {
                    Ok(wf) => entries.push(WorkflowEntry {
                        name: wf.name.into(),
                        description: wf.description.unwrap_or_default(),
                        source_path: format!("workflows/{}.toml", name.as_str()),
                    }),
                    Err(err) => {
                        eprintln!("warning: skipping {}: {err}", path.as_str());
                    }
                }
            }
        }

        entries.sort_by(|a, b| a.name().as_str().cmp(&b.name().as_str()));
        Ok(entries)
    }

    /// Build the skill listing for this project. Iterates `iter_skill_dirs`
    /// across every knowledge root, dedupes first-wins by name, sorts
    /// alphabetically. Per-skill parse failures log one warning line to
    /// stderr and the entry is skipped.
    pub fn list_skills(&self) -> Result<Vec<SkillEntry>, ListingError> {
        use crate::knowledge::base::KnowledgeSource;
        use crate::knowledge::skills::Skill;

        let mut entries: Vec<SkillEntry> = Vec::new();
        for root in &self.knowledge {
            for entry in crate::knowledge::base::iter_skill_dirs(root)? {
                let (name, dir) = entry?;
                if entries.iter().any(|e| e.name == name.as_str()) {
                    continue;
                }
                let skill_md = dir.join("SKILL.md").map_err(|source| ListingError::Vfs {
                    path: dir.as_str().to_string(),
                    source,
                })?;
                let exists = skill_md.exists().map_err(|source| ListingError::Vfs {
                    path: skill_md.as_str().to_string(),
                    source,
                })?;
                if !exists {
                    continue;
                }
                let raw = match skill_md.read_to_string() {
                    Ok(r) => r,
                    Err(err) => {
                        eprintln!("warning: skipping {}: {err}", skill_md.as_str());
                        continue;
                    }
                };
                let source = KnowledgeSource {
                    root: root.clone(),
                    path: skill_md.clone(),
                };
                match Skill::parse(source, &raw, &name) {
                    Ok(skill) => entries.push(SkillEntry {
                        name: skill.name().as_str().to_string(),
                        description: skill.description().as_str().to_string(),
                        source_path: format!("skills/{}/SKILL.md", name.as_str()),
                    }),
                    Err(err) => {
                        eprintln!("warning: skipping {}: {err}", skill_md.as_str());
                    }
                }
            }
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    /// Build the tool listing. Calls `tool_registry()`, sorts the names
    /// for stable output, then awaits each `ToolDyn::definition` to
    /// resolve the description.
    pub async fn list_tools(&self) -> Vec<ToolEntry> {
        use crate::engine::ToolRegistry;

        let registry = self.tool_registry();
        let mut names: Vec<String> = registry.names().into_iter().map(String::from).collect();
        names.sort();

        let mut entries = Vec::with_capacity(names.len());
        for name in names {
            let Some(tool) = registry.resolve(&name) else {
                continue;
            };
            let def = tool.definition(String::new()).await;
            entries.push(ToolEntry {
                name,
                description: def.description,
            });
        }
        entries
    }

    /// Returns the source paths the workflow listing consulted, used by
    /// the unknown-name preface. For each knowledge root, lists the bare
    /// `workflow.toml` and the `workflows/` directory in that order.
    pub fn workflow_search_paths(&self) -> Vec<String> {
        let mut paths = Vec::with_capacity(2 * self.knowledge.len());
        for root in &self.knowledge {
            if let Ok(p) = root
                .as_path()
                .join(crate::knowledge::base::ROOT_WORKFLOW_FILE)
            {
                paths.push(p.as_str().to_string());
            }
            if let Ok(p) = root.as_path().join("workflows") {
                paths.push(p.as_str().to_string());
            }
        }
        paths
    }

    /// Assemble the tool registry the workflow runtime registers today.
    /// Single source of truth for "what tools does Ailly carry?". Used
    /// by both the runtime and the listing surface so the two cannot drift.
    pub fn tool_registry(&self) -> crate::engine::HashMapRegistry {
        use std::sync::Arc;
        let mut registry = crate::engine::HashMapRegistry::default();
        registry.insert(
            crate::knowledge::tools::FsAbsent::NAME,
            Arc::new(crate::knowledge::tools::FsAbsent::new(&self.conversations)),
        );
        registry.insert(
            crate::knowledge::clarify::ClarifyTool::NAME,
            Arc::new(crate::knowledge::clarify::ClarifyTool::new(Arc::new(
                crate::ailly::knowledge_base::StdinKnowledgeBase::new(),
            ))),
        );
        registry.insert(
            crate::knowledge::tools::FsList::NAME,
            Arc::new(crate::knowledge::tools::FsList::new(&self.root)),
        );
        registry.insert(
            crate::knowledge::tools::FsEdit::NAME,
            Arc::new(crate::knowledge::tools::FsEdit::new(&self.root)),
        );
        registry.insert(
            crate::knowledge::tools::FsGrep::NAME,
            Arc::new(crate::knowledge::tools::FsGrep::new(&self.root)),
        );
        registry.insert(
            crate::knowledge::tools::Bash::NAME,
            Arc::new(crate::knowledge::tools::Bash::new(self.bash_cwd.clone())),
        );
        registry
    }
}

/// A row in the `--list-workflows` output. `source_path` is rendered
/// either as the literal `workflow.toml` for the conversation-root entry
/// or as `workflows/<name>.toml` for entries discovered under a knowledge
/// root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowEntry {
    name: WorkflowName,
    description: String,
    source_path: String,
}

impl WorkflowEntry {
    pub fn new(name: impl Into<WorkflowName>, description: String, source_path: String) -> Self {
        WorkflowEntry {
            name: name.into(),
            description,
            source_path,
        }
    }

    pub fn name(&self) -> &WorkflowName {
        &self.name
    }

    pub fn description(&self) -> &str {
        self.description.as_str()
    }

    pub fn source_path(&self) -> &str {
        self.source_path.as_str()
    }
}

/// A row in the `--list-skills` output. `source_path` is rendered as
/// `skills/<name>/SKILL.md`, knowledge-root-relative.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillEntry {
    pub name: String,
    pub description: String,
    pub source_path: String,
}

/// A row in the `--list-tools` output. Tools have no on-disk source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolEntry {
    pub name: String,
    pub description: String,
}

fn read_workflow(path: &VfsPath) -> Result<crate::workflow::Workflow, anyhow::Error> {
    let raw = path.read_to_string()?;
    let wf: crate::workflow::Workflow = toml::from_str(&raw)?;
    Ok(wf)
}

/// Hard failures during listing. Per-entry parse failures are not reported
/// here; they are logged to stderr and the entry is skipped.
#[derive(Debug, thiserror::Error)]
pub enum ListingError {
    #[error("knowledge base error: {0}")]
    Knowledge(#[from] crate::knowledge::base::KnowledgeError),
    #[error("filesystem error reading {path}: {source}")]
    Vfs {
        path: String,
        #[source]
        source: vfs::VfsError,
    },
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
    fn workflow_subdir_creates_dot_ailly_under_project() {
        let fs = mem_fs! {
            "project": {
                "AGENTS.md": "# agents\n",
            },
        };
        let project = ProjectRoot::try_from(fs.join("project").unwrap()).unwrap();

        let conversation = ConversationRoot::workflow_subdir(&project).unwrap();

        assert!(
            conversation.as_path().as_str().ends_with("/.ailly"),
            "expected leaf to be .ailly, got {:?}",
            conversation.as_path().as_str()
        );
        assert!(
            conversation.as_path().is_dir().unwrap(),
            ".ailly subdirectory should exist after workflow_subdir"
        );
    }

    #[test]
    fn workflow_subdir_is_idempotent_when_dot_ailly_already_exists() {
        let fs = mem_fs! {
            "project": {
                ".ailly": {
                    "01_first.toml": "",
                },
            },
        };
        let project = ProjectRoot::try_from(fs.join("project").unwrap()).unwrap();

        let conversation = ConversationRoot::workflow_subdir(&project).unwrap();

        let preserved = conversation.as_path().join("01_first.toml").unwrap();
        assert!(
            preserved.exists().unwrap(),
            "existing files under .ailly must be preserved across resolution"
        );
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
    fn list_workflows_unions_knowledge_and_conversation_root_first_wins() {
        let fs = mem_fs! {
            "project": {
                "AGENTS.md": "# agents\n",
                "workflow.toml": "name = \"local\"\ndescription = \"the local one\"\nstart = \"go\"\n[[tasks]]\nname = \"go\"\ntask = { kind = \"prompt\", text = \"hi\" }\n",
            },
            "extra_kb": {
                "AGENTS.md": "# extra\n",
                "workflows": {
                    "shared.toml": "name = \"shared\"\ndescription = \"the shared one\"\nstart = \"go\"\n[[tasks]]\nname = \"go\"\ntask = { kind = \"prompt\", text = \"hi\" }\n",
                    "local.toml": "name = \"local\"\ndescription = \"the SECOND local one\"\nstart = \"go\"\n[[tasks]]\nname = \"go\"\ntask = { kind = \"prompt\", text = \"hi\" }\n",
                },
            },
        };
        let project_root = ProjectRoot::try_from(fs.join("project").unwrap()).unwrap();
        let extra = KnowledgeRoot::try_from(fs.join("extra_kb").unwrap()).unwrap();
        let project = Project {
            root: project_root.clone(),
            conversations: ConversationRoot::from(project_root.clone()),
            knowledge: vec![KnowledgeRoot::from(project_root.clone()), extra],
            bash_cwd: std::path::PathBuf::from("."),
        };

        let entries = project.list_workflows().expect("list_workflows ok");

        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["local", "shared"]);
        let local = entries.iter().find(|e| e.name == "local".into()).unwrap();
        assert_eq!(local.description, "the local one");
        assert_eq!(local.source_path, "workflow.toml");
        let shared = entries.iter().find(|e| e.name == "shared".into()).unwrap();
        assert_eq!(shared.description, "the shared one");
        assert_eq!(shared.source_path, "workflows/shared.toml");
    }

    #[test]
    fn list_skills_dedupes_first_wins_across_knowledge_roots_and_sorts_alphabetically() {
        let fs = mem_fs! {
            "project": {
                "AGENTS.md": "# agents\n",
                "skills": {
                    "local": { "SKILL.md": "---\nname: local\ndescription: project local\n---\nbody\n" },
                    "shared": { "SKILL.md": "---\nname: shared\ndescription: PROJECT shared\n---\nbody\n" },
                },
            },
            "extra_kb": {
                "AGENTS.md": "# kb\n",
                "skills": {
                    "extra": { "SKILL.md": "---\nname: extra\ndescription: kb extra\n---\nbody\n" },
                    "shared": { "SKILL.md": "---\nname: shared\ndescription: KB shared\n---\nbody\n" },
                },
            },
        };
        let project_root = ProjectRoot::try_from(fs.join("project").unwrap()).unwrap();
        let extra = KnowledgeRoot::try_from(fs.join("extra_kb").unwrap()).unwrap();
        let project = Project {
            root: project_root.clone(),
            conversations: ConversationRoot::from(project_root.clone()),
            knowledge: vec![KnowledgeRoot::from(project_root.clone()), extra],
            bash_cwd: std::path::PathBuf::from("."),
        };

        let entries = project.list_skills().expect("list_skills ok");

        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["extra", "local", "shared"]);
        let shared = entries.iter().find(|e| e.name == "shared").unwrap();
        assert_eq!(shared.description, "PROJECT shared");
        assert_eq!(shared.source_path, "skills/shared/SKILL.md");
    }

    #[test]
    fn list_skills_returns_entries_when_a_knowledge_root_carries_a_skill() {
        let fs = mem_fs! {
            "project": { "AGENTS.md": "# agents\n" },
            "extra_kb": {
                "skills": {
                    "only": { "SKILL.md": "---\nname: only\ndescription: kb only\n---\nbody\n" },
                },
            },
        };
        let project_root = ProjectRoot::try_from(fs.join("project").unwrap()).unwrap();
        let extra = KnowledgeRoot::try_from(fs.join("extra_kb").unwrap()).unwrap();
        let project = Project {
            root: project_root.clone(),
            conversations: ConversationRoot::from(project_root.clone()),
            knowledge: vec![KnowledgeRoot::from(project_root.clone()), extra],
            bash_cwd: std::path::PathBuf::from("."),
        };

        let entries = project.list_skills().expect("list_skills ok");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "only");
        assert_eq!(entries[0].description, "kb only");
        assert_eq!(entries[0].source_path, "skills/only/SKILL.md");
    }

    #[test]
    fn list_skills_empty_when_no_knowledge_roots_carry_skills() {
        let fs = mem_fs! { "project": { "AGENTS.md": "# agents\n" } };
        let project_root = ProjectRoot::try_from(fs.join("project").unwrap()).unwrap();
        let project = Project {
            root: project_root.clone(),
            conversations: ConversationRoot::from(project_root.clone()),
            knowledge: vec![KnowledgeRoot::from(project_root.clone())],
            bash_cwd: std::path::PathBuf::from("."),
        };

        let entries = project.list_skills().expect("list_skills ok");

        assert!(entries.is_empty(), "expected empty, got {entries:?}");
    }

    #[test]
    fn list_skills_skips_broken_skill_md_with_warning() {
        let fs = mem_fs! {
            "project": {
                "AGENTS.md": "# agents\n",
                "skills": {
                    "good": { "SKILL.md": "---\nname: good\ndescription: ok\n---\nbody\n" },
                    "broken": { "SKILL.md": "no frontmatter at all\n" },
                },
            },
        };
        let project_root = ProjectRoot::try_from(fs.join("project").unwrap()).unwrap();
        let project = Project {
            root: project_root.clone(),
            conversations: ConversationRoot::from(project_root.clone()),
            knowledge: vec![KnowledgeRoot::from(project_root.clone())],
            bash_cwd: std::path::PathBuf::from("."),
        };

        let entries = project.list_skills().expect("list_skills ok");

        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["good"]);
    }

    #[test]
    fn list_workflows_skips_broken_toml_with_warning() {
        let fs = mem_fs! {
            "project": {
                "AGENTS.md": "# agents\n",
                "workflows": {
                    "good.toml": "name = \"good\"\nstart = \"go\"\n[[tasks]]\nname = \"go\"\ntask = { kind = \"prompt\", text = \"hi\" }\n",
                    "broken.toml": "this is { not valid toml [[",
                },
            },
        };
        let project_root = ProjectRoot::try_from(fs.join("project").unwrap()).unwrap();
        let project = Project {
            root: project_root.clone(),
            conversations: ConversationRoot::from(project_root.clone()),
            knowledge: vec![KnowledgeRoot::from(project_root.clone())],
            bash_cwd: std::path::PathBuf::from("."),
        };

        let entries = project.list_workflows().expect("list_workflows ok");

        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["good"]);
    }

    #[test]
    fn workflow_search_paths_lists_each_knowledge_root_workflow_toml_then_workflows_dir() {
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
            bash_cwd: std::path::PathBuf::from("."),
        };

        let paths = project.workflow_search_paths();

        assert_eq!(paths.len(), 4);
        assert!(
            paths[0].ends_with("/project/workflow.toml"),
            "first entry should be the project knowledge root's workflow.toml: {paths:?}"
        );
        assert!(
            paths[1].ends_with("/project/workflows"),
            "second entry should be the project knowledge root's workflows dir: {paths:?}"
        );
        assert!(
            paths[2].ends_with("/extra_kb/workflow.toml"),
            "third entry should be the extra knowledge root's workflow.toml: {paths:?}"
        );
        assert!(
            paths[3].ends_with("/extra_kb/workflows"),
            "fourth entry should be the extra knowledge root's workflows dir: {paths:?}"
        );
    }

    #[tokio::test]
    async fn list_tools_returns_registered_names_with_descriptions() {
        let fs = mem_fs! { "project": { "AGENTS.md": "# agents\n" } };
        let project_root = ProjectRoot::try_from(fs.join("project").unwrap()).unwrap();
        let project = Project {
            root: project_root.clone(),
            conversations: ConversationRoot::from(project_root.clone()),
            knowledge: vec![KnowledgeRoot::from(project_root.clone())],
            bash_cwd: std::path::PathBuf::from("."),
        };

        let entries = project.list_tools().await;

        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "bash",
                "fs.absent",
                "fs.edit",
                "fs.grep",
                "fs.list",
                "user.clarify",
            ]
        );
        assert!(
            entries.iter().all(|e| !e.description.is_empty()),
            "every tool should have a non-empty description: {entries:?}"
        );
    }

    #[test]
    fn project_tool_registry_includes_bash_and_fs_tools() {
        let fs = mem_fs! { "project": { "AGENTS.md": "# agents\n" } };
        let project_root = ProjectRoot::try_from(fs.join("project").unwrap()).unwrap();
        let project = Project {
            root: project_root.clone(),
            conversations: ConversationRoot::from(project_root.clone()),
            knowledge: vec![KnowledgeRoot::from(project_root.clone())],
            bash_cwd: std::path::PathBuf::from("."),
        };

        let registry = project.tool_registry();
        let mut names = registry.names();
        names.sort();

        assert_eq!(
            names,
            vec![
                "bash",
                "fs.absent",
                "fs.edit",
                "fs.grep",
                "fs.list",
                "user.clarify",
            ]
        );
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
            bash_cwd: std::path::PathBuf::from("."),
        };

        assert_eq!(project.knowledge.len(), 2);
        assert_eq!(
            project.knowledge[0].as_path().as_str(),
            project.root.as_path().as_str()
        );
    }
}
