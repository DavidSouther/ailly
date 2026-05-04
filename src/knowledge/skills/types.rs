use std::fmt;

use vfs::VfsPath;

use crate::knowledge::skills::errors::SkillError;

const SKILL_NAME_MAX: usize = 64;
const SKILL_DESCRIPTION_MAX: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillName(String);

impl SkillName {
    pub fn try_from(raw: &str) -> Result<Self, SkillError> {
        if raw.is_empty() {
            return Err(SkillError::InvalidName {
                raw: raw.to_string(),
                reason: "must be non-empty".to_string(),
            });
        }
        if raw.len() > SKILL_NAME_MAX {
            return Err(SkillError::InvalidName {
                raw: raw.to_string(),
                reason: format!("must be {SKILL_NAME_MAX} characters or fewer"),
            });
        }
        if !raw
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(SkillError::InvalidName {
                raw: raw.to_string(),
                reason: "must contain only lowercase ASCII letters, digits, and `-`".to_string(),
            });
        }
        Ok(Self(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SkillName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDescription(String);

impl SkillDescription {
    pub fn try_from(raw: &str) -> Result<Self, SkillError> {
        if raw.is_empty() {
            return Err(SkillError::InvalidDescription {
                reason: "must be non-empty".to_string(),
            });
        }
        if raw.len() > SKILL_DESCRIPTION_MAX {
            return Err(SkillError::InvalidDescription {
                reason: format!("must be {SKILL_DESCRIPTION_MAX} characters or fewer"),
            });
        }
        Ok(Self(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillBody(String);

impl SkillBody {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct SkillSource(pub VfsPath);

impl SkillSource {
    pub fn as_path(&self) -> &VfsPath {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct Skill {
    pub name: SkillName,
    pub description: SkillDescription,
    pub body: SkillBody,
    pub source: SkillSource,
}

#[derive(serde::Deserialize)]
struct SkillFrontmatter {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

impl Skill {
    /// Parse a `SKILL.md` body. The first non-empty line must be `---`,
    /// followed by YAML, followed by a closing `---`. The body is
    /// everything after the closing fence with leading newlines trimmed.
    pub fn parse(
        source: SkillSource,
        raw: &str,
        expected_name: &SkillName,
    ) -> Result<Skill, SkillError> {
        let (yaml, body) = split_frontmatter(raw, expected_name)?;

        let frontmatter: SkillFrontmatter =
            serde_yml::from_str(yaml).map_err(|source| SkillError::FrontmatterParse {
                name: expected_name.clone(),
                source,
            })?;

        let name_raw = frontmatter
            .name
            .ok_or(SkillError::FrontmatterMissing {
                name: expected_name.clone(),
                field: "name",
            })?;
        let description_raw =
            frontmatter
                .description
                .ok_or(SkillError::FrontmatterMissing {
                    name: expected_name.clone(),
                    field: "description",
                })?;

        let parsed_name =
            SkillName::try_from(&name_raw).map_err(|err| match err {
                SkillError::InvalidName { reason, .. } => SkillError::FrontmatterInvalid {
                    name: expected_name.clone(),
                    field: "name",
                    reason,
                },
                other => other,
            })?;

        if parsed_name != *expected_name {
            return Err(SkillError::NameMismatch {
                expected: expected_name.clone(),
                found: name_raw,
            });
        }

        let description = SkillDescription::try_from(description_raw.trim()).map_err(|err| {
            match err {
                SkillError::InvalidDescription { reason } => SkillError::FrontmatterInvalid {
                    name: expected_name.clone(),
                    field: "description",
                    reason,
                },
                other => other,
            }
        })?;

        Ok(Skill {
            name: parsed_name,
            description,
            body: SkillBody(body.trim_end().to_string()),
            source,
        })
    }
}

fn split_frontmatter<'a>(
    raw: &'a str,
    expected_name: &SkillName,
) -> Result<(&'a str, &'a str), SkillError> {
    let trimmed_start = raw.trim_start_matches(['\u{feff}', '\n', '\r']);
    let after_open = trimmed_start
        .strip_prefix("---\n")
        .or_else(|| trimmed_start.strip_prefix("---\r\n"))
        .ok_or(SkillError::FrontmatterMissing {
            name: expected_name.clone(),
            field: "---",
        })?;

    let close_idx = find_closing_fence(after_open).ok_or(SkillError::FrontmatterMissing {
        name: expected_name.clone(),
        field: "---",
    })?;
    let yaml = &after_open[..close_idx];

    let after_yaml = &after_open[close_idx..];
    let after_close = after_yaml
        .strip_prefix("---\n")
        .or_else(|| after_yaml.strip_prefix("---\r\n"))
        .or_else(|| after_yaml.strip_prefix("---"))
        .unwrap_or(after_yaml);

    let body = after_close.trim_start_matches(['\n', '\r']);
    Ok((yaml, body))
}

fn find_closing_fence(after_open: &str) -> Option<usize> {
    let mut start = 0usize;
    while let Some(rel) = after_open[start..].find("---") {
        let abs = start + rel;
        let preceded_by_newline = abs == 0 || matches!(after_open.as_bytes()[abs - 1], b'\n');
        let after = &after_open[abs + 3..];
        let followed_ok = after.is_empty()
            || after.starts_with('\n')
            || after.starts_with("\r\n");
        if preceded_by_newline && followed_ok {
            return Some(abs);
        }
        start = abs + 3;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem_fs;

    fn dummy_source() -> SkillSource {
        let fs = mem_fs! { "x": { "SKILL.md": "" } };
        SkillSource(fs.join("x/SKILL.md").unwrap())
    }

    #[test]
    fn skill_name_accepts_lowercase_alphanumeric_dash() {
        let n = SkillName::try_from("foo-bar-2").unwrap();
        assert_eq!(n.as_str(), "foo-bar-2");
    }

    #[test]
    fn skill_name_rejects_uppercase() {
        let err = SkillName::try_from("Foo").unwrap_err();
        assert!(
            matches!(err, SkillError::InvalidName { .. }),
            "expected InvalidName, got {err:?}"
        );
    }

    #[test]
    fn skill_name_rejects_empty() {
        let err = SkillName::try_from("").unwrap_err();
        assert!(matches!(err, SkillError::InvalidName { .. }));
    }

    #[test]
    fn skill_name_rejects_over_64_chars() {
        let raw = "a".repeat(65);
        let err = SkillName::try_from(&raw).unwrap_err();
        assert!(matches!(err, SkillError::InvalidName { .. }));
    }

    #[test]
    fn skill_description_rejects_empty() {
        let err = SkillDescription::try_from("").unwrap_err();
        assert!(matches!(err, SkillError::InvalidDescription { .. }));
    }

    #[test]
    fn skill_description_rejects_over_1024_chars() {
        let raw = "a".repeat(1025);
        let err = SkillDescription::try_from(&raw).unwrap_err();
        assert!(matches!(err, SkillError::InvalidDescription { .. }));
    }

    #[test]
    fn parse_strips_frontmatter_and_returns_body() {
        let expected = SkillName::try_from("foo").unwrap();
        let raw = "---\nname: foo\ndescription: a foo skill\n---\nFOO BODY\n";
        let skill = Skill::parse(dummy_source(), raw, &expected).unwrap();
        assert_eq!(skill.name.as_str(), "foo");
        assert_eq!(skill.description.as_str(), "a foo skill");
        assert_eq!(skill.body.as_str(), "FOO BODY");
    }

    #[test]
    fn parse_rejects_missing_name_field() {
        let expected = SkillName::try_from("foo").unwrap();
        let raw = "---\ndescription: only desc\n---\nbody\n";
        let err = Skill::parse(dummy_source(), raw, &expected).unwrap_err();
        assert!(
            matches!(err, SkillError::FrontmatterMissing { field: "name", .. }),
            "expected FrontmatterMissing(name), got {err:?}"
        );
    }

    #[test]
    fn parse_rejects_missing_description_field() {
        let expected = SkillName::try_from("foo").unwrap();
        let raw = "---\nname: foo\n---\nbody\n";
        let err = Skill::parse(dummy_source(), raw, &expected).unwrap_err();
        assert!(
            matches!(
                err,
                SkillError::FrontmatterMissing {
                    field: "description",
                    ..
                }
            ),
            "expected FrontmatterMissing(description), got {err:?}"
        );
    }

    #[test]
    fn parse_rejects_name_mismatch_with_directory() {
        let expected = SkillName::try_from("foo").unwrap();
        let raw = "---\nname: bar\ndescription: x\n---\nbody\n";
        let err = Skill::parse(dummy_source(), raw, &expected).unwrap_err();
        assert!(
            matches!(err, SkillError::NameMismatch { .. }),
            "expected NameMismatch, got {err:?}"
        );
    }

    #[test]
    fn parse_rejects_invalid_yaml() {
        let expected = SkillName::try_from("foo").unwrap();
        let raw = "---\nname: foo\n  description: : :\n---\nbody\n";
        let err = Skill::parse(dummy_source(), raw, &expected).unwrap_err();
        assert!(
            matches!(err, SkillError::FrontmatterParse { .. }),
            "expected FrontmatterParse, got {err:?}"
        );
    }

    #[test]
    fn parse_rejects_missing_open_fence() {
        let expected = SkillName::try_from("foo").unwrap();
        let raw = "name: foo\ndescription: x\nbody\n";
        let err = Skill::parse(dummy_source(), raw, &expected).unwrap_err();
        assert!(
            matches!(err, SkillError::FrontmatterMissing { field: "---", .. }),
            "expected FrontmatterMissing(---), got {err:?}"
        );
    }
}
