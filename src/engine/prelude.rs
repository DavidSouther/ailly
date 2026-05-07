//! Prelude conversion at the engine boundary.
//!
//! This module owns the typed handle the engine adapter uses to turn a
//! conversation `Preamble` plus an advertised tool slice into the wire
//! messages sent to the provider, and the audit record persisted onto the
//! turn file after the engine finishes.
//!
//! Three independent concerns share one module:
//!
//! - `PreludeSource` is a typed tag distinguishing inherited system text,
//!   skill bodies (named), local system text, and the synthetic tools
//!   exchange. The `Anthropic cache_control: ephemeral` follow-on slice
//!   consumes this tag to attach cache hints to skill exchanges without
//!   re-parsing strings.
//! - `PreludeExchange` is a single synthetic two-message exchange (one
//!   user-side rendering of the block, one hardcoded assistant ack). The
//!   producer fixes the order; this type does not enforce ordering itself.
//! - `SentEnvelope` is the complete record of what the adapter sent: the
//!   ordered prelude exchanges and the tool definitions advertised. Yielded
//!   by every `Engine::stream` impl as `EngineEvent::Envelope` before any
//!   text or tool event, persisted to the turn file by the generator before
//!   the final write. Reload repopulates a typed `SentEnvelope` for
//!   inspection but does not feed it back into the engine input.
//!
//! Invariant: every `PreambleBlock` produces exactly one `PreludeExchange`.
//! Empty `prelude` and empty `tools` together mean there is no envelope
//! to record on disk.

use rig::completion::request::ToolDefinition;
use rig::message::Message;

use crate::content::{Preamble, PreambleBlock};

#[derive(Debug, Clone)]
pub enum PreludeSource {
    InheritedSystem,
    Skill { name: String },
    LocalSystem,
    Tools,
}

#[derive(Debug, Clone)]
pub struct PreludeExchange {
    pub user: Message,
    pub assistant: Message,
    pub source: PreludeSource,
}

#[derive(Debug, Clone, Default)]
pub struct SentEnvelope {
    pub prelude: Vec<PreludeExchange>,
    pub tools: Vec<ToolDefinition>,
}

/// Total conversion. Walks the `Preamble` in walk-then-declaration order
/// as produced by the content layer. Every `PreambleBlock` produces
/// exactly one `PreludeExchange`. The user-side rendering is bare text
/// for system blocks and `## {name}\n\n{body}` for skills. Ack content
/// is hardcoded by source per the table in `design.md`.
impl From<&Preamble> for Vec<PreludeExchange> {
    fn from(p: &Preamble) -> Self {
        p.blocks
            .iter()
            .map(|block| match block {
                PreambleBlock::InheritedSystem { text } => PreludeExchange {
                    user: Message::user(text.clone()),
                    assistant: Message::assistant(hardcoded_ack(&PreludeSource::InheritedSystem)),
                    source: PreludeSource::InheritedSystem,
                },
                PreambleBlock::LocalSystem { text } => PreludeExchange {
                    user: Message::user(text.clone()),
                    assistant: Message::assistant(hardcoded_ack(&PreludeSource::LocalSystem)),
                    source: PreludeSource::LocalSystem,
                },
                PreambleBlock::Skill(skill) => {
                    let name = skill.name().as_str().to_string();
                    let body = skill.body().as_str();
                    let source = PreludeSource::Skill { name: name.clone() };
                    PreludeExchange {
                        user: Message::user(format!("## {name}\n\n{body}")),
                        assistant: Message::assistant(hardcoded_ack(&source)),
                        source,
                    }
                }
            })
            .collect()
    }
}

/// Build the synthetic `Tools` exchange. Sorts by name. Renders the user
/// side as `## Available tools\n\n- {name}: {description}`. Returns
/// `None` when the slice is empty so the caller can append-or-skip
/// without an extra check.
pub fn tools_exchange(tools: &[ToolDefinition]) -> Option<PreludeExchange> {
    if tools.is_empty() {
        return None;
    }
    let mut sorted: Vec<&ToolDefinition> = tools.iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    let lines: Vec<String> = sorted
        .iter()
        .map(|t| format!("- {}: {}", t.name, t.description))
        .collect();
    let user_text = format!("## Available tools\n\n{}", lines.join("\n"));
    Some(PreludeExchange {
        user: Message::user(user_text),
        assistant: Message::assistant(hardcoded_ack(&PreludeSource::Tools)),
        source: PreludeSource::Tools,
    })
}

/// Hardcoded ack text per source. Pure function. The seam where a future
/// `PreludeAckProvider` trait would slot in if dynamic acks become
/// useful.
fn hardcoded_ack(source: &PreludeSource) -> String {
    match source {
        PreludeSource::InheritedSystem => {
            "Understood. I will follow the inherited instructions.".to_string()
        }
        PreludeSource::Skill { name } => {
            format!("Understood. I will apply the {name} skill when relevant.")
        }
        PreludeSource::LocalSystem => {
            "Understood. I will follow the local instructions.".to_string()
        }
        PreludeSource::Tools => {
            "Understood. I will use the listed tools when relevant.".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use rig::message::{AssistantContent, UserContent};
    use serde_json::json;

    use crate::content::PreambleBlock;
    use crate::knowledge::base::KnowledgeSource;
    use crate::knowledge::skills::{Skill, SkillName};
    use crate::mem_fs;
    use crate::project::KnowledgeRoot;

    fn dummy_skill(name: &str, body: &str) -> Skill {
        let fs = mem_fs! { "skills": { "SKILL.md": "" } };
        let source = KnowledgeSource {
            root: KnowledgeRoot::try_from(fs.clone()).expect("knowledge root"),
            path: fs.join("skills/SKILL.md").unwrap(),
        };
        let parsed_name = SkillName::try_from(name).unwrap();
        let raw = format!("---\nname: {name}\ndescription: a {name} skill\n---\n{body}\n");
        Skill::parse(source, &raw, &parsed_name).unwrap()
    }

    fn user_text(message: &Message) -> String {
        match message {
            Message::User { content } => match content.first() {
                UserContent::Text(t) => t.text.clone(),
                other => panic!("expected user text content, got {other:?}"),
            },
            other => panic!("expected User message, got {other:?}"),
        }
    }

    fn assistant_text(message: &Message) -> String {
        match message {
            Message::Assistant { content, .. } => match content.first() {
                AssistantContent::Text(t) => t.text.clone(),
                other => panic!("expected assistant text content, got {other:?}"),
            },
            other => panic!("expected Assistant message, got {other:?}"),
        }
    }

    #[test]
    fn prelude_from_preamble_emits_one_exchange_per_block() {
        let preamble = Preamble {
            blocks: vec![
                PreambleBlock::InheritedSystem {
                    text: "INHERITED".to_string(),
                },
                PreambleBlock::Skill(dummy_skill("foo", "FOO BODY")),
                PreambleBlock::LocalSystem {
                    text: "LOCAL".to_string(),
                },
            ],
        };

        let exchanges: Vec<PreludeExchange> = (&preamble).into();

        assert_eq!(exchanges.len(), 3);
        assert!(matches!(exchanges[0].source, PreludeSource::InheritedSystem));
        assert!(
            matches!(&exchanges[1].source, PreludeSource::Skill { name } if name == "foo"),
            "expected Skill source for block 1"
        );
        assert!(matches!(exchanges[2].source, PreludeSource::LocalSystem));

        assert_eq!(user_text(&exchanges[0].user), "INHERITED");
        assert_eq!(user_text(&exchanges[1].user), "## foo\n\nFOO BODY");
        assert_eq!(user_text(&exchanges[2].user), "LOCAL");
    }

    #[test]
    fn prelude_exchange_acks_match_source_table() {
        let preamble = Preamble {
            blocks: vec![
                PreambleBlock::InheritedSystem {
                    text: "I".to_string(),
                },
                PreambleBlock::Skill(dummy_skill("echo", "BODY")),
                PreambleBlock::LocalSystem {
                    text: "L".to_string(),
                },
            ],
        };

        let exchanges: Vec<PreludeExchange> = (&preamble).into();

        assert_eq!(
            assistant_text(&exchanges[0].assistant),
            "Understood. I will follow the inherited instructions."
        );
        assert_eq!(
            assistant_text(&exchanges[1].assistant),
            "Understood. I will apply the echo skill when relevant."
        );
        assert_eq!(
            assistant_text(&exchanges[2].assistant),
            "Understood. I will follow the local instructions."
        );

        let tools = vec![ToolDefinition {
            name: "echo".to_string(),
            description: "echoes input".to_string(),
            parameters: json!({"type": "object"}),
        }];
        let tools_exchange =
            tools_exchange(&tools).expect("non-empty tool slice should produce an exchange");
        assert_eq!(
            assistant_text(&tools_exchange.assistant),
            "Understood. I will use the listed tools when relevant."
        );
    }

    #[test]
    fn tools_exchange_sorts_by_name_and_renders_definitions() {
        let tools = vec![
            ToolDefinition {
                name: "zeta".to_string(),
                description: "z tool".to_string(),
                parameters: json!({"type": "object"}),
            },
            ToolDefinition {
                name: "alpha".to_string(),
                description: "a tool".to_string(),
                parameters: json!({"type": "object"}),
            },
        ];

        let exchange = tools_exchange(&tools).expect("two tools should produce an exchange");

        assert!(matches!(exchange.source, PreludeSource::Tools));
        let rendered = user_text(&exchange.user);
        assert_eq!(
            rendered, "## Available tools\n\n- alpha: a tool\n- zeta: z tool",
            "tools must render sorted by name with the prescribed shape"
        );
    }

    #[test]
    fn tools_exchange_returns_none_for_empty_tool_slice() {
        let exchange = tools_exchange(&[]);
        assert!(exchange.is_none());
    }
}
