//! Body-grep ranker for [`CompositeEnricher`].
//!
//! Scores every non-namespace candidate skill body by token-overlap with
//! the union of declared skill bodies plus the prompt. Tokens are
//! whitespace split, lowercased, deduped, with the fixed stop-word list
//! removed. Skills with at least [`THRESHOLD`] matching tokens join the
//! list, origin [`EnrichmentOrigin::Grep`], score = `matches / body_tokens`
//! rounded to two decimal places. Ties broken by skill name.

use std::collections::BTreeSet;
use std::path::PathBuf;

use super::{EnrichedSkill, EnrichmentOrigin};
use crate::knowledge::skills::SkillName;

pub(super) const THRESHOLD: usize = 3;

pub(super) const STOP_WORDS: &[&str] = &[
    "the", "a", "an", "is", "are", "was", "were", "be", "been", "being", "to", "of", "and", "or",
    "in", "on", "for", "with", "as", "at", "by", "from", "that", "this", "it", "its", "into",
    "then", "than", "but", "if", "when", "while", "you", "your", "our", "we", "they", "them",
    "their", "not", "no", "so", "do", "does", "doing", "has", "have", "had",
];

pub(super) fn grep_pass(
    declared_bodies: &[String],
    prompt: &str,
    candidates: &[(SkillName, String, PathBuf)],
) -> Vec<EnrichedSkill> {
    let stop: BTreeSet<&str> = STOP_WORDS.iter().copied().collect();

    let mut declared_tokens: BTreeSet<String> = BTreeSet::new();
    for body in declared_bodies {
        for tok in tokenize(body, &stop) {
            declared_tokens.insert(tok);
        }
    }
    for tok in tokenize(prompt, &stop) {
        declared_tokens.insert(tok);
    }

    let mut scored: Vec<EnrichedSkill> = Vec::new();
    for (name, body, source) in candidates {
        let body_tokens: BTreeSet<String> = tokenize(body, &stop).into_iter().collect();
        if body_tokens.is_empty() {
            continue;
        }
        let matches = body_tokens.intersection(&declared_tokens).count();
        if matches < THRESHOLD {
            continue;
        }
        let raw = matches as f32 / body_tokens.len() as f32;
        let score = (raw * 100.0).round() / 100.0;
        scored.push(EnrichedSkill {
            name: name.clone(),
            origin: EnrichmentOrigin::Grep,
            score: Some(score),
            source: source.clone(),
        });
    }

    scored.sort_by(|a, b| {
        let sa = a.score.unwrap_or(0.0);
        let sb = b.score.unwrap_or(0.0);
        sb.partial_cmp(&sa)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.as_str().cmp(b.name.as_str()))
    });

    scored
}

fn tokenize(text: &str, stop: &BTreeSet<&str>) -> Vec<String> {
    text.split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_ascii_alphanumeric())
                .to_ascii_lowercase()
        })
        .filter(|w| !w.is_empty() && !stop.contains(w.as_str()))
        .collect()
}
