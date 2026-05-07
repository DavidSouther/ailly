//! Skill enrichment: composes declared skills with related skills the
//! workflow runtime should attach to a Conversation before the slow
//! inference runs.
//!
//! The runtime calls a [`SkillEnricher`] with the turn's declared skill
//! list, the user prompt, and the loaded [`KnowledgeBase`]. The result is
//! one [`EnrichedSkill`] per skill that should appear on the Conversation
//! preamble, each carrying its [`EnrichmentOrigin`] for the audit trail
//! that lands on the persisted turn TOML's `[enrichment]` table.
//!
//! See `docs/developer/2026-05-07-A-thinking-fast-slow/design.md`.

use std::path::PathBuf;

use crate::knowledge::base::{KnowledgeBase, KnowledgeError, KnowledgeSource};
use crate::knowledge::skills::SkillName;

pub mod grep;
pub mod namespace;
pub mod ranker;

pub use ranker::CompositeEnricher;

/// Composes declared skills with related skills the runtime should attach
/// to a Conversation before the slow inference runs. Implementations must
/// be deterministic given (declared, prompt, kb).
pub trait SkillEnricher: Send + Sync {
    fn enrich(
        &self,
        declared: &[SkillName],
        prompt: &str,
        kb: &dyn KnowledgeBase,
    ) -> Result<Vec<EnrichedSkill>, EnrichmentError>;
}

/// One skill the enricher selected for a turn. Carries provenance so the
/// `[enrichment]` audit row can record why the skill is present.
#[derive(Debug, Clone, PartialEq)]
pub struct EnrichedSkill {
    pub name: SkillName,
    pub origin: EnrichmentOrigin,
    pub score: Option<f32>,
    pub source: PathBuf,
}

/// Why an [`EnrichedSkill`] was added. `Declared` preserves author
/// intent; `Namespace` is the same-plugin plus bootstrap allow-list
/// pass; `Grep` is the body-token-overlap ranker. The variant is the
/// audit primary key for the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EnrichmentOrigin {
    Declared,
    Namespace,
    Grep,
}

impl EnrichmentOrigin {
    /// Lowercase string the on-disk `[[enrichment.skills]]` row uses.
    pub fn as_str(&self) -> &'static str {
        match self {
            EnrichmentOrigin::Declared => "declared",
            EnrichmentOrigin::Namespace => "namespace",
            EnrichmentOrigin::Grep => "grep",
        }
    }
}

/// Combine a [`KnowledgeRoot`]'s display path with an in-root VFS path
/// into a [`PathBuf`] suitable for the audit record. For `PhysicalFS`
/// roots the display path is the user's native path and the inner path is
/// rooted at `/`, so joining yields the native path. For `MemoryFS` roots
/// used in tests the inner path already includes the root prefix and is
/// returned verbatim. The result is canonicalized to absolute when the
/// underlying file exists; otherwise the joined form is returned as-is.
pub(crate) fn knowledge_source_path(source: &KnowledgeSource) -> PathBuf {
    let display = source.root.display_path();
    let inner = source.path.as_str();
    let combined = if inner.starts_with(display) {
        PathBuf::from(inner)
    } else {
        let stripped = inner.trim_start_matches('/');
        if stripped.is_empty() {
            PathBuf::from(display)
        } else {
            PathBuf::from(display).join(stripped)
        }
    };
    std::fs::canonicalize(&combined).unwrap_or(combined)
}

#[derive(Debug, thiserror::Error)]
pub enum EnrichmentError {
    #[error(transparent)]
    Knowledge(#[from] KnowledgeError),

    #[error("enrichment referenced unknown skill {0}")]
    UnknownSkill(SkillName),
}
