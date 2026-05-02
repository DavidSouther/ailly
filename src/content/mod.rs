use anyhow::{Context as _, Result};
use rig::{
    OneOrMany,
    message::{AssistantContent, Message, UserContent},
};
use serde::{Deserialize, Serialize};
use vfs::VfsPath;

pub mod gitignore_fs;
pub mod gitignore_fs_constants;

#[cfg(test)]
mod test_util;

pub const AILLYRC: &str = ".aillyrc.toml";
pub const EXTENSION: &str = ".toml";

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ContentMetaContext {
    #[default]
    Conversation,
    Folder,
    None,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Parent {
    #[default]
    Root,
    Always,
    Never,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContentMeta {
    #[serde(default)]
    context: ContentMetaContext,
    #[serde(default)]
    parent: Parent,
    #[serde(default)]
    r#continue: bool,
    #[serde(default)]
    skip: bool,
    #[serde(default)]
    skip_head: bool,
    #[serde(default)]
    isolated: bool,
}

#[derive(Debug, Clone)]
pub struct ConversationTurn {
    /// Where the conversation was stored on disk
    path: VfsPath,
    /// Metadata for how this ConversationTurn should run
    meta: ContentMeta,
    /// System message chain inherited at this turn's location
    system: Vec<Message>,
    /// The original user prompt for this turn
    prompt: OneOrMany<Message>,
    /// All subsequent messages, for this turn.
    response: Vec<Message>,
}

impl ConversationTurn {
    /// Parse one `<name>.toml` turn file at `path`.
    ///
    /// The file format is `prompt = "..."` plus an optional `[[response]]`
    /// array of `{ role, text }` messages. The caller threads the inherited
    /// `meta` and `system` from the surrounding `.aillyrc.toml` chain;
    /// `ConversationTurn::load` does not walk the filesystem itself.
    ///
    /// Returns an error when the file cannot be read, fails to parse, or
    /// has an empty `prompt`.
    ///
    /// Out of scope: `combined`, `view`, `edit`, `template-view`, `mcp`,
    /// `tools`, `temperature`, `maxTokens`, `out`/`root`, and `augment`
    /// from the TypeScript `loadFile`.
    pub async fn load(path: VfsPath, meta: ContentMeta, system: Vec<Message>) -> Result<Self> {
        let text = path
            .read_to_string()
            .with_context(|| format!("reading {}", path.as_str()))?;
        let file: ConversationTurnFile =
            toml::from_str(&text).with_context(|| format!("parsing {}", path.as_str()))?;

        if file.prompt.is_empty() {
            anyhow::bail!("turn at {} has empty prompt", path.as_str());
        }
        let prompt = OneOrMany::one(Message::user(file.prompt));

        let response: Vec<Message> = file
            .response
            .into_iter()
            .map(MessageFile::into_message)
            .collect();

        Ok(Self {
            path,
            meta,
            system,
            prompt,
            response,
        })
    }

    /// Serialize this turn to its `path` as TOML.
    ///
    /// Format mirrors `ConversationTurn::load`: a top-level `prompt` string
    /// followed by an optional `[[response]]` array of `{ role, text }`
    /// entries. The file is created or truncated in place.
    ///
    /// Returns an error when a message contains non-text content (tool
    /// calls, tool results, images), when TOML serialization fails, or
    /// when the file cannot be opened for writing.
    pub async fn write(&self) -> Result<()> {
    // Out of scope: the `clean` option, the YAML head with
    // `view`/`debug`/`augment` summary, the `combined` mode that writes
    // prompt and response into a single file at `outPath != path`,
    // `skipHead`, and `mkdirp` of arbitrary `outPath` parents. These
    // extend the existing deferred items from the load path.
        let prompt_text = match self.prompt.first() {
            Message::User { content } => match content.first() {
                UserContent::Text(t) => t.text.clone(),
                _ => anyhow::bail!(
                    "non-text user prompt at {} cannot be serialized",
                    self.path.as_str()
                ),
            },
            _ => anyhow::bail!(
                "prompt at {} is not a user message",
                self.path.as_str()
            ),
        };

        let response: Vec<MessageFile> = self
            .response
            .iter()
            .cloned()
            .map(MessageFile::try_from)
            .collect::<Result<_>>()?;

        let file = ConversationTurnFile {
            prompt: prompt_text,
            response,
        };
        let text = toml::to_string(&file)
            .with_context(|| format!("serializing {}", self.path.as_str()))?;

        let mut writer = self
            .path
            .create_file()
            .with_context(|| format!("opening {} for write", self.path.as_str()))?;
        std::io::Write::write_all(&mut writer, text.as_bytes())
            .with_context(|| format!("writing {}", self.path.as_str()))?;
        Ok(())
    }
}

/// A Conversation holds all the ConversationTurns.
pub struct Conversation {
    turns: Vec<(VfsPath, ConversationTurn)>,
}

impl Conversation {
    /// Walk the directory rooted at `path` recursively and build one
    /// `ConversationTurn` per `<name>.toml` file other than `.aillyrc.toml`.
    ///
    /// `.aillyrc.toml` system messages and meta are threaded down via
    /// `AillyRc::load`, so each turn carries the system chain inherited at
    /// its location. Turns appear in `turns` in walk order: parent before
    /// child, lexicographic among siblings within a directory. A directory
    /// whose `.aillyrc.toml` sets `skip = true` contributes no turns and
    /// is not recursed into.
    ///
    /// Out of scope: the `.vectors` directory skip, synthetic CLI content,
    /// and folder-context wiring. See the implementation plan's
    /// "Deferred" list.
    pub async fn load(path: VfsPath) -> Result<Self> {
        let mut turns: Vec<(VfsPath, ConversationTurn)> = Vec::new();
        Self::load_into(&path, AillyRc::default(), &mut turns).await?;
        Ok(Self { turns })
    }

    /// Write every turn back to its on-disk path.
    ///
    /// Iterates `self.turns` in load order and calls each turn's `write`.
    /// Returns the first error encountered. The TypeScript
    /// log-and-continue behaviour is intentionally not ported.
    ///
    /// Out of scope: the `clean` option and any handling of
    /// `combined`/`outPath` rewriting. See `ConversationTurn::write` for
    /// the per-turn deferred items.
    pub async fn write(&self) -> Result<()> {
        for (_, turn) in &self.turns {
            turn.write().await?;
        }
        Ok(())
    }

    /// Return the immediately prior `ConversationTurn` in load order, if any.
    pub fn predecessor(&self, turn: &ConversationTurn) -> Option<&ConversationTurn> {
        let idx = self.turns.iter().position(|(_, t)| t.path == turn.path)?;
        if idx == 0 {
            return None;
        }
        Some(&self.turns[idx - 1].1)
    }

    /// Return the inherited system messages for `turn`.
    pub fn system<'a>(&self, turn: &'a ConversationTurn) -> &'a [Message] {
        &turn.system
    }

    async fn load_into(
        dir: &VfsPath,
        prior: AillyRc,
        turns: &mut Vec<(VfsPath, ConversationTurn)>,
    ) -> Result<()> {
        let acc = AillyRc::load(dir, prior).await?;
        if acc.meta.skip {
            return Ok(());
        }

        let mut entries: Vec<VfsPath> = dir.read_dir()?.collect();
        entries.sort_by(|a, b| a.filename().cmp(&b.filename()));

        for entry in &entries {
            if !is_turn_file(entry) {
                continue;
            }
            let turn =
                ConversationTurn::load(entry.clone(), acc.meta.clone(), acc.system.clone()).await?;
            turns.push((entry.clone(), turn));
        }

        let subdirs: Vec<VfsPath> = entries
            .into_iter()
            .filter(|e| e.is_dir().unwrap_or(false))
            .collect();
        for sub in subdirs {
            Box::pin(Self::load_into(&sub, acc.clone(), turns)).await?;
        }

        Ok(())
    }
}

/// True when `entry` is a turn file: a regular `.toml` file other than
/// the `.aillyrc.toml` marker.
fn is_turn_file(entry: &VfsPath) -> bool {
    if !entry.is_file().unwrap_or(false) {
        return false;
    }
    let name = entry.filename();
    name != AILLYRC && name.ends_with(EXTENSION)
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ConversationTurnFile {
    prompt: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    response: Vec<MessageFile>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
enum MessageFile {
    User { text: String },
    Assistant { text: String },
    System { text: String },
}

impl MessageFile {
    fn into_message(self) -> Message {
        match self {
            MessageFile::User { text } => Message::user(text),
            MessageFile::Assistant { text } => Message::assistant(text),
            MessageFile::System { text } => Message::system(text),
        }
    }
}

impl TryFrom<Message> for MessageFile {
    type Error = anyhow::Error;

    fn try_from(m: Message) -> Result<Self> {
        match m {
            Message::System { content } => Ok(MessageFile::System { text: content }),
            Message::User { content } => match content.first() {
                UserContent::Text(t) => Ok(MessageFile::User { text: t.text }),
                _ => anyhow::bail!("non-text user message cannot be serialized"),
            },
            Message::Assistant { content, .. } => match content.first() {
                AssistantContent::Text(t) => Ok(MessageFile::Assistant { text: t.text }),
                _ => anyhow::bail!("non-text assistant message cannot be serialized"),
            },
        }
    }
}

/// Accumulated state from walking up `.aillyrc.toml` files.
#[derive(Debug, Default, Clone)]
pub struct AillyRc {
    pub system: Vec<Message>,
    pub meta: ContentMeta,
}

impl AillyRc {
    /// Read `.aillyrc.toml` at `dir` and merge it into `prior` per the `parent` mode.
    ///
    /// Mirrors the TypeScript `loadAillyRc` from
    /// `ailly_typescript/core/src/content/content.ts`.
    pub async fn load(dir: &VfsPath, prior: AillyRc) -> Result<Self> {
        let AillyRc {
            mut system,
            mut meta,
        } = prior;

        let path = dir.join(AILLYRC)?;
        let exists = path.exists()?;
        let file: AillyRcFile = if exists {
            let text = path
                .read_to_string()
                .with_context(|| format!("reading {}", path.as_str()))?;
            toml::from_str(&text).with_context(|| format!("parsing {}", path.as_str()))?
        } else {
            AillyRcFile::default()
        };

        if !exists && file.parent.is_none() && meta.parent == Parent::Always {
            meta.parent = Parent::Root;
        }

        file.merge_into(&mut meta);

        let local = file
            .system
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(Message::system);

        match meta.parent {
            Parent::Root => {}
            Parent::Never => system.clear(),
            Parent::Always => {
                if system.is_empty() && !dir.is_root() {
                    let parent_dir = dir.parent();
                    let recursed = Box::pin(AillyRc::load(
                        &parent_dir,
                        AillyRc {
                            system: Vec::new(),
                            meta: meta.clone(),
                        },
                    ))
                    .await?;
                    system = recursed.system;
                }
            }
        }

        if let Some(m) = local {
            system.push(m);
        }

        Ok(AillyRc { system, meta })
    }
}

/// On-disk format of a `.aillyrc.toml` file. Each meta field is `Option`
/// so that fields absent from the file do not clobber inherited values.
#[derive(Debug, Default, Deserialize)]
struct AillyRcFile {
    #[serde(default)]
    system: Option<String>,
    #[serde(default)]
    context: Option<ContentMetaContext>,
    #[serde(default)]
    parent: Option<Parent>,
    #[serde(default)]
    r#continue: Option<bool>,
    #[serde(default)]
    skip: Option<bool>,
    #[serde(default)]
    skip_head: Option<bool>,
    #[serde(default)]
    isolated: Option<bool>,
}

impl AillyRcFile {
    /// Apply fields present on the file onto `meta`, leaving inherited values
    /// untouched when a field is absent.
    fn merge_into(&self, meta: &mut ContentMeta) {
        if let Some(c) = self.context {
            meta.context = c;
        }
        if let Some(p) = self.parent {
            meta.parent = p;
        }
        if let Some(c) = self.r#continue {
            meta.r#continue = c;
        }
        if let Some(s) = self.skip {
            meta.skip = s;
        }
        if let Some(s) = self.skip_head {
            meta.skip_head = s;
        }
        if let Some(i) = self.isolated {
            meta.isolated = i;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_util::mem_fs;

    #[tokio::test]
    async fn at_root_with_no_aillyrc_in_cwd() {
        let fs = mem_fs! { "root": {} };
        let cwd = fs.join("root").unwrap();

        let acc = AillyRc::load(&cwd, AillyRc::default()).await.unwrap();

        let texts: Vec<String> = acc.system.iter().map(message_text).collect();
        assert_eq!(texts, Vec::<String>::new());
    }

    #[tokio::test]
    async fn at_root_with_aillyrc_in_cwd() {
        let fs = mem_fs! {
            "root": { ".aillyrc.toml": r#"system = "system""# },
        };
        let cwd = fs.join("root").unwrap();

        let acc = AillyRc::load(&cwd, AillyRc::default()).await.unwrap();

        let texts: Vec<String> = acc.system.iter().map(message_text).collect();
        assert_eq!(texts, vec!["system".to_string()]);
    }

    #[tokio::test]
    async fn below_root_with_no_aillyrc_carries_parent_system() {
        let fs = mem_fs! {
            "root": {
                ".aillyrc.toml": r#"system = "system""#,
                "below": {},
            },
        };
        let cwd = fs.join("root/below").unwrap();

        let prior = AillyRc {
            system: vec![Message::system("root")],
            meta: ContentMeta::default(),
        };
        let acc = AillyRc::load(&cwd, prior).await.unwrap();

        let texts: Vec<String> = acc.system.iter().map(message_text).collect();
        assert_eq!(texts, vec!["root".to_string()]);
    }

    #[tokio::test]
    async fn below_root_with_aillyrc_appends() {
        let fs = mem_fs! {
            "root": {
                ".aillyrc.toml": r#"system = "root""#,
                "below": { ".aillyrc.toml": r#"system = "below""# },
            },
        };
        let cwd = fs.join("root/below").unwrap();

        let prior = AillyRc {
            system: vec![Message::system("root")],
            meta: ContentMeta::default(),
        };
        let acc = AillyRc::load(&cwd, prior).await.unwrap();

        let texts: Vec<String> = acc.system.iter().map(message_text).collect();
        assert_eq!(texts, vec!["root".to_string(), "below".to_string()]);
    }

    #[tokio::test]
    async fn always_pulls_parent_when_system_empty() {
        let fs = mem_fs! {
            "root": {
                ".aillyrc.toml": r#"system = "root""#,
                "below": { ".aillyrc.toml": r#"system = "below""# },
            },
        };
        let cwd = fs.join("root/below").unwrap();

        let prior = AillyRc {
            system: Vec::new(),
            meta: ContentMeta {
                parent: Parent::Always,
                ..Default::default()
            },
        };
        let acc = AillyRc::load(&cwd, prior).await.unwrap();

        let texts: Vec<String> = acc.system.iter().map(message_text).collect();
        assert_eq!(texts, vec!["root".to_string(), "below".to_string()]);
    }

    #[tokio::test]
    async fn always_chains_three_levels() {
        let fs = mem_fs! {
            "root": {
                ".aillyrc.toml": r#"system = "root""#,
                "below": {
                    ".aillyrc.toml": "parent = \"always\"\nsystem = \"below\"\n",
                    "deep": {
                        ".aillyrc.toml": "parent = \"always\"\nsystem = \"deep\"\n",
                    },
                },
            },
        };
        let cwd = fs.join("root/below/deep").unwrap();

        let prior = AillyRc {
            system: Vec::new(),
            meta: ContentMeta {
                parent: Parent::Always,
                ..Default::default()
            },
        };
        let acc = AillyRc::load(&cwd, prior).await.unwrap();

        let texts: Vec<String> = acc.system.iter().map(message_text).collect();
        assert_eq!(
            texts,
            vec!["root".to_string(), "below".to_string(), "deep".to_string()]
        );
    }

    #[tokio::test]
    async fn always_breaks_at_missing_intermediate() {
        let fs = mem_fs! {
            "root": {
                ".aillyrc.toml": r#"system = "root""#,
                "below": {
                    "deep": {
                        ".aillyrc.toml": "parent = \"always\"\nsystem = \"deep\"\n",
                    },
                },
            },
        };
        let cwd = fs.join("root/below/deep").unwrap();

        let prior = AillyRc {
            system: Vec::new(),
            meta: ContentMeta {
                parent: Parent::Always,
                ..Default::default()
            },
        };
        let acc = AillyRc::load(&cwd, prior).await.unwrap();

        let texts: Vec<String> = acc.system.iter().map(message_text).collect();
        assert_eq!(texts, vec!["deep".to_string()]);
    }

    #[tokio::test]
    async fn always_keeps_existing_system_and_appends() {
        let fs = mem_fs! {
            "root": {
                ".aillyrc.toml": r#"system = "root""#,
                "below": {
                    "deep": {
                        ".aillyrc.toml": "parent = \"always\"\nsystem = \"deep\"\n",
                    },
                },
            },
        };
        let cwd = fs.join("root/below/deep").unwrap();

        let prior = AillyRc {
            system: vec![Message::system("below")],
            meta: ContentMeta {
                parent: Parent::Always,
                ..Default::default()
            },
        };
        let acc = AillyRc::load(&cwd, prior).await.unwrap();

        let texts: Vec<String> = acc.system.iter().map(message_text).collect();
        assert_eq!(texts, vec!["below".to_string(), "deep".to_string()]);
    }

    #[tokio::test]
    async fn never_replaces_inherited_with_local() {
        let fs = mem_fs! {
            "root": {
                ".aillyrc.toml": r#"system = "root""#,
                "below": { ".aillyrc.toml": r#"system = "below""# },
            },
        };
        let cwd = fs.join("root/below").unwrap();

        let prior = AillyRc {
            system: vec![Message::system("root")],
            meta: ContentMeta {
                parent: Parent::Never,
                ..Default::default()
            },
        };
        let acc = AillyRc::load(&cwd, prior).await.unwrap();

        let texts: Vec<String> = acc.system.iter().map(message_text).collect();
        assert_eq!(texts, vec!["below".to_string()]);
    }

    #[tokio::test]
    async fn never_with_no_local_aillyrc_drops_inherited() {
        let fs = mem_fs! { "root": { "below": {} } };
        let cwd = fs.join("root/below").unwrap();

        let prior = AillyRc {
            system: vec![Message::system("root")],
            meta: ContentMeta {
                parent: Parent::Never,
                ..Default::default()
            },
        };
        let acc = AillyRc::load(&cwd, prior).await.unwrap();

        let texts: Vec<String> = acc.system.iter().map(message_text).collect();
        assert_eq!(texts, Vec::<String>::new());
    }

    #[tokio::test]
    async fn loads_a_single_turn_with_prompt_only() {
        let fs = mem_fs! {
            "root": {
                "01_intro.toml": r#"prompt = "say hi""#,
            },
        };
        let path = fs.join("root/01_intro.toml").unwrap();
        let turn = ConversationTurn::load(path.clone(), ContentMeta::default(), Vec::new())
            .await
            .unwrap();
        assert_eq!(turn.path.as_str(), path.as_str());
        assert_eq!(turn.response.len(), 0);
        assert_eq!(turn.prompt.len(), 1);
        assert!(turn.system.is_empty());
    }

    #[tokio::test]
    async fn predecessor_returns_prior_turn_or_none() {
        let fs = mem_fs! {
            "root": {
                "01.toml": r#"prompt = "first""#,
                "02.toml": r#"prompt = "second""#,
            },
        };
        let convo = Conversation::load(fs.join("root").unwrap()).await.unwrap();
        let first = &convo.turns[0].1;
        let second = &convo.turns[1].1;

        assert!(convo.predecessor(first).is_none());
        let pred = convo.predecessor(second).expect("second has a predecessor");
        assert!(pred.path.as_str().ends_with("01.toml"));
    }

    #[tokio::test]
    async fn system_returns_messages_for_turn() {
        let fs = mem_fs! {
            "root": {
                ".aillyrc.toml": r#"system = "sys""#,
                "01.toml": r#"prompt = "x""#,
            },
        };
        let convo = Conversation::load(fs.join("root").unwrap()).await.unwrap();
        let only = &convo.turns[0].1;
        assert_eq!(convo.system(only).len(), 1);
    }

    #[tokio::test]
    async fn loads_turns_recursively_parents_before_children() {
        let fs = mem_fs! {
            "root": {
                ".aillyrc.toml": r#"system = "root-sys""#,
                "01.toml": r#"prompt = "top""#,
                "child": {
                    "02.toml": r#"prompt = "deep""#,
                },
            },
        };
        let convo = Conversation::load(fs.join("root").unwrap()).await.unwrap();
        let paths: Vec<String> = convo
            .turns
            .iter()
            .map(|(p, _)| p.as_str().to_string())
            .collect();
        assert_eq!(paths.len(), 2);
        assert!(paths[0].ends_with("01.toml"));
        assert!(paths[1].ends_with("02.toml"));
    }

    #[tokio::test]
    async fn child_turns_inherit_parent_system() {
        let fs = mem_fs! {
            "root": {
                ".aillyrc.toml": r#"system = "root-sys""#,
                "child": {
                    ".aillyrc.toml": r#"system = "child-sys""#,
                    "01.toml": r#"prompt = "x""#,
                },
            },
        };
        let convo = Conversation::load(fs.join("root").unwrap()).await.unwrap();
        assert_eq!(convo.turns.len(), 1);
        assert_eq!(convo.turns[0].1.system.len(), 2);
    }

    #[tokio::test]
    async fn loads_directory_of_turns_in_lexicographic_order() {
        let fs = mem_fs! {
            "root": {
                ".aillyrc.toml": r#"system = "sys""#,
                "02_b.toml": r#"prompt = "second""#,
                "01_a.toml": r#"prompt = "first""#,
            },
        };
        let dir = fs.join("root").unwrap();
        let convo = Conversation::load(dir).await.unwrap();
        assert_eq!(convo.turns.len(), 2);
        assert!(convo.turns[0].0.as_str().ends_with("01_a.toml"));
        assert!(convo.turns[1].0.as_str().ends_with("02_b.toml"));
        assert_eq!(convo.turns[0].1.system.len(), 1);
    }

    #[tokio::test]
    async fn skips_directory_when_meta_skip_true() {
        let fs = mem_fs! {
            "root": {
                ".aillyrc.toml": "skip = true\n",
                "01.toml": r#"prompt = "x""#,
            },
        };
        let convo = Conversation::load(fs.join("root").unwrap()).await.unwrap();
        assert!(convo.turns.is_empty());
    }

    #[tokio::test]
    async fn loads_turn_with_response() {
        let fs = mem_fs! {
            "root": {
                "01_intro.toml": "prompt = \"say hi\"\n\n[[response]]\nrole = \"assistant\"\ntext = \"hi\"\n",
            },
        };
        let path = fs.join("root/01_intro.toml").unwrap();
        let turn = ConversationTurn::load(path, ContentMeta::default(), Vec::new())
            .await
            .unwrap();
        assert_eq!(turn.response.len(), 1);
    }

    #[tokio::test]
    async fn rejects_turn_with_empty_prompt() {
        let fs = mem_fs! { "root": { "01.toml": r#"prompt = """# } };
        let path = fs.join("root/01.toml").unwrap();
        let err = ConversationTurn::load(path, ContentMeta::default(), Vec::new())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("empty prompt"));
    }

    #[tokio::test]
    async fn carries_system_passed_in() {
        let fs = mem_fs! { "root": { "01.toml": r#"prompt = "x""# } };
        let path = fs.join("root/01.toml").unwrap();
        let sys = vec![Message::system("inherited")];
        let turn = ConversationTurn::load(path, ContentMeta::default(), sys.clone())
            .await
            .unwrap();
        assert_eq!(turn.system.len(), 1);
    }

    #[test]
    fn parses_turn_file_with_string_prompt_and_response() {
        let toml = r#"
prompt = "hello\nworld"

[[response]]
role = "assistant"
text = "hi"
"#;
        let parsed: ConversationTurnFile = toml::from_str(toml).unwrap();
        assert_eq!(parsed.prompt, "hello\nworld");
        assert_eq!(parsed.response.len(), 1);
    }

    #[test]
    fn parses_turn_file_with_no_response() {
        let toml = r#"prompt = "hi""#;
        let parsed: ConversationTurnFile = toml::from_str(toml).unwrap();
        assert_eq!(parsed.prompt, "hi");
        assert!(parsed.response.is_empty());
    }

    #[test]
    fn serializes_turn_file_with_prompt_and_response() {
        let file = ConversationTurnFile {
            prompt: "hello\nworld".to_string(),
            response: vec![MessageFile::Assistant {
                text: "hi".to_string(),
            }],
        };
        let toml_text = toml::to_string(&file).unwrap();
        let parsed: ConversationTurnFile = toml::from_str(&toml_text).unwrap();
        assert_eq!(parsed.prompt, "hello\nworld");
        assert_eq!(parsed.response.len(), 1);
    }

    #[test]
    fn serializes_turn_file_omits_empty_response() {
        let file = ConversationTurnFile {
            prompt: "hi".to_string(),
            response: Vec::new(),
        };
        let toml_text = toml::to_string(&file).unwrap();
        assert!(!toml_text.contains("response"));
    }

    #[tokio::test]
    async fn write_rejects_non_text_user_prompt() {
        use rig::message::Image;

        let fs = mem_fs! { "root": { "01.toml": r#"prompt = "x""# } };
        let path = fs.join("root/01.toml").unwrap();

        let image = UserContent::Image(Image::default());
        let prompt = OneOrMany::one(Message::User {
            content: OneOrMany::one(image),
        });
        let turn = ConversationTurn {
            path: path.clone(),
            meta: ContentMeta::default(),
            system: Vec::new(),
            prompt,
            response: Vec::new(),
        };

        let err = turn.write().await.unwrap_err();
        assert!(err.to_string().contains("non-text user prompt"));
    }

    #[tokio::test]
    async fn conversation_write_surfaces_turn_error() {
        use rig::message::Image;

        let fs = mem_fs! { "root": { "01.toml": r#"prompt = "x""# } };
        let dir = fs.join("root").unwrap();
        let mut convo = Conversation::load(dir).await.unwrap();

        let image = UserContent::Image(Image::default());
        convo.turns[0].1.prompt = OneOrMany::one(Message::User {
            content: OneOrMany::one(image),
        });

        assert!(convo.write().await.is_err());
    }

    #[tokio::test]
    async fn conversation_write_round_trips_all_turns() {
        let fs = mem_fs! {
            "root": {
                ".aillyrc.toml": r#"system = "sys""#,
                "01.toml": r#"prompt = "first""#,
                "child": {
                    "02.toml": "prompt = \"second\"\n\n[[response]]\nrole = \"assistant\"\ntext = \"a\"\n",
                },
            },
        };
        let dir = fs.join("root").unwrap();
        let convo = Conversation::load(dir.clone()).await.unwrap();
        assert_eq!(convo.turns.len(), 2);

        convo.write().await.unwrap();

        let reloaded = Conversation::load(dir).await.unwrap();
        assert_eq!(reloaded.turns.len(), 2);
        assert!(reloaded.turns[0].0.as_str().ends_with("01.toml"));
        assert!(reloaded.turns[1].0.as_str().ends_with("02.toml"));
        assert_eq!(reloaded.turns[1].1.response.len(), 1);
    }

    #[tokio::test]
    async fn write_then_load_round_trips_with_response() {
        let fs = mem_fs! {
            "root": {
                "01.toml": "prompt = \"q\"\n\n[[response]]\nrole = \"assistant\"\ntext = \"a\"\n",
            },
        };
        let path = fs.join("root/01.toml").unwrap();
        let turn = ConversationTurn::load(path.clone(), ContentMeta::default(), Vec::new())
            .await
            .unwrap();

        turn.write().await.unwrap();

        let reloaded = ConversationTurn::load(path, ContentMeta::default(), Vec::new())
            .await
            .unwrap();
        assert_eq!(reloaded.response.len(), 1);
    }

    #[tokio::test]
    async fn write_then_load_round_trips_multi_response() {
        let fs = mem_fs! {
            "root": {
                "01.toml": "prompt = \"q\"\n\n[[response]]\nrole = \"assistant\"\ntext = \"a1\"\n\n[[response]]\nrole = \"user\"\ntext = \"u\"\n\n[[response]]\nrole = \"assistant\"\ntext = \"a2\"\n",
            },
        };
        let path = fs.join("root/01.toml").unwrap();
        let turn = ConversationTurn::load(path.clone(), ContentMeta::default(), Vec::new())
            .await
            .unwrap();

        turn.write().await.unwrap();

        let reloaded = ConversationTurn::load(path, ContentMeta::default(), Vec::new())
            .await
            .unwrap();
        assert_eq!(reloaded.response.len(), 3);
    }

    #[tokio::test]
    async fn write_then_load_round_trips_prompt_only() {
        let fs = mem_fs! {
            "root": { "01.toml": r#"prompt = "hello""# },
        };
        let path = fs.join("root/01.toml").unwrap();
        let turn = ConversationTurn::load(path.clone(), ContentMeta::default(), Vec::new())
            .await
            .unwrap();

        turn.write().await.unwrap();

        let reloaded = ConversationTurn::load(path, ContentMeta::default(), Vec::new())
            .await
            .unwrap();
        assert_eq!(reloaded.prompt.len(), 1);
        assert_eq!(reloaded.response.len(), 0);
    }

    fn message_text(m: &Message) -> String {
        match m {
            Message::System { content } => content.clone(),
            Message::User { content } => match content.first() {
                UserContent::Text(t) => t.text.clone(),
                _ => String::new(),
            },
            Message::Assistant { content, .. } => match content.first() {
                AssistantContent::Text(t) => t.text.clone(),
                _ => String::new(),
            },
        }
    }
}
