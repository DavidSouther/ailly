//! `KnowledgeBase` trait surface and the filesystem-backed implementation.
//!
//! On-disk conventions inside any [`KnowledgeRoot`]:
//!
//! - `<root>/AGENTS.md` — root-level agent instructions.
//! - `<root>/skills/<name>/SKILL.md` — one skill per directory.
//! - `<root>/workflows/<name>.toml` — one workflow definition per file.
//!
//! Lookup methods are first-wins across the [`FsKnowledgeBase`] root list.
//! Listing methods union across roots and dedupe by name with the first
//! occurrence winning.

use vfs::VfsPath;

use crate::knowledge::skills::{Skill, SkillError, SkillName, SkillSource};
use crate::project::KnowledgeRoot;
use crate::workflow::Workflow;

const SKILLS_DIR: &str = "skills";
const WORKFLOWS_DIR: &str = "workflows";
const AGENTS_FILE: &str = "AGENTS.md";
const SKILL_FILE: &str = "SKILL.md";
const WORKFLOW_EXT: &str = "toml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowName(String);

impl WorkflowName {
    pub fn new(raw: impl Into<String>) -> Self {
        Self(raw.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct KnowledgeSource {
    pub root: KnowledgeRoot,
    pub path: VfsPath,
}

#[derive(Debug, Clone)]
pub struct SkillSummary {
    pub name: SkillName,
    pub description: String,
    pub source: KnowledgeSource,
}

#[derive(Debug, Clone)]
pub struct WorkflowSummary {
    pub name: WorkflowName,
    pub description: String,
    pub source: KnowledgeSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KnowledgeKind {
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
    #[error("{kind:?} {name:?} not found in any of {search_paths:?}")]
    Missing {
        kind: KnowledgeKind,
        name: String,
        search_paths: Vec<String>,
    },
    #[error("{kind:?} {name:?} could not be read at {path:?}")]
    Read {
        kind: KnowledgeKind,
        name: String,
        path: String,
        #[source]
        source: vfs::VfsError,
    },
    #[error("{kind:?} {name:?} at {path:?} failed to parse")]
    Parse {
        kind: KnowledgeKind,
        name: String,
        path: String,
        #[source]
        source: anyhow::Error,
    },
    #[error("{kind:?} requested as {requested:?} but declares itself as {declared:?}")]
    NameMismatch {
        kind: KnowledgeKind,
        requested: String,
        declared: String,
    },
    #[error("invalid {kind:?} name {raw:?}: {reason}")]
    InvalidName {
        kind: KnowledgeKind,
        raw: String,
        reason: String,
    },
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

/// Filesystem-backed [`KnowledgeBase`].
pub struct FsKnowledgeBase {
    roots: Vec<KnowledgeRoot>,
}

impl FsKnowledgeBase {
    pub fn new(roots: Vec<KnowledgeRoot>) -> Self {
        Self { roots }
    }
}

impl KnowledgeBase for FsKnowledgeBase {
    fn skill(&self, name: &SkillName) -> Result<Skill, KnowledgeError> {
        let mut search_paths = Vec::with_capacity(self.roots.len());
        for root in &self.roots {
            let skill_md = skill_md_path(root, name)?;
            search_paths.push(skill_md.as_str().to_string());
            if !exists(&skill_md, KnowledgeKind::Skill, name.as_str())? {
                continue;
            }
            let raw = read_to_string(&skill_md, KnowledgeKind::Skill, name.as_str())?;
            return Skill::parse(SkillSource(skill_md.clone()), &raw, name)
                .map_err(|err| skill_error_to_knowledge(err, name, &skill_md));
        }
        Err(KnowledgeError::Missing {
            kind: KnowledgeKind::Skill,
            name: name.as_str().to_string(),
            search_paths,
        })
    }

    fn workflow(&self, name: &WorkflowName) -> Result<Workflow, KnowledgeError> {
        let mut search_paths = Vec::with_capacity(self.roots.len());
        for root in &self.roots {
            let workflow_path = workflow_toml_path(root, name)?;
            search_paths.push(workflow_path.as_str().to_string());
            if !exists(&workflow_path, KnowledgeKind::Workflow, name.as_str())? {
                continue;
            }
            let raw = read_to_string(&workflow_path, KnowledgeKind::Workflow, name.as_str())?;
            let workflow: Workflow = toml::from_str(&raw).map_err(|err| KnowledgeError::Parse {
                kind: KnowledgeKind::Workflow,
                name: name.as_str().to_string(),
                path: workflow_path.as_str().to_string(),
                source: anyhow::Error::new(err),
            })?;
            if workflow.name != name.as_str() {
                return Err(KnowledgeError::NameMismatch {
                    kind: KnowledgeKind::Workflow,
                    requested: name.as_str().to_string(),
                    declared: workflow.name,
                });
            }
            return Ok(workflow);
        }
        Err(KnowledgeError::Missing {
            kind: KnowledgeKind::Workflow,
            name: name.as_str().to_string(),
            search_paths,
        })
    }

    fn agents(&self) -> Result<Vec<AgentsDoc>, KnowledgeError> {
        let mut docs = Vec::new();
        for root in &self.roots {
            let agents_path = agents_md_path(root)?;
            if !exists(&agents_path, KnowledgeKind::Agents, AGENTS_FILE)? {
                continue;
            }
            let body = read_to_string(&agents_path, KnowledgeKind::Agents, AGENTS_FILE)?;
            docs.push(AgentsDoc {
                source: KnowledgeSource {
                    root: root.clone(),
                    path: agents_path,
                },
                body,
            });
        }
        Ok(docs)
    }

    fn list_skills(&self) -> Result<Vec<SkillSummary>, KnowledgeError> {
        let mut out: Vec<SkillSummary> = Vec::new();
        for root in &self.roots {
            for entry in iter_skill_dirs(root)? {
                let (name, dir) = entry?;
                if out.iter().any(|s| s.name == name) {
                    continue;
                }
                let skill_md = join_under(&dir, SKILL_FILE, KnowledgeKind::Skill, name.as_str())?;
                if !exists(&skill_md, KnowledgeKind::Skill, name.as_str())? {
                    continue;
                }
                let raw = read_to_string(&skill_md, KnowledgeKind::Skill, name.as_str())?;
                let skill = Skill::parse(SkillSource(skill_md.clone()), &raw, &name)
                    .map_err(|err| skill_error_to_knowledge(err, &name, &skill_md))?;
                out.push(SkillSummary {
                    name: skill.name,
                    description: skill.description.as_str().to_string(),
                    source: KnowledgeSource {
                        root: root.clone(),
                        path: skill_md,
                    },
                });
            }
        }
        Ok(out)
    }

    fn list_workflows(&self) -> Result<Vec<WorkflowSummary>, KnowledgeError> {
        let mut out: Vec<WorkflowSummary> = Vec::new();
        for root in &self.roots {
            for entry in iter_workflow_files(root)? {
                let (name, path) = entry?;
                if out.iter().any(|w| w.name == name) {
                    continue;
                }
                let raw = read_to_string(&path, KnowledgeKind::Workflow, name.as_str())?;
                let workflow: Workflow =
                    toml::from_str(&raw).map_err(|err| KnowledgeError::Parse {
                        kind: KnowledgeKind::Workflow,
                        name: name.as_str().to_string(),
                        path: path.as_str().to_string(),
                        source: anyhow::Error::new(err),
                    })?;
                out.push(WorkflowSummary {
                    name: WorkflowName::new(workflow.name),
                    description: workflow.description.unwrap_or_default(),
                    source: KnowledgeSource {
                        root: root.clone(),
                        path,
                    },
                });
            }
        }
        Ok(out)
    }

    fn search(&self, query: &str) -> Result<Vec<KnowledgeHit>, KnowledgeError> {
        let mut hits = Vec::new();
        for root in &self.roots {
            let mut names: Vec<(KnowledgeKind, String, VfsPath)> = Vec::new();
            for entry in iter_skill_dirs(root)? {
                let (name, dir) = entry?;
                names.push((KnowledgeKind::Skill, name.as_str().to_string(), dir));
            }
            for entry in iter_workflow_files(root)? {
                let (name, path) = entry?;
                names.push((KnowledgeKind::Workflow, name.as_str().to_string(), path));
            }
            names.sort_by(|a, b| a.1.cmp(&b.1));
            for (kind, name, path) in names {
                if name.contains(query) {
                    hits.push(KnowledgeHit {
                        kind,
                        name,
                        score: 1.0,
                        source: KnowledgeSource {
                            root: root.clone(),
                            path,
                        },
                    });
                }
            }
        }
        Ok(hits)
    }
}

fn join_under(
    parent: &VfsPath,
    segment: &str,
    kind: KnowledgeKind,
    name: &str,
) -> Result<VfsPath, KnowledgeError> {
    parent.join(segment).map_err(|source| KnowledgeError::Read {
        kind,
        name: name.to_string(),
        path: parent.as_str().to_string(),
        source,
    })
}

fn skill_md_path(root: &KnowledgeRoot, name: &SkillName) -> Result<VfsPath, KnowledgeError> {
    let kind = KnowledgeKind::Skill;
    let n = name.as_str();
    let skills = join_under(root.as_path(), SKILLS_DIR, kind, n)?;
    let dir = join_under(&skills, n, kind, n)?;
    join_under(&dir, SKILL_FILE, kind, n)
}

fn workflow_toml_path(
    root: &KnowledgeRoot,
    name: &WorkflowName,
) -> Result<VfsPath, KnowledgeError> {
    let kind = KnowledgeKind::Workflow;
    let n = name.as_str();
    let workflows = join_under(root.as_path(), WORKFLOWS_DIR, kind, n)?;
    let file = format!("{n}.{WORKFLOW_EXT}");
    join_under(&workflows, &file, kind, n)
}

fn agents_md_path(root: &KnowledgeRoot) -> Result<VfsPath, KnowledgeError> {
    join_under(
        root.as_path(),
        AGENTS_FILE,
        KnowledgeKind::Agents,
        AGENTS_FILE,
    )
}

fn exists(path: &VfsPath, kind: KnowledgeKind, name: &str) -> Result<bool, KnowledgeError> {
    path.exists().map_err(|source| KnowledgeError::Read {
        kind,
        name: name.to_string(),
        path: path.as_str().to_string(),
        source,
    })
}

fn read_to_string(
    path: &VfsPath,
    kind: KnowledgeKind,
    name: &str,
) -> Result<String, KnowledgeError> {
    path.read_to_string()
        .map_err(|source| KnowledgeError::Read {
            kind,
            name: name.to_string(),
            path: path.as_str().to_string(),
            source,
        })
}

pub(crate) type SkillFsEntry = Result<(SkillName, VfsPath), KnowledgeError>;

pub(crate) fn iter_skill_dirs(
    root: &KnowledgeRoot,
) -> Result<Box<dyn Iterator<Item = SkillFsEntry>>, KnowledgeError> {
    let skills_dir = join_under(root.as_path(), SKILLS_DIR, KnowledgeKind::Skill, SKILLS_DIR)?;
    let exists = skills_dir.exists().map_err(|source| KnowledgeError::Read {
        kind: KnowledgeKind::Skill,
        name: SKILLS_DIR.to_string(),
        path: skills_dir.as_str().to_string(),
        source,
    })?;
    if !exists {
        return Ok(Box::new(std::iter::empty()));
    }
    let entries = skills_dir
        .read_dir()
        .map_err(|source| KnowledgeError::Read {
            kind: KnowledgeKind::Skill,
            name: SKILLS_DIR.to_string(),
            path: skills_dir.as_str().to_string(),
            source,
        })?;
    Ok(Box::new(entries.filter_map(|entry| {
        let is_dir = match entry.is_dir() {
            Ok(b) => b,
            Err(source) => {
                return Some(Err(KnowledgeError::Read {
                    kind: KnowledgeKind::Skill,
                    name: entry.filename(),
                    path: entry.as_str().to_string(),
                    source,
                }));
            }
        };
        if !is_dir {
            return None;
        }
        let raw = entry.filename();
        let name = match SkillName::try_from(&raw) {
            Ok(n) => n,
            Err(_) => return None,
        };
        Some(Ok((name, entry)))
    })))
}

pub(crate) type WorkflowFsEntry = Result<(WorkflowName, VfsPath), KnowledgeError>;

pub(crate) fn iter_workflow_files(
    root: &KnowledgeRoot,
) -> Result<Box<dyn Iterator<Item = WorkflowFsEntry>>, KnowledgeError> {
    let workflows_dir = join_under(
        root.as_path(),
        WORKFLOWS_DIR,
        KnowledgeKind::Workflow,
        WORKFLOWS_DIR,
    )?;
    let exists = workflows_dir
        .exists()
        .map_err(|source| KnowledgeError::Read {
            kind: KnowledgeKind::Workflow,
            name: WORKFLOWS_DIR.to_string(),
            path: workflows_dir.as_str().to_string(),
            source,
        })?;
    if !exists {
        return Ok(Box::new(std::iter::empty()));
    }
    let entries = workflows_dir
        .read_dir()
        .map_err(|source| KnowledgeError::Read {
            kind: KnowledgeKind::Workflow,
            name: WORKFLOWS_DIR.to_string(),
            path: workflows_dir.as_str().to_string(),
            source,
        })?;
    let suffix = format!(".{WORKFLOW_EXT}");
    Ok(Box::new(entries.filter_map(move |entry| {
        let filename = entry.filename();
        let stem = filename.strip_suffix(&suffix)?;
        Some(Ok((WorkflowName::new(stem), entry)))
    })))
}

fn skill_error_to_knowledge(err: SkillError, name: &SkillName, path: &VfsPath) -> KnowledgeError {
    match err {
        SkillError::NameMismatch { found, .. } => KnowledgeError::NameMismatch {
            kind: KnowledgeKind::Skill,
            requested: name.as_str().to_string(),
            declared: found,
        },
        other => KnowledgeError::Parse {
            kind: KnowledgeKind::Skill,
            name: name.as_str().to_string(),
            path: path.as_str().to_string(),
            source: anyhow::Error::new(other),
        },
    }
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
        let kb = FsKnowledgeBase::new(vec![project, extra]);

        let shared = kb.skill(&SkillName::try_from("shared").unwrap()).unwrap();

        assert_eq!(shared.body.as_str(), "PROJECT-SHARED BODY");
    }

    #[test]
    fn fs_knowledge_base_workflow_first_wins() {
        let (project, extra) = project_and_extra();
        let kb = FsKnowledgeBase::new(vec![project, extra]);

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
        let kb = FsKnowledgeBase::new(vec![project, extra]);

        let mut summaries = kb.list_skills().unwrap();
        summaries.sort_by(|a, b| a.name.as_str().cmp(b.name.as_str()));

        let names: Vec<&str> = summaries.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["extra", "local", "shared"]);
        let shared = summaries
            .iter()
            .find(|s| s.name.as_str() == "shared")
            .unwrap();
        assert_eq!(shared.source.root.as_path().as_str(), project_path);
    }

    #[test]
    fn fs_knowledge_base_search_returns_hits_from_all_roots() {
        let (project, extra) = project_and_extra();
        let kb = FsKnowledgeBase::new(vec![project, extra]);

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
        let kb = FsKnowledgeBase::new(vec![project, extra]);

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
        let kb = FsKnowledgeBase::new(vec![project, extra]);

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
}
