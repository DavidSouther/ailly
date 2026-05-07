//! `KnowledgeBase` trait surface and the filesystem-backed implementation.
//!
//! On-disk conventions inside any [`KnowledgeRoot`]:
//!
//! - `<root>/AGENTS.md` — root-level agent instructions.
//! - `<root>/skills/<name>/SKILL.md` — one skill per directory.
//! - `<root>/workflows/<name>.toml` — one workflow definition per file.
//!
//! `FsKnowledgeBase::build` walks every root once, parses every entry, and
//! stores the results in containers. Lookup methods are then plain
//! `HashMap`/`Vec` access. Same-named skills and workflows are deduped
//! first-wins, project root before extras.

use std::collections::HashMap;

use vfs::VfsPath;

use crate::knowledge::skills::{Skill, SkillError, SkillName};
use crate::project::KnowledgeRoot;
use crate::workflow::{Workflow, WorkflowParseError};

const SKILLS_DIR: &str = "skills";
const WORKFLOWS_DIR: &str = "workflows";
const AGENTS_FILE: &str = "AGENTS.md";
const SKILL_FILE: &str = "SKILL.md";
const WORKFLOW_EXT: &str = ".toml";

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WorkflowName(String);

impl WorkflowName {
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for WorkflowName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<I> From<I> for WorkflowName
where
    I: Into<String>,
{
    fn from(value: I) -> Self {
        WorkflowName::new(value)
    }
}

#[derive(Debug, Clone)]
pub struct KnowledgeSource {
    pub root: KnowledgeRoot,
    pub path: VfsPath,
}

#[derive(Debug, Clone)]
pub struct SkillSummary {
    name: SkillName,
    description: String,
    source: KnowledgeSource,
}

impl SkillSummary {
    pub fn name(&self) -> &SkillName {
        &self.name
    }

    pub fn description(&self) -> &str {
        &self.description
    }

    pub fn source(&self) -> &KnowledgeSource {
        &self.source
    }
}

impl From<&Skill> for SkillSummary {
    fn from(value: &Skill) -> Self {
        SkillSummary {
            name: value.name().clone(),
            description: value.description().as_str().to_string(),
            source: value.source().clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorkflowSummary {
    pub name: WorkflowName,
    pub description: String,
    pub source: KnowledgeSource,
}

impl From<&LoadedWorkflow> for WorkflowSummary {
    fn from(value: &LoadedWorkflow) -> Self {
        WorkflowSummary {
            name: WorkflowName::new(&value.workflow.name),
            description: value.workflow.description.clone().unwrap_or_default(),
            source: value.source.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeKind {
    Any,
    Skill,
    Workflow,
    Agents,
}

#[derive(Debug, Clone)]
pub struct KnowledgeHit {
    pub kind: KnowledgeKind,
    pub name: String,
    pub score: f32,
    pub source: KnowledgeSource,
}

#[derive(Debug, Clone)]
pub struct AgentsDoc {
    pub source: KnowledgeSource,
    pub body: String,
}

#[derive(Debug, thiserror::Error)]
pub enum KnowledgeError {
    #[error("invalid {kind:?} name {raw:?}: {reason}")]
    InvalidName {
        kind: KnowledgeKind,
        raw: String,
        reason: String,
    },

    #[error("{kind:?} {name:?} not found in any of {search_paths:?}")]
    Missing {
        kind: KnowledgeKind,
        name: String,
        search_paths: Vec<String>,
    },

    #[error("could not read knowledge file at {path:?}")]
    Read {
        path: String,
        #[source]
        source: vfs::VfsError,
    },

    #[error("{kind:?} requested as {requested:?} but the file declares {declared:?}")]
    NameMismatch {
        kind: KnowledgeKind,
        requested: String,
        declared: String,
    },

    #[error("failed to parse {kind:?}: {source}")]
    Parse {
        kind: KnowledgeKind,
        #[source]
        source: ParseSource,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum ParseSource {
    #[error(transparent)]
    Skill(#[from] SkillError),
    #[error(transparent)]
    Workflow(#[from] WorkflowParseError),
}

impl From<vfs::VfsError> for KnowledgeError {
    fn from(value: vfs::VfsError) -> Self {
        KnowledgeError::Read {
            path: value.path().to_string(),
            source: value,
        }
    }
}

impl From<SkillError> for KnowledgeError {
    fn from(value: SkillError) -> Self {
        match value {
            SkillError::InvalidName { raw, reason } => KnowledgeError::InvalidName {
                kind: KnowledgeKind::Skill,
                raw,
                reason,
            },
            SkillError::NameMismatch { expected, found } => KnowledgeError::NameMismatch {
                kind: KnowledgeKind::Skill,
                requested: expected.as_str().to_string(),
                declared: found,
            },
            other => KnowledgeError::Parse {
                kind: KnowledgeKind::Skill,
                source: ParseSource::Skill(other),
            },
        }
    }
}

impl From<WorkflowParseError> for KnowledgeError {
    fn from(value: WorkflowParseError) -> Self {
        KnowledgeError::Parse {
            kind: KnowledgeKind::Workflow,
            source: ParseSource::Workflow(value),
        }
    }
}

pub trait KnowledgeBase: Send + Sync {
    fn skill(&self, name: &SkillName) -> Result<Skill, KnowledgeError>;
    fn workflow(&self, name: &WorkflowName) -> Result<Workflow, KnowledgeError>;
    fn agents(&self) -> Result<Vec<AgentsDoc>, KnowledgeError>;

    fn list_skills(&self) -> Result<Vec<SkillSummary>, KnowledgeError>;
    fn list_workflows(&self) -> Result<Vec<WorkflowSummary>, KnowledgeError>;

    fn search(&self, query: &str) -> Result<Vec<KnowledgeHit>, KnowledgeError>;
}

/// A [`KnowledgeBase`] that fails every lookup. Useful as a placeholder
/// when a caller has no knowledge configured and any `skills =` declaration
/// in a `.ailly.toml` should surface as a missing-skill error rather than
/// silently succeed.
pub struct EmptyKnowledgeBase;

impl KnowledgeBase for EmptyKnowledgeBase {
    fn skill(&self, name: &SkillName) -> Result<Skill, KnowledgeError> {
        Err(KnowledgeError::Missing {
            kind: KnowledgeKind::Skill,
            name: name.as_str().to_string(),
            search_paths: vec!["<no knowledge configured>".to_string()],
        })
    }
    fn workflow(&self, name: &WorkflowName) -> Result<Workflow, KnowledgeError> {
        Err(KnowledgeError::Missing {
            kind: KnowledgeKind::Workflow,
            name: name.as_str().to_string(),
            search_paths: vec!["<no knowledge configured>".to_string()],
        })
    }
    fn agents(&self) -> Result<Vec<AgentsDoc>, KnowledgeError> {
        Ok(Vec::new())
    }
    fn list_skills(&self) -> Result<Vec<SkillSummary>, KnowledgeError> {
        Ok(Vec::new())
    }
    fn list_workflows(&self) -> Result<Vec<WorkflowSummary>, KnowledgeError> {
        Ok(Vec::new())
    }
    fn search(&self, _query: &str) -> Result<Vec<KnowledgeHit>, KnowledgeError> {
        Ok(Vec::new())
    }
}

#[derive(Debug, Clone)]
pub(crate) struct LoadedWorkflow {
    pub workflow: Workflow,
    pub source: KnowledgeSource,
}

/// Filesystem-backed [`KnowledgeBase`]. Eagerly loads every skill,
/// workflow, and `AGENTS.md` from each root at construction time.
pub struct FsKnowledgeBase {
    roots: Vec<KnowledgeRoot>,
    skills: HashMap<SkillName, Skill>,
    workflows: HashMap<WorkflowName, LoadedWorkflow>,
    agents: Vec<AgentsDoc>,
}

impl FsKnowledgeBase {
    pub fn build(roots: Vec<KnowledgeRoot>) -> Result<Self, KnowledgeError> {
        let mut skills: HashMap<SkillName, Skill> = HashMap::new();
        let mut workflows: HashMap<WorkflowName, LoadedWorkflow> = HashMap::new();
        let mut agents: Vec<AgentsDoc> = Vec::new();

        for root in &roots {
            for entry in iter_skill_dirs(root)? {
                let (name, dir) = entry?;
                if skills.contains_key(&name) {
                    continue;
                }
                let skill_md = dir.join(SKILL_FILE)?;
                if !skill_md.exists()? {
                    continue;
                }
                let raw = skill_md.read_to_string()?;
                let skill = Skill::parse(
                    KnowledgeSource {
                        root: root.clone(),
                        path: skill_md,
                    },
                    &raw,
                    &name,
                )?;
                skills.insert(skill.name().clone(), skill);
            }

            for entry in iter_workflow_files(root)? {
                let (filename_stem, path) = entry?;
                let raw = path.read_to_string()?;
                let workflow: Workflow =
                    toml::from_str(&raw).map_err(|source| WorkflowParseError {
                        name: filename_stem.as_str().to_string(),
                        source,
                    })?;
                let name = WorkflowName::new(&workflow.name);
                if workflows.contains_key(&name) {
                    continue;
                }
                workflows.insert(
                    name,
                    LoadedWorkflow {
                        workflow,
                        source: KnowledgeSource {
                            root: root.clone(),
                            path,
                        },
                    },
                );
            }

            let agents_path = root.as_path().join(AGENTS_FILE)?;
            if agents_path.exists()? {
                let body = agents_path.read_to_string()?;
                agents.push(AgentsDoc {
                    source: KnowledgeSource {
                        root: root.clone(),
                        path: agents_path,
                    },
                    body,
                });
            }
        }

        Ok(Self {
            roots,
            skills,
            workflows,
            agents,
        })
    }

    fn search_paths(&self) -> Vec<String> {
        self.roots
            .iter()
            .map(|r| r.display_path().to_string())
            .collect()
    }
}

impl KnowledgeBase for FsKnowledgeBase {
    fn skill(&self, name: &SkillName) -> Result<Skill, KnowledgeError> {
        self.skills
            .get(name)
            .cloned()
            .ok_or_else(|| KnowledgeError::Missing {
                kind: KnowledgeKind::Skill,
                name: name.as_str().to_string(),
                search_paths: self.search_paths(),
            })
    }

    fn workflow(&self, name: &WorkflowName) -> Result<Workflow, KnowledgeError> {
        self.workflows
            .get(name)
            .map(|loaded| loaded.workflow.clone())
            .ok_or_else(|| KnowledgeError::Missing {
                kind: KnowledgeKind::Workflow,
                name: name.as_str().to_string(),
                search_paths: self.search_paths(),
            })
    }

    fn agents(&self) -> Result<Vec<AgentsDoc>, KnowledgeError> {
        Ok(self.agents.clone())
    }

    fn list_skills(&self) -> Result<Vec<SkillSummary>, KnowledgeError> {
        Ok(self.skills.values().map(SkillSummary::from).collect())
    }

    fn list_workflows(&self) -> Result<Vec<WorkflowSummary>, KnowledgeError> {
        Ok(self.workflows.values().map(WorkflowSummary::from).collect())
    }

    fn search(&self, query: &str) -> Result<Vec<KnowledgeHit>, KnowledgeError> {
        let mut hits: Vec<KnowledgeHit> = Vec::new();
        for skill in self.skills.values() {
            let name = skill.name().as_str();
            if name.contains(query) {
                hits.push(KnowledgeHit {
                    kind: KnowledgeKind::Skill,
                    name: name.to_string(),
                    score: 1.0,
                    source: skill.source().clone(),
                });
            }
        }
        for (wf_name, loaded) in &self.workflows {
            if wf_name.as_str().contains(query) {
                hits.push(KnowledgeHit {
                    kind: KnowledgeKind::Workflow,
                    name: wf_name.as_str().to_string(),
                    score: 1.0,
                    source: loaded.source.clone(),
                });
            }
        }
        hits.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(hits)
    }
}

pub(crate) type SkillDirEntries = Vec<Result<(SkillName, VfsPath), KnowledgeError>>;
pub(crate) type WorkflowFileEntries = Vec<Result<(WorkflowName, VfsPath), KnowledgeError>>;

/// Enumerate the immediate subdirectories of `<root>/skills/`, paired with
/// their parsed [`SkillName`]. Returns an empty list when the directory is
/// absent. Per-entry name validation surfaces as `Err` items inside the
/// returned `Vec`; an I/O failure on the directory itself is the outer
/// `Err`.
pub(crate) fn iter_skill_dirs(root: &KnowledgeRoot) -> Result<SkillDirEntries, KnowledgeError> {
    let dir = root.as_path().join(SKILLS_DIR)?;
    if !dir.exists()? {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in dir.read_dir()? {
        if !entry.is_dir()? {
            continue;
        }
        let item = SkillName::try_from(entry.filename().as_str())
            .map(|name| (name, entry))
            .map_err(KnowledgeError::from);
        out.push(item);
    }
    Ok(out)
}

/// Enumerate the `*.toml` files under `<root>/workflows/`, paired with
/// their stem as a [`WorkflowName`]. Returns an empty list when the
/// directory is absent.
pub(crate) fn iter_workflow_files(
    root: &KnowledgeRoot,
) -> Result<WorkflowFileEntries, KnowledgeError> {
    let dir = root.as_path().join(WORKFLOWS_DIR)?;
    if !dir.exists()? {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in dir.read_dir()? {
        let filename = entry.filename();
        let Some(stem) = filename.strip_suffix(WORKFLOW_EXT) else {
            continue;
        };
        out.push(Ok((WorkflowName::new(stem), entry)));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem_fs;

    fn project_and_extra() -> (KnowledgeRoot, KnowledgeRoot) {
        let fs = mem_fs! {
            "project": {
                "AGENTS.md": "# project agents\n",
                "skills": {
                    "local": {
                        "SKILL.md": "---\nname: local\ndescription: project-only\n---\nLOCAL BODY\n",
                    },
                    "shared": {
                        "SKILL.md": "---\nname: shared\ndescription: project shared\n---\nPROJECT-SHARED BODY\n",
                    },
                },
                "workflows": {
                    "build.toml": "name = \"build\"\nstart = \"compile\"\n[[tasks]]\nname = \"compile\"\ntask = { kind = \"prompt\", text = \"build it\" }\n",
                    "shared.toml": "name = \"shared\"\nstart = \"go\"\n[[tasks]]\nname = \"go\"\ntask = { kind = \"prompt\", text = \"project shared workflow\" }\n",
                },
            },
            "extra_kb": {
                "AGENTS.md": "# extra agents\n",
                "skills": {
                    "extra": {
                        "SKILL.md": "---\nname: extra\ndescription: extra-only\n---\nEXTRA BODY\n",
                    },
                    "shared": {
                        "SKILL.md": "---\nname: shared\ndescription: extra shared\n---\nEXTRA-SHARED BODY\n",
                    },
                },
                "workflows": {
                    "deploy.toml": "name = \"deploy\"\nstart = \"ship\"\n[[tasks]]\nname = \"ship\"\ntask = { kind = \"prompt\", text = \"ship it\" }\n",
                    "shared.toml": "name = \"shared\"\nstart = \"go\"\n[[tasks]]\nname = \"go\"\ntask = { kind = \"prompt\", text = \"extra shared workflow\" }\n",
                },
            },
        };
        let project = KnowledgeRoot::try_from(fs.join("project").unwrap()).unwrap();
        let extra = KnowledgeRoot::try_from(fs.join("extra_kb").unwrap()).unwrap();
        (project, extra)
    }

    #[test]
    fn fs_knowledge_base_skill_first_wins() {
        let (project, extra) = project_and_extra();
        let kb = FsKnowledgeBase::build(vec![project, extra]).unwrap();

        let shared = kb.skill(&SkillName::try_from("shared").unwrap()).unwrap();

        assert_eq!(shared.body().as_str(), "PROJECT-SHARED BODY");
    }

    /// A workflow file's lookup key is the `name` field declared inside
    /// the TOML, not the filename stem. The DDD `developer` plugin ships
    /// `workflows/workflow.toml` with `name = "ailly"`; `-w ailly` must
    /// resolve it.
    #[test]
    fn fs_knowledge_base_keys_workflow_by_internal_name_not_filename() {
        let fs = mem_fs! {
            "kb": {
                "workflows": {
                    "workflow.toml": "name = \"ailly\"\nstart = \"go\"\n[[tasks]]\nname = \"go\"\ntask = { kind = \"prompt\", text = \"hi\" }\n",
                },
            },
        };
        let root = KnowledgeRoot::try_from(fs.join("kb").unwrap()).unwrap();
        let kb = FsKnowledgeBase::build(vec![root]).unwrap();

        let result = kb.workflow(&WorkflowName::new("ailly"));

        assert!(
            result.is_ok(),
            "lookup must use the workflow's internal `name` field, not the filename stem; got {result:?}"
        );
    }

    #[test]
    fn fs_knowledge_base_workflow_first_wins() {
        let (project, extra) = project_and_extra();
        let kb = FsKnowledgeBase::build(vec![project, extra]).unwrap();

        let shared = kb.workflow(&WorkflowName::new("shared")).unwrap();

        assert_eq!(shared.name, "shared");
        assert_eq!(shared.start, "go");
        assert_eq!(shared.tasks[0].name, "go");
        match &shared.tasks[0].task {
            crate::workflow::TaskAction::Prompt { text } => {
                assert_eq!(text, "project shared workflow");
            }
            other => panic!("expected Prompt, got {other:?}"),
        }
    }

    #[test]
    fn fs_knowledge_base_list_skills_dedupes_by_name_first_wins() {
        let (project, extra) = project_and_extra();
        let project_path = project.as_path().as_str().to_string();
        let kb = FsKnowledgeBase::build(vec![project, extra]).unwrap();

        let mut summaries = kb.list_skills().unwrap();
        summaries.sort_by(|a, b| a.name().as_str().cmp(b.name().as_str()));

        let names: Vec<&str> = summaries.iter().map(|s| s.name().as_str()).collect();
        assert_eq!(names, ["extra", "local", "shared"]);
        let shared = summaries
            .iter()
            .find(|s| s.name().as_str() == "shared")
            .unwrap();
        assert_eq!(shared.source().root.as_path().as_str(), project_path);
    }

    #[test]
    fn fs_knowledge_base_search_returns_hits_from_all_roots() {
        let (project, extra) = project_and_extra();
        let kb = FsKnowledgeBase::build(vec![project, extra]).unwrap();

        let hits = kb.search("shared").unwrap();

        let by_root: Vec<(KnowledgeKind, &str, &str)> = hits
            .iter()
            .map(|h| (h.kind, h.name.as_str(), h.source.root.as_path().as_str()))
            .collect();
        assert!(
            by_root
                .iter()
                .filter(|(_, name, _)| *name == "shared")
                .count()
                >= 2,
            "expected `shared` to appear in hits from both roots, got {by_root:?}"
        );
        assert!(
            by_root
                .iter()
                .any(|(kind, _, _)| *kind == KnowledgeKind::Skill),
            "expected at least one Skill hit, got {by_root:?}"
        );
        assert!(
            by_root
                .iter()
                .any(|(kind, _, _)| *kind == KnowledgeKind::Workflow),
            "expected at least one Workflow hit, got {by_root:?}"
        );
    }

    #[test]
    fn fs_knowledge_base_agents_orders_by_root_iteration() {
        let (project, extra) = project_and_extra();
        let kb = FsKnowledgeBase::build(vec![project, extra]).unwrap();

        let agents = kb.agents().unwrap();

        assert_eq!(agents.len(), 2);
        assert!(agents[0].body.contains("project agents"));
        assert!(agents[1].body.contains("extra agents"));
    }

    #[test]
    fn fs_knowledge_base_missing_skill_includes_all_search_paths() {
        let (project, extra) = project_and_extra();
        let project_path = project.as_path().as_str().to_string();
        let extra_path = extra.as_path().as_str().to_string();
        let kb = FsKnowledgeBase::build(vec![project, extra]).unwrap();

        let err = kb.skill(&SkillName::try_from("nope").unwrap()).unwrap_err();

        match err {
            KnowledgeError::Missing {
                kind,
                name,
                search_paths,
            } => {
                assert_eq!(kind, KnowledgeKind::Skill);
                assert_eq!(name, "nope");
                assert_eq!(search_paths.len(), 2);
                assert!(
                    search_paths[0].starts_with(&project_path),
                    "expected first search path under {project_path:?}, got {search_paths:?}"
                );
                assert!(
                    search_paths[1].starts_with(&extra_path),
                    "expected second search path under {extra_path:?}, got {search_paths:?}"
                );
            }
            other => panic!("expected Missing, got {other:?}"),
        }
    }

    /// `PhysicalFS`-backed roots have an empty `as_str()` because the
    /// path-within-filesystem is the root itself. The `Missing` error
    /// must surface the native filesystem path the user typed, not the
    /// empty in-vfs path.
    #[test]
    fn fs_knowledge_base_missing_includes_native_path_for_physical_fs_roots() {
        let raw = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let root = KnowledgeRoot::from_physical(raw).unwrap();
        let kb = FsKnowledgeBase::build(vec![root]).unwrap();

        let err = kb
            .workflow(&WorkflowName::new("definitely-missing"))
            .unwrap_err();

        match err {
            KnowledgeError::Missing { search_paths, .. } => {
                assert!(
                    search_paths.iter().all(|p| !p.is_empty()),
                    "search_paths must not contain empty strings, got: {search_paths:?}"
                );
                assert!(
                    search_paths
                        .iter()
                        .any(|p| p.contains(raw.to_str().unwrap())),
                    "search_paths should include the native filesystem path {raw:?}, got: {search_paths:?}"
                );
            }
            other => panic!("expected Missing, got {other:?}"),
        }
    }
}
