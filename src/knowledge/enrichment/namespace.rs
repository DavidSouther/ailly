//! Plugin-namespace pass for [`CompositeEnricher`].
//!
//! For every declared skill named `<plugin>:<name>`, every other skill in
//! the same `<plugin>` namespace and every skill in the fixed bootstrap
//! allow-list joins the enriched list. Ordering is alphabetical by skill
//! name within each pass; declared skills retain their declared order at
//! the head of the returned list (the composite enricher prepends them
//! first, so this pass omits them). Score is `None`. Origin is
//! [`EnrichmentOrigin::Namespace`].

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::{EnrichedSkill, EnrichmentError, EnrichmentOrigin, knowledge_source_path};
use crate::knowledge::base::KnowledgeBase;
use crate::knowledge::skills::SkillName;

pub(super) const BOOTSTRAP: &[&str] = &[
    "general:using-general",
    "patterns:using-patterns",
    "characters:using-characters",
];

pub(super) fn namespace_pass(
    declared: &[SkillName],
    kb: &dyn KnowledgeBase,
) -> Result<Vec<EnrichedSkill>, EnrichmentError> {
    let summaries = kb.list_skills()?;

    let declared_set: BTreeSet<&str> = declared.iter().map(SkillName::as_str).collect();

    let plugins: BTreeSet<&str> = declared.iter().filter_map(SkillName::plugin).collect();

    let mut picked: BTreeSet<String> = BTreeSet::new();
    let mut entries: Vec<(SkillName, PathBuf)> = Vec::new();

    for plugin in &plugins {
        let mut siblings: Vec<&_> = summaries
            .iter()
            .filter(|s| s.name().plugin() == Some(*plugin))
            .filter(|s| !declared_set.contains(s.name().as_str()))
            .collect();
        siblings.sort_by(|a, b| a.name().as_str().cmp(b.name().as_str()));
        for sib in siblings {
            if picked.insert(sib.name().as_str().to_string()) {
                entries.push((sib.name().clone(), knowledge_source_path(sib.source())));
            }
        }
    }

    for raw in BOOTSTRAP {
        if declared_set.contains(*raw) || picked.contains(*raw) {
            continue;
        }
        let Ok(name) = SkillName::try_from(raw) else {
            continue;
        };
        let Some(summary) = summaries.iter().find(|s| s.name() == &name) else {
            continue;
        };
        if picked.insert(raw.to_string()) {
            entries.push((
                summary.name().clone(),
                knowledge_source_path(summary.source()),
            ));
        }
    }

    Ok(entries
        .into_iter()
        .map(|(name, source)| EnrichedSkill {
            name,
            origin: EnrichmentOrigin::Namespace,
            score: None,
            source,
        })
        .collect())
}
