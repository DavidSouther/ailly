use crate::knowledge::skills::types::SkillName;

#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("skill `{name}` not found under {search_path}")]
    Missing {
        name: SkillName,
        search_path: String,
    },

    #[error("skill `{expected}` declares name `{found}` in its frontmatter")]
    NameMismatch {
        expected: SkillName,
        found: String,
    },

    #[error("skill `{name}` SKILL.md is missing required frontmatter field `{field}`")]
    FrontmatterMissing {
        name: SkillName,
        field: &'static str,
    },

    #[error("skill `{name}` SKILL.md frontmatter field `{field}` is invalid: {reason}")]
    FrontmatterInvalid {
        name: SkillName,
        field: &'static str,
        reason: String,
    },

    #[error("skill `{name}` SKILL.md failed to parse YAML frontmatter")]
    FrontmatterParse {
        name: SkillName,
        #[source]
        source: serde_yml::Error,
    },

    #[error("skill `{name}` SKILL.md could not be read at {path}")]
    Read {
        name: SkillName,
        path: String,
        #[source]
        source: vfs::VfsError,
    },

    #[error("invalid skill name `{raw}`: {reason}")]
    InvalidName { raw: String, reason: String },

    #[error("invalid skill description: {reason}")]
    InvalidDescription { reason: String },
}
