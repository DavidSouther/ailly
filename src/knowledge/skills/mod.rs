mod errors;
mod repository;
mod types;

pub use errors::SkillError;
pub use repository::{FsSkillRepository, SkillRepository};
pub use types::{Skill, SkillBody, SkillDescription, SkillName, SkillSource};

/// A `SkillRepository` that fails every lookup. Useful as a placeholder
/// when a caller has no skills configured and any `skills =` declaration
/// in a `.ailly.toml` should surface as a missing-skill error rather than
/// silently succeed.
pub struct NullSkillRepository;

impl SkillRepository for NullSkillRepository {
    fn get(&self, name: &SkillName) -> Result<Skill, SkillError> {
        Err(SkillError::Missing {
            name: name.clone(),
            search_path: "<no skill repository configured>".to_string(),
        })
    }
}
