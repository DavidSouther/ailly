use vfs::VfsPath;

use crate::knowledge::skills::errors::SkillError;
use crate::knowledge::skills::types::{Skill, SkillName, SkillSource};

pub trait SkillRepository: Send + Sync {
    fn get(&self, name: &SkillName) -> Result<Skill, SkillError>;
}

/// Adapter that resolves skills from `<project_root>/.ailly/skills/<name>/SKILL.md`.
///
/// Construction is infallible. Absence of `<root>/.ailly/skills/` is tolerated
/// and surfaces only at the first `get` call.
pub struct FsSkillRepository {
    root: VfsPath,
}

impl FsSkillRepository {
    pub fn new(project_root: &VfsPath) -> Self {
        let root = project_root
            .join(".ailly/skills")
            .expect("static suffix `.ailly/skills` is always join-able");
        Self { root }
    }

    pub fn search_path(&self) -> &str {
        self.root.as_str()
    }
}

impl SkillRepository for FsSkillRepository {
    fn get(&self, name: &SkillName) -> Result<Skill, SkillError> {
        let dir = self
            .root
            .join(name.as_str())
            .map_err(|source| SkillError::Read {
                name: name.clone(),
                path: self.root.as_str().to_string(),
                source,
            })?;
        let exists = dir.exists().map_err(|source| SkillError::Read {
            name: name.clone(),
            path: dir.as_str().to_string(),
            source,
        })?;
        if !exists {
            return Err(SkillError::Missing {
                name: name.clone(),
                search_path: self.root.as_str().to_string(),
            });
        }
        let skill_md = dir.join("SKILL.md").map_err(|source| SkillError::Read {
            name: name.clone(),
            path: dir.as_str().to_string(),
            source,
        })?;
        let raw = skill_md
            .read_to_string()
            .map_err(|source| SkillError::Read {
                name: name.clone(),
                path: skill_md.as_str().to_string(),
                source,
            })?;
        let source = SkillSource(skill_md);
        Skill::parse(source, &raw, name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem_fs;

    #[test]
    fn repository_get_returns_skill_for_existing_directory() {
        let fs = mem_fs! {
            "root": {
                ".ailly": {
                    "skills": {
                        "foo": {
                            "SKILL.md": "---\nname: foo\ndescription: a foo\n---\nFOO BODY\n",
                        },
                    },
                },
            },
        };
        let root = fs.join("root").unwrap();
        let repo = FsSkillRepository::new(&root);
        let name = SkillName::try_from("foo").unwrap();

        let skill = repo.get(&name).unwrap();

        assert_eq!(skill.name.as_str(), "foo");
        assert_eq!(skill.description.as_str(), "a foo");
        assert_eq!(skill.body.as_str(), "FOO BODY");
    }

    #[test]
    fn repository_get_missing_includes_search_path_in_error() {
        let fs = mem_fs! { "root": {} };
        let root = fs.join("root").unwrap();
        let repo = FsSkillRepository::new(&root);
        let name = SkillName::try_from("nope").unwrap();

        let err = repo.get(&name).unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("nope"), "error must include skill name: {msg}");
        assert!(
            msg.contains(".ailly/skills"),
            "error must include search path: {msg}"
        );
    }
}
