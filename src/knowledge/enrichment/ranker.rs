//! [`CompositeEnricher`] composes the namespace pass and the grep pass.
//!
//! `enrich` returns the declared skills first (origin
//! [`EnrichmentOrigin::Declared`], in declared order), then the
//! plugin-namespace siblings and bootstrap allow-list (origin
//! [`EnrichmentOrigin::Namespace`], alphabetical), then any remaining
//! skills the body-grep ranker scores above the threshold (origin
//! [`EnrichmentOrigin::Grep`], score-then-name order). Each skill appears
//! at most once; the strongest provenance wins (declared > namespace >
//! grep).

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::grep::grep_pass;
use super::namespace::namespace_pass;
use super::{
    EnrichedSkill, EnrichmentError, EnrichmentOrigin, SkillEnricher, knowledge_source_path,
};
use crate::knowledge::base::{KnowledgeBase, KnowledgeError};
use crate::knowledge::skills::SkillName;

#[derive(Debug, Default, Clone)]
pub struct CompositeEnricher;

impl CompositeEnricher {
    pub fn new() -> Self {
        Self
    }
}

impl SkillEnricher for CompositeEnricher {
    fn enrich(
        &self,
        declared: &[SkillName],
        prompt: &str,
        kb: &dyn KnowledgeBase,
    ) -> Result<Vec<EnrichedSkill>, EnrichmentError> {
        let mut out: Vec<EnrichedSkill> = Vec::with_capacity(declared.len());
        let mut picked: BTreeSet<String> = BTreeSet::new();

        let mut declared_bodies: Vec<String> = Vec::with_capacity(declared.len());
        for name in declared {
            let skill = match kb.skill(name) {
                Ok(s) => s,
                Err(KnowledgeError::Missing { .. }) => continue,
                Err(other) => return Err(other.into()),
            };
            declared_bodies.push(skill.body().as_str().to_string());
            if picked.insert(skill.name().as_str().to_string()) {
                out.push(EnrichedSkill {
                    name: skill.name().clone(),
                    origin: EnrichmentOrigin::Declared,
                    score: None,
                    source: knowledge_source_path(skill.source()),
                });
            }
        }

        for entry in namespace_pass(declared, kb)? {
            if picked.insert(entry.name.as_str().to_string()) {
                out.push(entry);
            }
        }

        let summaries = kb.list_skills()?;
        let mut candidates: Vec<(SkillName, String, PathBuf)> = Vec::new();
        for summary in &summaries {
            if picked.contains(summary.name().as_str()) {
                continue;
            }
            let skill = match kb.skill(summary.name()) {
                Ok(s) => s,
                Err(KnowledgeError::Missing { .. }) => continue,
                Err(other) => return Err(other.into()),
            };
            candidates.push((
                skill.name().clone(),
                skill.body().as_str().to_string(),
                knowledge_source_path(skill.source()),
            ));
        }

        for entry in grep_pass(&declared_bodies, prompt, &candidates) {
            if picked.insert(entry.name.as_str().to_string()) {
                out.push(entry);
            }
        }

        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use super::super::grep::grep_pass;
    use super::super::namespace::namespace_pass;
    use super::*;
    use crate::knowledge::base::{
        AgentsDoc, KnowledgeBase, KnowledgeError, KnowledgeHit, KnowledgeKind, KnowledgeSource,
        SkillSummary, WorkflowName, WorkflowSummary,
    };
    use crate::knowledge::skills::{Skill, SkillName};
    use crate::mem_fs;
    use crate::project::KnowledgeRoot;
    use crate::workflow::Workflow;
    use vfs::VfsPath;

    struct TestKb {
        root: KnowledgeRoot,
        fs: VfsPath,
        skills: HashMap<SkillName, Skill>,
    }

    impl TestKb {
        fn new() -> Self {
            let fs = mem_fs! { "kb": {} };
            let root = KnowledgeRoot::try_from(fs.join("kb").unwrap()).unwrap();
            Self {
                root,
                fs,
                skills: HashMap::new(),
            }
        }

        fn with_skill(&mut self, name: &str, description: &str, body: &str) -> &mut Self {
            let skill_name = SkillName::try_from(name).unwrap();
            let path_safe = name.replace(':', "_");
            let path = self
                .fs
                .join(format!("kb/skills/{path_safe}/SKILL.md"))
                .unwrap();
            let raw = format!("---\nname: {name}\ndescription: {description}\n---\n{body}\n");
            let source = KnowledgeSource {
                root: self.root.clone(),
                path,
            };
            let skill = Skill::parse(source, &raw, &skill_name).unwrap();
            self.skills.insert(skill_name, skill);
            self
        }
    }

    impl KnowledgeBase for TestKb {
        fn skill(&self, name: &SkillName) -> Result<Skill, KnowledgeError> {
            self.skills
                .get(name)
                .cloned()
                .ok_or_else(|| KnowledgeError::Missing {
                    kind: KnowledgeKind::Skill,
                    name: name.as_str().to_string(),
                    search_paths: vec!["<test>".to_string()],
                })
        }
        fn workflow(&self, name: &WorkflowName) -> Result<Workflow, KnowledgeError> {
            Err(KnowledgeError::Missing {
                kind: KnowledgeKind::Workflow,
                name: name.as_str().to_string(),
                search_paths: vec!["<test>".to_string()],
            })
        }
        fn agents(&self) -> Result<Vec<AgentsDoc>, KnowledgeError> {
            Ok(Vec::new())
        }
        fn list_skills(&self) -> Result<Vec<SkillSummary>, KnowledgeError> {
            Ok(self.skills.values().map(SkillSummary::from).collect())
        }
        fn list_workflows(&self) -> Result<Vec<WorkflowSummary>, KnowledgeError> {
            Ok(Vec::new())
        }
        fn search(&self, _query: &str) -> Result<Vec<KnowledgeHit>, KnowledgeError> {
            Ok(Vec::new())
        }
    }

    fn skill_name(raw: &str) -> SkillName {
        SkillName::try_from(raw).unwrap()
    }

    #[test]
    fn composite_returns_declared_first_in_declared_order() {
        let mut kb = TestKb::new();
        kb.with_skill("dev:design", "design skill", "design body")
            .with_skill("dev:thinking", "thinking skill", "thinking body");

        let declared = vec![skill_name("dev:thinking"), skill_name("dev:design")];
        let result = CompositeEnricher::new().enrich(&declared, "", &kb).unwrap();

        let head: Vec<&str> = result.iter().take(2).map(|e| e.name.as_str()).collect();
        assert_eq!(head, vec!["dev:thinking", "dev:design"]);
        assert!(
            result[..2]
                .iter()
                .all(|e| e.origin == EnrichmentOrigin::Declared)
        );
    }

    #[test]
    fn namespace_pass_includes_same_plugin_siblings_alphabetically() {
        let mut kb = TestKb::new();
        kb.with_skill("dev:design", "design", "x")
            .with_skill("dev:thinking", "thinking", "x")
            .with_skill("dev:using-dev", "using-dev", "x")
            .with_skill("other:elsewhere", "other", "x");

        let declared = vec![skill_name("dev:design")];
        let result = namespace_pass(&declared, &kb).unwrap();

        let names: Vec<&str> = result.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"dev:thinking"));
        assert!(names.contains(&"dev:using-dev"));
        assert!(!names.contains(&"other:elsewhere"));
        assert!(!names.contains(&"dev:design"));
        let dev_only: Vec<&str> = names
            .iter()
            .copied()
            .filter(|n| n.starts_with("dev:"))
            .collect();
        let mut sorted = dev_only.clone();
        sorted.sort();
        assert_eq!(dev_only, sorted);
        assert!(
            result
                .iter()
                .all(|e| e.origin == EnrichmentOrigin::Namespace)
        );
    }

    #[test]
    fn namespace_pass_includes_bootstrap_allow_list_when_present() {
        let mut kb = TestKb::new();
        kb.with_skill("dev:design", "d", "x")
            .with_skill("general:using-general", "g", "x")
            .with_skill("patterns:using-patterns", "p", "x")
            .with_skill("characters:using-characters", "c", "x");

        let declared = vec![skill_name("dev:design")];
        let result = namespace_pass(&declared, &kb).unwrap();

        let names: Vec<&str> = result.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"general:using-general"));
        assert!(names.contains(&"patterns:using-patterns"));
        assert!(names.contains(&"characters:using-characters"));
    }

    #[test]
    fn namespace_pass_skips_bootstrap_entries_missing_from_kb() {
        let mut kb = TestKb::new();
        kb.with_skill("dev:design", "d", "x")
            .with_skill("general:using-general", "g", "x");

        let declared = vec![skill_name("dev:design")];
        let result = namespace_pass(&declared, &kb).unwrap();

        let names: Vec<&str> = result.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"general:using-general"));
        assert!(!names.contains(&"patterns:using-patterns"));
        assert!(!names.contains(&"characters:using-characters"));
    }

    #[test]
    fn grep_pass_omits_skills_below_threshold() {
        let declared_bodies = vec!["alpha beta gamma delta epsilon zeta".to_string()];
        let candidates = vec![(
            skill_name("low"),
            "alpha beta unrelated nothing".to_string(),
            PathBuf::from("/x/low"),
        )];
        let result = grep_pass(&declared_bodies, "", &candidates);
        assert!(result.is_empty(), "expected empty, got {result:?}");
    }

    #[test]
    fn grep_pass_orders_above_threshold_by_score_then_name() {
        let declared_bodies = vec!["alpha beta gamma delta epsilon".to_string()];
        let candidates = vec![
            (
                skill_name("zebra"),
                "alpha beta gamma extraword anotherword".to_string(),
                PathBuf::from("/z"),
            ),
            (
                skill_name("apple"),
                "alpha beta gamma delta extra".to_string(),
                PathBuf::from("/a"),
            ),
            (
                skill_name("banana"),
                "alpha beta gamma delta epsilon".to_string(),
                PathBuf::from("/b"),
            ),
        ];

        let result = grep_pass(&declared_bodies, "", &candidates);
        let names: Vec<&str> = result.iter().map(|e| e.name.as_str()).collect();

        assert_eq!(names.first().copied(), Some("banana"));
        assert!(result.windows(2).all(|w| {
            let sa = w[0].score.unwrap_or(0.0);
            let sb = w[1].score.unwrap_or(0.0);
            sa > sb || (sa == sb && w[0].name.as_str() < w[1].name.as_str())
        }));
    }

    #[test]
    fn grep_pass_score_is_matches_over_body_tokens_two_decimals() {
        let declared_bodies = vec!["alpha beta gamma".to_string()];
        let candidates = vec![(
            skill_name("c"),
            "alpha beta gamma delta epsilon zeta eta theta iota".to_string(),
            PathBuf::from("/c"),
        )];
        let result = grep_pass(&declared_bodies, "", &candidates);
        assert_eq!(result.len(), 1);
        let score = result[0].score.unwrap();
        let expected = (3.0_f32 / 9.0_f32 * 100.0).round() / 100.0;
        assert!(
            (score - expected).abs() < 1e-4,
            "score {score} != expected {expected}"
        );
        let s_str = format!("{score}");
        assert!(
            s_str.split('.').nth(1).map(str::len).unwrap_or(0) <= 2,
            "score must be 2-decimal: {s_str}"
        );
    }

    #[test]
    fn composite_is_deterministic_across_invocations() {
        let mut kb = TestKb::new();
        kb.with_skill("dev:design", "d", "alpha beta gamma delta")
            .with_skill("dev:thinking", "t", "alpha beta gamma delta")
            .with_skill("general:using-general", "g", "alpha beta gamma delta");

        let declared = vec![skill_name("dev:design")];

        let a = CompositeEnricher::new()
            .enrich(&declared, "alpha", &kb)
            .unwrap();
        let b = CompositeEnricher::new()
            .enrich(&declared, "alpha", &kb)
            .unwrap();

        let names_a: Vec<&str> = a.iter().map(|e| e.name.as_str()).collect();
        let names_b: Vec<&str> = b.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names_a, names_b);
    }
}
