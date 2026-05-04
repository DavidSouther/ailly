use rig::{
    OneOrMany,
    message::{
        AssistantContent, Message, ToolCall, ToolFunction, ToolResult, ToolResultContent,
        UserContent,
    },
};
use serde::{Deserialize, Serialize};
use vfs::{MemoryFS, VfsPath};

mod gitignore_fs;
mod gitignore_fs_constants;

use crate::knowledge::skills::{Skill, SkillError, SkillName, SkillRepository};

pub const AILLYRC: &str = ".ailly.toml";
pub const EXTENSION: &str = ".toml";

#[derive(Debug, thiserror::Error)]
pub enum ContentError {
    #[error("reading {path}")]
    Read {
        path: String,
        #[source]
        source: vfs::VfsError,
    },

    #[error("listing directory {path}")]
    ListDir {
        path: String,
        #[source]
        source: vfs::VfsError,
    },

    #[error("resolving path under {path}")]
    ResolvePath {
        path: String,
        #[source]
        source: vfs::VfsError,
    },

    #[error("checking existence of {path}")]
    CheckExists {
        path: String,
        #[source]
        source: vfs::VfsError,
    },

    #[error("opening {path} for write")]
    OpenForWrite {
        path: String,
        #[source]
        source: vfs::VfsError,
    },

    #[error("writing {path}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("parsing {path}")]
    ParseToml {
        path: String,
        #[source]
        source: toml::de::Error,
    },

    #[error("serializing {path}")]
    SerializeToml {
        path: String,
        #[source]
        source: toml::ser::Error,
    },

    #[error("turn at {path} has empty prompt")]
    EmptyPrompt { path: String },

    #[error("prompt at {path} is not a user message")]
    PromptNotUserMessage { path: String },

    #[error("non-text user prompt at {path} cannot be serialized")]
    NonTextUserPrompt { path: String },

    #[error("failed to load skill `{name}` referenced in {origin}")]
    Skill {
        name: String,
        origin: String,
        #[source]
        source: SkillError,
    },

    #[error("non-text tool result at {path} cannot be serialized")]
    NonTextToolResult { path: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssistantResponse {
    pub text: String,
    pub model: Option<String>,
    pub engine: Option<String>,
    pub stop_reason: Option<String>,
    pub usage: Option<ResponseUsage>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResponseUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, Clone)]
pub enum TurnMessage {
    User(String),
    Assistant(AssistantResponse),
    System(String),
    ToolCall(ToolCall),
    ToolResult(ToolResult),
}

/// The preamble for a turn: everything that should be sent to an engine
/// before the chat history. Blocks appear in the order produced by
/// `Conversation::preamble_for`.
#[derive(Debug, Clone, Default)]
pub struct Preamble {
    pub blocks: Vec<PreambleBlock>,
}

#[derive(Debug, Clone)]
pub enum PreambleBlock {
    /// A `Message::System` inherited from an ancestor `.ailly.toml`.
    InheritedSystem { text: String },
    /// A `Skill` resolved at this turn's location, in walk-then-declaration
    /// order.
    Skill(Skill),
    /// The `Message::System` that originates in the turn's own directory.
    LocalSystem { text: String },
}

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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolsParent {
    #[default]
    Extend,
    Replace,
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
    /// System message chain visible at this turn's location, in walk order.
    /// The trailing `local_system_count` entries originate in this turn's own
    /// directory; the remaining leading entries are inherited from ancestors.
    system: Vec<Message>,
    /// Number of trailing entries of `system` that come from this turn's own
    /// directory (0 or 1 in practice).
    local_system_count: usize,
    /// Skills resolved at this turn's location, in walk-then-declaration order
    skills: Vec<Skill>,
    /// The original user prompt for this turn
    prompt: OneOrMany<Message>,
    /// All subsequent messages, for this turn.
    response: Vec<TurnMessage>,
    /// Tool names resolved for this turn after combining the inherited
    /// chain with the per-turn declaration.
    tool_names: Vec<String>,
    /// Per-turn `tools` literal, exactly as authored on disk. Captured so
    /// `write` can reproduce the source file byte-stably.
    declared_tools: Vec<String>,
    /// Per-turn `parent_tools` literal, exactly as authored on disk.
    declared_parent_tools: Option<ToolsParent>,
}

impl ConversationTurn {
    /// Where this turn lives on disk.
    pub fn path(&self) -> &VfsPath {
        &self.path
    }

    /// Parse one `<name>.toml` turn file at `path`.
    ///
    /// The file format is `prompt = "..."` plus an optional `[[response]]`
    /// array of `{ role, text }` messages. The caller threads the inherited
    /// `meta` and `system` from the surrounding `.ailly.toml` chain;
    /// `ConversationTurn::load` does not walk the filesystem itself.
    ///
    /// Returns an error when the file cannot be read, fails to parse, or
    /// has an empty `prompt`.
    pub async fn load(
        path: VfsPath,
        meta: ContentMeta,
        system: Vec<Message>,
        local_system_count: usize,
        skills: Vec<Skill>,
        tools: Vec<String>,
    ) -> Result<Self, ContentError> {
        // Out of scope: `combined`, `view`, `edit`, `template-view`, `mcp`,
        // `tools`, `temperature`, `maxTokens`, `out`/`root`, and `augment`
        // from the TypeScript `loadFile`.
        let text = path.read_to_string().map_err(|source| ContentError::Read {
            path: path.as_str().to_string(),
            source,
        })?;
        let file: ConversationTurnFile =
            toml::from_str(&text).map_err(|source| ContentError::ParseToml {
                path: path.as_str().to_string(),
                source,
            })?;

        if file.prompt.is_empty() {
            return Err(ContentError::EmptyPrompt {
                path: path.as_str().to_string(),
            });
        }
        let prompt = OneOrMany::one(Message::user(file.prompt));

        let response: Vec<TurnMessage> = file.response.into_iter().map(TurnMessage::from).collect();

        let declared_tools = file.tools.clone();
        let declared_parent_tools = file.parent_tools;
        let tool_names = combine_tools(tools, &file.tools, file.parent_tools);

        Ok(Self {
            path,
            meta,
            system,
            local_system_count,
            skills,
            prompt,
            response,
            tool_names,
            declared_tools,
            declared_parent_tools,
        })
    }

    /// Tool names resolved for this turn after walking the directory chain
    /// and applying any per-turn `tools`/`parent_tools` override.
    pub fn tool_names(&self) -> &[String] {
        &self.tool_names
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
    pub async fn write(&self) -> Result<(), ContentError> {
        // Out of scope: the `clean` option, the YAML head with
        // `view`/`debug`/`augment` summary, the `combined` mode that writes
        // prompt and response into a single file at `outPath != path`,
        // `skipHead`, and `mkdirp` of arbitrary `outPath` parents. These
        // extend the existing deferred items from the load path.
        let prompt_text = match self.prompt.first() {
            Message::User { content } => match content.first() {
                UserContent::Text(t) => t.text.clone(),
                _ => {
                    return Err(ContentError::NonTextUserPrompt {
                        path: self.path.as_str().to_string(),
                    });
                }
            },
            _ => {
                return Err(ContentError::PromptNotUserMessage {
                    path: self.path.as_str().to_string(),
                });
            }
        };

        let response: Vec<MessageFile> = self
            .response
            .iter()
            .map(|m| turn_message_to_file(m, self.path.as_str()))
            .collect::<Result<_, _>>()?;

        let file = ConversationTurnFile {
            prompt: prompt_text,
            tools: self.declared_tools.clone(),
            parent_tools: self.declared_parent_tools,
            response,
        };
        let text = toml::to_string(&file).map_err(|source| ContentError::SerializeToml {
            path: self.path.as_str().to_string(),
            source,
        })?;

        let mut writer = self
            .path
            .create_file()
            .map_err(|source| ContentError::OpenForWrite {
                path: self.path.as_str().to_string(),
                source,
            })?;
        std::io::Write::write_all(&mut writer, text.as_bytes()).map_err(|source| {
            ContentError::Write {
                path: self.path.as_str().to_string(),
                source,
            }
        })?;
        Ok(())
    }
}

/// A Conversation holds all the ConversationTurns.
pub struct Conversation {
    turns: Vec<(VfsPath, ConversationTurn)>,
}

impl Conversation {
    /// Walk the directory rooted at `path` recursively and build one
    /// `ConversationTurn` per `<name>.toml` file (other than `.ailly.toml`).
    ///
    /// `.ailly.toml` system messages and meta are threaded down via
    /// `AillyRc::load`, so each turn carries the system chain inherited at
    /// its location. Turns appear in `turns` in walk order: parent before
    /// child, lexicographic among siblings within a directory. A directory
    /// whose `.ailly.toml` sets `skip = true` contributes no turns and
    /// is not recursed into.
    pub async fn load(path: VfsPath, skills: &dyn SkillRepository) -> Result<Self, ContentError> {
        // Out of scope: the `.vectors` directory skip, synthetic CLI content,
        // and folder-context wiring. See the implementation plan's
        // "Deferred" list.
        let mut turns: Vec<(VfsPath, ConversationTurn)> = Vec::new();
        Self::load_into(&path, AillyRc::default(), skills, &mut turns).await?;
        Ok(Self { turns })
    }

    /// Build a Conversation with no turns.
    ///
    /// Used by callers that need a starting point to append synthetic
    /// turns to without first walking a real filesystem.
    pub fn empty() -> Self {
        Self { turns: Vec::new() }
    }

    /// Write every turn back to its on-disk path.
    ///
    /// Iterates `self.turns` in load order and calls each turn's `write`.
    /// Returns the first error encountered.
    pub async fn write(&self) -> Result<(), ContentError> {
        // Out of scope: the `clean` option and any handling of
        // `combined`/`outPath` rewriting. See `ConversationTurn::write` for
        // the per-turn deferred items.
        //
        // The TypeScript log-and-continue behaviour is intentionally not ported.
        for (_, turn) in &self.turns {
            turn.write().await?;
        }
        Ok(())
    }

    /// Number of loaded turns, in load order.
    pub fn turn_count(&self) -> usize {
        self.turns.len()
    }

    /// Borrow the turn at `idx` in load order.
    pub fn turn(&self, idx: usize) -> &ConversationTurn {
        &self.turns[idx].1
    }

    /// Strip every `[[response]]` entry from every turn, then write each
    /// turn back to disk. Idempotent: a second call produces a
    /// byte-identical file because `ConversationTurnFile.response`
    /// serializes via `skip_serializing_if = "Vec::is_empty"`.
    pub async fn clean(&mut self) -> Result<(), ContentError> {
        for (_, turn) in &mut self.turns {
            turn.response.clear();
            turn.write().await?;
        }
        Ok(())
    }

    /// Push an assistant `response` onto the turn at `idx`.
    ///
    /// Used by `Generator` to record an engine's `Final` text and metadata
    /// so subsequent `history_for` calls observe it as a predecessor's
    /// response and the on-disk file carries provenance for that turn.
    pub fn record_response(&mut self, idx: usize, response: AssistantResponse) {
        self.turns[idx]
            .1
            .response
            .push(TurnMessage::Assistant(response));
    }

    /// Append a tool call onto the turn at `idx` so subsequent `history_for`
    /// calls and the on-disk file observe it in stream order alongside the
    /// surrounding assistant text and the matching `ToolResult`.
    pub fn record_tool_call(&mut self, idx: usize, call: ToolCall) {
        self.turns[idx].1.response.push(TurnMessage::ToolCall(call));
    }

    /// Append a tool result onto the turn at `idx` so subsequent `history_for`
    /// calls and the on-disk file observe it in stream order paired with the
    /// preceding `ToolCall`.
    pub fn record_tool_result(&mut self, idx: usize, result: ToolResult) {
        self.turns[idx]
            .1
            .response
            .push(TurnMessage::ToolResult(result));
    }

    /// Append a synthetic user-prompt turn to the conversation.
    ///
    /// The new turn lives on an in-memory `VfsPath`, inheriting the
    /// `system` chain and `meta` from the previously loaded final turn so
    /// it observes the same system messages and metadata as a sibling
    /// would. When the conversation has no turns, the synthetic turn
    /// carries an empty system chain and default meta.
    ///
    /// Used by the CLI to support `--root <dir> --prompt <text>` where
    /// the prompt is the trailing turn over the root's loaded context.
    pub fn push_synthetic_prompt(&mut self, prompt: &str) -> Result<(), ContentError> {
        let (system, local_system_count, skills, meta, tool_names) = match self.turns.last() {
            Some((_, last)) => (
                last.system.clone(),
                last.local_system_count,
                last.skills.clone(),
                last.meta.clone(),
                last.tool_names.clone(),
            ),
            None => (
                Vec::new(),
                0,
                Vec::new(),
                ContentMeta::default(),
                Vec::new(),
            ),
        };

        let fs = VfsPath::new(MemoryFS::new());
        let dir = fs
            .join("synthetic")
            .map_err(|source| ContentError::ResolvePath {
                path: "synthetic".to_string(),
                source,
            })?;
        dir.create_dir()
            .map_err(|source| ContentError::OpenForWrite {
                path: dir.as_str().to_string(),
                source,
            })?;
        let path = dir
            .join("prompt.toml")
            .map_err(|source| ContentError::ResolvePath {
                path: "synthetic/prompt.toml".to_string(),
                source,
            })?;

        let turn = ConversationTurn {
            path: path.clone(),
            meta,
            system,
            local_system_count,
            skills,
            prompt: OneOrMany::one(Message::user(prompt.to_string())),
            response: Vec::new(),
            tool_names,
            declared_tools: Vec::new(),
            declared_parent_tools: None,
        };
        self.turns.push((path, turn));
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

    /// Return the resolved skills for `turn`, in walk-then-declaration order.
    pub fn skills<'a>(&self, turn: &'a ConversationTurn) -> &'a [Skill] {
        &turn.skills
    }

    /// Assemble the preamble for `turn`.
    ///
    /// Block order matches the design contract: every ancestor system
    /// message becomes one `InheritedSystem` block (inherited blocks are
    /// not collapsed), then every `Skill` becomes one `Skill` block in
    /// walk-then-declaration order, then the local system message (if any)
    /// becomes a `LocalSystem` block. `meta.skip_head` suppresses the
    /// entire preamble. Empty system messages are elided.
    pub fn preamble_for(&self, turn: &ConversationTurn) -> Preamble {
        let mut blocks: Vec<PreambleBlock> = Vec::new();
        if turn.meta.skip_head {
            return Preamble { blocks };
        }
        let inherited_end = turn.system.len().saturating_sub(turn.local_system_count);
        for m in &turn.system[..inherited_end] {
            if let Message::System { content } = m
                && !content.is_empty()
            {
                blocks.push(PreambleBlock::InheritedSystem {
                    text: content.clone(),
                });
            }
        }
        for s in &turn.skills {
            blocks.push(PreambleBlock::Skill(s.clone()));
        }
        for m in &turn.system[inherited_end..] {
            if let Message::System { content } = m
                && !content.is_empty()
            {
                blocks.push(PreambleBlock::LocalSystem {
                    text: content.clone(),
                });
            }
        }
        Preamble { blocks }
    }

    /// Chat history (user/assistant turns only) for `turn`, gated by
    /// `meta.isolated` and `meta.continue`. Excludes the system entries —
    /// those flow through `preamble_for`.
    pub fn messages_for(&self, turn: &ConversationTurn) -> Vec<Message> {
        let mut out: Vec<Message> = Vec::new();

        if !turn.meta.isolated {
            let mut chain: Vec<&ConversationTurn> = Vec::new();
            let mut cursor: &ConversationTurn = turn;
            while let Some(prev) = self.predecessor(cursor) {
                chain.push(prev);
                cursor = prev;
            }
            for prev in chain.into_iter().rev() {
                out.extend(prev.prompt.iter().cloned());
                out.extend(prev.response.iter().map(Message::from));
            }
        }

        out.extend(turn.prompt.iter().cloned());

        if !turn.meta.r#continue && matches!(out.last(), Some(Message::Assistant { .. })) {
            out.pop();
        }

        out
    }

    /// Build the chat history that should be sent to the engine for `turn`.
    ///
    /// Order: system chain (unless `meta.skip_head`), predecessor turns'
    /// `prompt + response` in load order (unless `meta.isolated`), then
    /// `turn.prompt`. Trailing assistant message is dropped unless
    /// `meta.continue == true`.
    pub fn history_for(&self, turn: &ConversationTurn) -> Vec<Message> {
        let mut out: Vec<Message> = Vec::new();

        if !turn.meta.skip_head {
            out.extend(turn.system.iter().cloned());
        }

        if !turn.meta.isolated {
            let mut chain: Vec<&ConversationTurn> = Vec::new();
            let mut cursor: &ConversationTurn = turn;
            while let Some(prev) = self.predecessor(cursor) {
                chain.push(prev);
                cursor = prev;
            }
            for prev in chain.into_iter().rev() {
                out.extend(prev.prompt.iter().cloned());
                extend_with_response_run(&mut out, &prev.response);
            }
        }

        out.extend(turn.prompt.iter().cloned());

        if !turn.meta.r#continue && matches!(out.last(), Some(Message::Assistant { .. })) {
            out.pop();
        }

        out
    }

    async fn load_into(
        dir: &VfsPath,
        prior: AillyRc,
        skills_repo: &dyn SkillRepository,
        turns: &mut Vec<(VfsPath, ConversationTurn)>,
    ) -> Result<(), ContentError> {
        let acc = AillyRc::load(dir, prior, skills_repo).await?;
        if acc.meta.skip {
            return Ok(());
        }

        let mut entries: Vec<VfsPath> = dir
            .read_dir()
            .map_err(|source| ContentError::ListDir {
                path: dir.as_str().to_string(),
                source,
            })?
            .collect();
        entries.sort_by_key(|entry| entry.filename());

        for entry in &entries {
            if !is_turn_file(entry) {
                continue;
            }
            let turn = ConversationTurn::load(
                entry.clone(),
                acc.meta.clone(),
                acc.system.clone(),
                acc.local_system_count,
                acc.skills.clone(),
                acc.tools.clone(),
            )
            .await?;
            turns.push((entry.clone(), turn));
        }

        let subdirs: Vec<VfsPath> = entries
            .into_iter()
            .filter(|e| e.is_dir().unwrap_or(false))
            .collect();
        for sub in subdirs {
            Box::pin(Self::load_into(&sub, acc.clone(), skills_repo, turns)).await?;
        }

        Ok(())
    }
}

fn extend_with_response_run(out: &mut Vec<Message>, response: &[TurnMessage]) {
    let mut assistant_buf: Vec<AssistantContent> = Vec::new();
    let flush = |buf: &mut Vec<AssistantContent>, out: &mut Vec<Message>| {
        if buf.is_empty() {
            return;
        }
        let drained: Vec<AssistantContent> = std::mem::take(buf);
        let content = OneOrMany::many(drained)
            .expect("flush only runs when assistant_buf has at least one entry");
        out.push(Message::Assistant { id: None, content });
    };

    for msg in response {
        match msg {
            TurnMessage::Assistant(response) => {
                assistant_buf.push(AssistantContent::Text(rig::message::Text {
                    text: response.text.clone(),
                }));
            }
            TurnMessage::ToolCall(call) => {
                assistant_buf.push(AssistantContent::ToolCall(call.clone()));
            }
            TurnMessage::ToolResult(result) => {
                flush(&mut assistant_buf, out);
                out.push(Message::User {
                    content: OneOrMany::one(UserContent::ToolResult(result.clone())),
                });
            }
            TurnMessage::User(text) => {
                flush(&mut assistant_buf, out);
                out.push(Message::user(text.clone()));
            }
            TurnMessage::System(text) => {
                flush(&mut assistant_buf, out);
                out.push(Message::system(text.clone()));
            }
        }
    }
    flush(&mut assistant_buf, out);
}

/// True when `entry` is a turn file: a regular `.toml` file other than
/// the `.ailly.toml` marker.
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
    tools: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    parent_tools: Option<ToolsParent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    response: Vec<MessageFile>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "snake_case")]
enum MessageFile {
    User {
        text: String,
    },
    Assistant {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        engine: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        stop_reason: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        usage: Option<UsageFile>,
    },
    System {
        text: String,
    },
    ToolCall {
        id: String,
        name: String,
        arguments: serde_json::Value,
    },
    ToolResult {
        id: String,
        content: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
struct UsageFile {
    input_tokens: u32,
    output_tokens: u32,
}

impl From<MessageFile> for TurnMessage {
    fn from(value: MessageFile) -> Self {
        match value {
            MessageFile::User { text } => TurnMessage::User(text),
            MessageFile::Assistant {
                text,
                model,
                engine,
                stop_reason,
                usage,
            } => TurnMessage::Assistant(AssistantResponse {
                text,
                model,
                engine,
                stop_reason,
                usage: usage.map(|u| ResponseUsage {
                    input_tokens: u.input_tokens,
                    output_tokens: u.output_tokens,
                }),
            }),
            MessageFile::System { text } => TurnMessage::System(text),
            MessageFile::ToolCall {
                id,
                name,
                arguments,
            } => TurnMessage::ToolCall(ToolCall::new(id, ToolFunction::new(name, arguments))),
            MessageFile::ToolResult { id, content } => TurnMessage::ToolResult(ToolResult {
                id,
                call_id: None,
                content: OneOrMany::one(ToolResultContent::Text(rig::message::Text {
                    text: content,
                })),
            }),
        }
    }
}

fn turn_message_to_file(value: &TurnMessage, path: &str) -> Result<MessageFile, ContentError> {
    Ok(match value {
        TurnMessage::User(text) => MessageFile::User { text: text.clone() },
        TurnMessage::Assistant(response) => MessageFile::Assistant {
            text: response.text.clone(),
            model: response.model.clone(),
            engine: response.engine.clone(),
            stop_reason: response.stop_reason.clone(),
            usage: response.usage.as_ref().map(|u| UsageFile {
                input_tokens: u.input_tokens,
                output_tokens: u.output_tokens,
            }),
        },
        TurnMessage::System(text) => MessageFile::System { text: text.clone() },
        TurnMessage::ToolCall(call) => MessageFile::ToolCall {
            id: call.id.clone(),
            name: call.function.name.clone(),
            arguments: call.function.arguments.clone(),
        },
        TurnMessage::ToolResult(result) => {
            let text = match result.content.first() {
                ToolResultContent::Text(t) => t.text.clone(),
                _ => {
                    return Err(ContentError::NonTextToolResult {
                        path: path.to_string(),
                    });
                }
            };
            MessageFile::ToolResult {
                id: result.id.clone(),
                content: text,
            }
        }
    })
}

/// Accumulated state from walking up `.ailly.toml` files.
#[derive(Debug, Default, Clone)]
pub struct AillyRc {
    pub system: Vec<Message>,
    /// Trailing entries of `system` that originate in this dir's own
    /// `.ailly.toml`. Always 0 or 1 in practice.
    pub local_system_count: usize,
    pub skills: Vec<Skill>,
    pub meta: ContentMeta,
    pub tools: Vec<String>,
}

impl AillyRc {
    /// Read `.ailly.toml` at `dir` and merge it into `prior` per the `parent` mode.
    pub async fn load(
        dir: &VfsPath,
        prior: AillyRc,
        skills_repo: &dyn SkillRepository,
    ) -> Result<Self, ContentError> {
        let AillyRc {
            mut system,
            local_system_count: _prior_local_count,
            mut skills,
            mut meta,
            mut tools,
        } = prior;

        let path = dir
            .join(AILLYRC)
            .map_err(|source| ContentError::ResolvePath {
                path: dir.as_str().to_string(),
                source,
            })?;
        let exists = path.exists().map_err(|source| ContentError::CheckExists {
            path: path.as_str().to_string(),
            source,
        })?;
        let file: AillyRcFile = if exists {
            let text = path.read_to_string().map_err(|source| ContentError::Read {
                path: path.as_str().to_string(),
                source,
            })?;
            toml::from_str(&text).map_err(|source| ContentError::ParseToml {
                path: path.as_str().to_string(),
                source,
            })?
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

        let local_skill_names: Vec<String> = file.skills.clone().unwrap_or_default();

        match meta.parent {
            Parent::Root => {}
            Parent::Never => {
                system.clear();
                skills.clear();
            }
            Parent::Always => {
                if system.is_empty() && !dir.is_root() {
                    let parent_dir = dir.parent();
                    let recursed = Box::pin(AillyRc::load(
                        &parent_dir,
                        AillyRc {
                            system: Vec::new(),
                            local_system_count: 0,
                            skills: Vec::new(),
                            meta: meta.clone(),
                            tools: Vec::new(),
                        },
                        skills_repo,
                    ))
                    .await?;
                    system = recursed.system;
                    if skills.is_empty() {
                        skills = recursed.skills;
                    }
                }
            }
        }

        let mut local_system_count = 0usize;
        if let Some(m) = local {
            system.push(m);
            local_system_count = 1;
        }

        if !local_skill_names.is_empty() {
            let origin = path.as_str().to_string();
            for raw in local_skill_names {
                let parsed = SkillName::try_from(&raw).map_err(|err| ContentError::Skill {
                    name: raw.clone(),
                    origin: origin.clone(),
                    source: err,
                })?;
                let skill = skills_repo
                    .get(&parsed)
                    .map_err(|source| ContentError::Skill {
                        name: parsed.as_str().to_string(),
                        origin: origin.clone(),
                        source,
                    })?;
                skills.push(skill);
            }
        }

        tools = combine_tools(tools, &file.tools, file.parent_tools);

        Ok(AillyRc {
            system,
            local_system_count,
            skills,
            meta,
            tools,
        })
    }
}

/// Combine an inherited `tools` chain with a local declaration per
/// `parent_tools`. `Extend` (the default) appends new names while preserving
/// declaration order and dropping duplicates; `Replace` discards the inherited
/// chain and uses only the local names (also deduped, preserving order).
fn combine_tools(
    inherited: Vec<String>,
    local: &[String],
    parent: Option<ToolsParent>,
) -> Vec<String> {
    let mut out = match parent.unwrap_or_default() {
        ToolsParent::Extend => inherited,
        ToolsParent::Replace => Vec::new(),
    };
    for name in local {
        if !out.iter().any(|n| n == name) {
            out.push(name.clone());
        }
    }
    out
}

/// On-disk format of a `.ailly.toml` file. Each meta field is `Option`
/// so that fields absent from the file do not clobber inherited values.
#[derive(Debug, Default, Deserialize)]
struct AillyRcFile {
    #[serde(default)]
    system: Option<String>,
    #[serde(default)]
    skills: Option<Vec<String>>,
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
    #[serde(default)]
    tools: Vec<String>,
    #[serde(default)]
    parent_tools: Option<ToolsParent>,
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
    use crate::knowledge::skills::{FsSkillRepository, NullSkillRepository};
    use crate::mem_fs;
    use rig::message::AssistantContent;

    fn test_skills(fs: &VfsPath) -> FsSkillRepository {
        FsSkillRepository::new(fs)
    }

    #[tokio::test]
    async fn at_root_with_no_aillyrc_in_cwd() {
        let fs = mem_fs! { "root": {} };
        let cwd = fs.join("root").unwrap();

        let acc = AillyRc::load(&cwd, AillyRc::default(), &NullSkillRepository)
            .await
            .unwrap();

        let texts: Vec<String> = acc.system.iter().map(message_text).collect();
        assert_eq!(texts, Vec::<String>::new());
    }

    #[tokio::test]
    async fn at_root_with_aillyrc_in_cwd() {
        let fs = mem_fs! {
            "root": { ".ailly.toml": r#"system = "system""# },
        };
        let cwd = fs.join("root").unwrap();

        let acc = AillyRc::load(&cwd, AillyRc::default(), &NullSkillRepository)
            .await
            .unwrap();

        let texts: Vec<String> = acc.system.iter().map(message_text).collect();
        assert_eq!(texts, vec!["system".to_string()]);
    }

    #[tokio::test]
    async fn below_root_with_no_aillyrc_carries_parent_system() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": r#"system = "system""#,
                "below": {},
            },
        };
        let cwd = fs.join("root/below").unwrap();

        let prior = AillyRc {
            system: vec![Message::system("root")],
            local_system_count: 0,
            skills: Vec::new(),
            meta: ContentMeta::default(),
            tools: Vec::new(),
        };
        let acc = AillyRc::load(&cwd, prior, &NullSkillRepository)
            .await
            .unwrap();

        let texts: Vec<String> = acc.system.iter().map(message_text).collect();
        assert_eq!(texts, vec!["root".to_string()]);
    }

    #[tokio::test]
    async fn below_root_with_aillyrc_appends() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": r#"system = "root""#,
                "below": { ".ailly.toml": r#"system = "below""# },
            },
        };
        let cwd = fs.join("root/below").unwrap();

        let prior = AillyRc {
            system: vec![Message::system("root")],
            local_system_count: 0,
            skills: Vec::new(),
            meta: ContentMeta::default(),
            tools: Vec::new(),
        };
        let acc = AillyRc::load(&cwd, prior, &NullSkillRepository)
            .await
            .unwrap();

        let texts: Vec<String> = acc.system.iter().map(message_text).collect();
        assert_eq!(texts, vec!["root".to_string(), "below".to_string()]);
    }

    #[tokio::test]
    async fn always_pulls_parent_when_system_empty() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": r#"system = "root""#,
                "below": { ".ailly.toml": r#"system = "below""# },
            },
        };
        let cwd = fs.join("root/below").unwrap();

        let prior = AillyRc {
            system: Vec::new(),
            local_system_count: 0,
            skills: Vec::new(),
            meta: ContentMeta {
                parent: Parent::Always,
                ..Default::default()
            },
            tools: Vec::new(),
        };
        let acc = AillyRc::load(&cwd, prior, &NullSkillRepository)
            .await
            .unwrap();

        let texts: Vec<String> = acc.system.iter().map(message_text).collect();
        assert_eq!(texts, vec!["root".to_string(), "below".to_string()]);
    }

    #[tokio::test]
    async fn always_chains_three_levels() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": r#"system = "root""#,
                "below": {
                    ".ailly.toml": "parent = \"always\"\nsystem = \"below\"\n",
                    "deep": {
                        ".ailly.toml": "parent = \"always\"\nsystem = \"deep\"\n",
                    },
                },
            },
        };
        let cwd = fs.join("root/below/deep").unwrap();

        let prior = AillyRc {
            system: Vec::new(),
            local_system_count: 0,
            skills: Vec::new(),
            meta: ContentMeta {
                parent: Parent::Always,
                ..Default::default()
            },
            tools: Vec::new(),
        };
        let acc = AillyRc::load(&cwd, prior, &NullSkillRepository)
            .await
            .unwrap();

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
                ".ailly.toml": r#"system = "root""#,
                "below": {
                    "deep": {
                        ".ailly.toml": "parent = \"always\"\nsystem = \"deep\"\n",
                    },
                },
            },
        };
        let cwd = fs.join("root/below/deep").unwrap();

        let prior = AillyRc {
            system: Vec::new(),
            local_system_count: 0,
            skills: Vec::new(),
            meta: ContentMeta {
                parent: Parent::Always,
                ..Default::default()
            },
            tools: Vec::new(),
        };
        let acc = AillyRc::load(&cwd, prior, &NullSkillRepository)
            .await
            .unwrap();

        let texts: Vec<String> = acc.system.iter().map(message_text).collect();
        assert_eq!(texts, vec!["deep".to_string()]);
    }

    #[tokio::test]
    async fn always_keeps_existing_system_and_appends() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": r#"system = "root""#,
                "below": {
                    "deep": {
                        ".ailly.toml": "parent = \"always\"\nsystem = \"deep\"\n",
                    },
                },
            },
        };
        let cwd = fs.join("root/below/deep").unwrap();

        let prior = AillyRc {
            system: vec![Message::system("below")],
            local_system_count: 0,
            skills: Vec::new(),
            meta: ContentMeta {
                parent: Parent::Always,
                ..Default::default()
            },
            tools: Vec::new(),
        };
        let acc = AillyRc::load(&cwd, prior, &NullSkillRepository)
            .await
            .unwrap();

        let texts: Vec<String> = acc.system.iter().map(message_text).collect();
        assert_eq!(texts, vec!["below".to_string(), "deep".to_string()]);
    }

    #[tokio::test]
    async fn never_replaces_inherited_with_local() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": r#"system = "root""#,
                "below": { ".ailly.toml": r#"system = "below""# },
            },
        };
        let cwd = fs.join("root/below").unwrap();

        let prior = AillyRc {
            system: vec![Message::system("root")],
            local_system_count: 0,
            skills: Vec::new(),
            meta: ContentMeta {
                parent: Parent::Never,
                ..Default::default()
            },
            tools: Vec::new(),
        };
        let acc = AillyRc::load(&cwd, prior, &NullSkillRepository)
            .await
            .unwrap();

        let texts: Vec<String> = acc.system.iter().map(message_text).collect();
        assert_eq!(texts, vec!["below".to_string()]);
    }

    #[tokio::test]
    async fn never_with_no_local_aillyrc_drops_inherited() {
        let fs = mem_fs! { "root": { "below": {} } };
        let cwd = fs.join("root/below").unwrap();

        let prior = AillyRc {
            system: vec![Message::system("root")],
            local_system_count: 0,
            skills: Vec::new(),
            meta: ContentMeta {
                parent: Parent::Never,
                ..Default::default()
            },
            tools: Vec::new(),
        };
        let acc = AillyRc::load(&cwd, prior, &NullSkillRepository)
            .await
            .unwrap();

        let texts: Vec<String> = acc.system.iter().map(message_text).collect();
        assert_eq!(texts, Vec::<String>::new());
    }

    #[tokio::test]
    async fn tools_chain_extends_by_default() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": r#"tools = ["b"]"#,
            },
        };
        let cwd = fs.join("root").unwrap();
        let prior = AillyRc {
            system: Vec::new(),
            meta: ContentMeta::default(),
            tools: vec!["a".to_string()],
            skills: vec![],
            local_system_count: 0,
        };
        let acc = AillyRc::load(&cwd, prior, &NullSkillRepository {})
            .await
            .unwrap();
        assert_eq!(acc.tools, vec!["a".to_string(), "b".to_string()]);
    }

    #[tokio::test]
    async fn tools_chain_replace_drops_inherited() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": "tools = [\"b\"]\nparent_tools = \"replace\"\n",
            },
        };
        let cwd = fs.join("root").unwrap();
        let prior = AillyRc {
            system: Vec::new(),
            meta: ContentMeta::default(),
            tools: vec!["a".to_string()],
            skills: vec![],
            local_system_count: 0,
        };
        let acc = AillyRc::load(&cwd, prior, &NullSkillRepository {})
            .await
            .unwrap();
        assert_eq!(acc.tools, vec!["b".to_string()]);
    }

    #[tokio::test]
    async fn tools_chain_extend_dedupes_in_declaration_order() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": r#"tools = ["a", "b"]"#,
            },
        };
        let cwd = fs.join("root").unwrap();
        let prior = AillyRc {
            system: Vec::new(),
            meta: ContentMeta::default(),
            tools: vec!["a".to_string()],
            skills: vec![],
            local_system_count: 0,
        };
        let acc = AillyRc::load(&cwd, prior, &NullSkillRepository {})
            .await
            .unwrap();
        assert_eq!(acc.tools, vec!["a".to_string(), "b".to_string()]);
    }

    #[tokio::test]
    async fn tools_chain_omitted_field_inherits_unchanged() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": r#"system = "s""#,
            },
        };
        let cwd = fs.join("root").unwrap();
        let prior = AillyRc {
            system: Vec::new(),
            meta: ContentMeta::default(),
            tools: vec!["a".to_string()],
            skills: vec![],
            local_system_count: 0,
        };
        let acc = AillyRc::load(&cwd, prior, &NullSkillRepository {})
            .await
            .unwrap();
        assert_eq!(acc.tools, vec!["a".to_string()]);
    }

    #[tokio::test]
    async fn turn_tools_extend_inherited_chain_by_default() {
        let fs = mem_fs! {
            "root": { "01.toml": "prompt = \"hi\"\ntools = [\"b\"]\n" },
        };
        let path = fs.join("root/01.toml").unwrap();
        let turn = ConversationTurn::load(
            path,
            ContentMeta::default(),
            Vec::new(),
            0,
            vec![],
            vec!["a".to_string()],
        )
        .await
        .unwrap();
        assert_eq!(turn.tool_names(), &["a".to_string(), "b".to_string()]);
    }

    #[tokio::test]
    async fn turn_tools_replace_drops_inherited_chain() {
        let fs = mem_fs! {
            "root": {
                "01.toml": "prompt = \"hi\"\ntools = [\"b\"]\nparent_tools = \"replace\"\n",
            },
        };
        let path = fs.join("root/01.toml").unwrap();
        let turn = ConversationTurn::load(
            path,
            ContentMeta::default(),
            Vec::new(),
            0,
            vec![],
            vec!["a".to_string()],
        )
        .await
        .unwrap();
        assert_eq!(turn.tool_names(), &["b".to_string()]);
    }

    #[tokio::test]
    async fn turn_tools_omitted_inherits_chain_unchanged() {
        let fs = mem_fs! {
            "root": { "01.toml": r#"prompt = "hi""# },
        };
        let path = fs.join("root/01.toml").unwrap();
        let turn = ConversationTurn::load(
            path,
            ContentMeta::default(),
            Vec::new(),
            0,
            vec![],
            vec!["a".to_string()],
        )
        .await
        .unwrap();
        assert_eq!(turn.tool_names(), &["a".to_string()]);
    }

    #[tokio::test]
    async fn turn_file_without_tools_round_trips_byte_stably() {
        let fs = mem_fs! {
            "root": { "01.toml": "prompt = \"q\"\n" },
        };
        let path = fs.join("root/01.toml").unwrap();
        let original = path.read_to_string().unwrap();
        let turn = ConversationTurn::load(
            path.clone(),
            ContentMeta::default(),
            Vec::new(),
            0,
            vec![],
            Vec::new(),
        )
        .await
        .unwrap();
        turn.write().await.unwrap();
        assert_eq!(path.read_to_string().unwrap(), original);
    }

    #[tokio::test]
    async fn turn_file_with_tools_and_parent_tools_round_trips_byte_stably() {
        let fs = mem_fs! {
            "root": {
                "01.toml": "prompt = \"q\"\ntools = [\"x\"]\nparent_tools = \"replace\"\n",
            },
        };
        let path = fs.join("root/01.toml").unwrap();
        let original = path.read_to_string().unwrap();
        let turn = ConversationTurn::load(
            path.clone(),
            ContentMeta::default(),
            Vec::new(),
            0,
            Vec::new(),
            Vec::new(),
        )
        .await
        .unwrap();
        turn.write().await.unwrap();
        assert_eq!(path.read_to_string().unwrap(), original);
    }

    #[tokio::test]
    async fn loads_a_single_turn_with_prompt_only() {
        let fs = mem_fs! {
            "root": {
                "01_intro.toml": r#"prompt = "say hi""#,
            },
        };
        let path = fs.join("root/01_intro.toml").unwrap();
        let turn = ConversationTurn::load(
            path.clone(),
            ContentMeta::default(),
            Vec::new(),
            0,
            Vec::new(),
            Vec::new(),
        )
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
        let convo = Conversation::load(fs.join("root").unwrap(), &test_skills(&fs))
            .await
            .unwrap();
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
                ".ailly.toml": r#"system = "sys""#,
                "01.toml": r#"prompt = "x""#,
            },
        };
        let convo = Conversation::load(fs.join("root").unwrap(), &test_skills(&fs))
            .await
            .unwrap();
        let only = &convo.turns[0].1;
        assert_eq!(convo.system(only).len(), 1);
    }

    #[tokio::test]
    async fn loads_turns_recursively_parents_before_children() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": r#"system = "root-sys""#,
                "01.toml": r#"prompt = "top""#,
                "child": {
                    "02.toml": r#"prompt = "deep""#,
                },
            },
        };
        let convo = Conversation::load(fs.join("root").unwrap(), &test_skills(&fs))
            .await
            .unwrap();
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
                ".ailly.toml": r#"system = "root-sys""#,
                "child": {
                    ".ailly.toml": r#"system = "child-sys""#,
                    "01.toml": r#"prompt = "x""#,
                },
            },
        };
        let convo = Conversation::load(fs.join("root").unwrap(), &test_skills(&fs))
            .await
            .unwrap();
        assert_eq!(convo.turns.len(), 1);
        assert_eq!(convo.turns[0].1.system.len(), 2);
    }

    #[tokio::test]
    async fn loads_directory_of_turns_in_lexicographic_order() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": r#"system = "sys""#,
                "02_b.toml": r#"prompt = "second""#,
                "01_a.toml": r#"prompt = "first""#,
            },
        };
        let dir = fs.join("root").unwrap();
        let convo = Conversation::load(dir, &test_skills(&fs)).await.unwrap();
        assert_eq!(convo.turns.len(), 2);
        assert!(convo.turns[0].0.as_str().ends_with("01_a.toml"));
        assert!(convo.turns[1].0.as_str().ends_with("02_b.toml"));
        assert_eq!(convo.turns[0].1.system.len(), 1);
    }

    #[tokio::test]
    async fn skips_directory_when_meta_skip_true() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": "skip = true\n",
                "01.toml": r#"prompt = "x""#,
            },
        };
        let convo = Conversation::load(fs.join("root").unwrap(), &test_skills(&fs))
            .await
            .unwrap();
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
        let turn = ConversationTurn::load(
            path,
            ContentMeta::default(),
            Vec::new(),
            0,
            Vec::new(),
            Vec::new(),
        )
        .await
        .unwrap();
        assert_eq!(turn.response.len(), 1);
    }

    #[tokio::test]
    async fn rejects_turn_with_empty_prompt() {
        let fs = mem_fs! { "root": { "01.toml": r#"prompt = """# } };
        let path = fs.join("root/01.toml").unwrap();
        let err = ConversationTurn::load(
            path,
            ContentMeta::default(),
            Vec::new(),
            0,
            Vec::new(),
            Vec::new(),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("empty prompt"));
    }

    #[tokio::test]
    async fn carries_system_passed_in() {
        let fs = mem_fs! { "root": { "01.toml": r#"prompt = "x""# } };
        let path = fs.join("root/01.toml").unwrap();
        let sys = vec![Message::system("inherited")];
        let turn = ConversationTurn::load(
            path,
            ContentMeta::default(),
            sys.clone(),
            0,
            Vec::new(),
            Vec::new(),
        )
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
            tools: Vec::new(),
            parent_tools: None,
            response: vec![MessageFile::Assistant {
                text: "hi".to_string(),
                model: None,
                engine: None,
                stop_reason: None,
                usage: None,
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
            tools: Vec::new(),
            parent_tools: None,
            response: Vec::new(),
        };
        let toml_text = toml::to_string(&file).unwrap();
        assert!(!toml_text.contains("response"));
    }

    #[test]
    fn assistant_message_file_round_trips_with_all_metadata() {
        let file = ConversationTurnFile {
            prompt: "q".to_string(),
            tools: Vec::new(),
            parent_tools: None,
            response: vec![MessageFile::Assistant {
                text: "a".to_string(),
                model: Some("test-model".to_string()),
                engine: Some("noop".to_string()),
                stop_reason: Some("end_turn".to_string()),
                usage: Some(UsageFile {
                    input_tokens: 11,
                    output_tokens: 22,
                }),
            }],
        };

        let toml_text = toml::to_string(&file).unwrap();
        let parsed: ConversationTurnFile = toml::from_str(&toml_text).unwrap();

        let MessageFile::Assistant {
            text,
            model,
            engine,
            stop_reason,
            usage,
        } = parsed.response.into_iter().next().unwrap()
        else {
            panic!("expected an Assistant entry");
        };
        assert_eq!(text, "a");
        assert_eq!(model.as_deref(), Some("test-model"));
        assert_eq!(engine.as_deref(), Some("noop"));
        assert_eq!(stop_reason.as_deref(), Some("end_turn"));
        let u = usage.expect("usage round-trips");
        assert_eq!(u.input_tokens, 11);
        assert_eq!(u.output_tokens, 22);
    }

    #[test]
    fn assistant_message_file_round_trips_without_metadata() {
        let toml_text = "prompt = \"q\"\n\n[[response]]\nrole = \"assistant\"\ntext = \"a\"\n";
        let parsed: ConversationTurnFile = toml::from_str(toml_text).unwrap();

        let MessageFile::Assistant {
            text,
            model,
            engine,
            stop_reason,
            usage,
        } = parsed.response.into_iter().next().unwrap()
        else {
            panic!("expected an Assistant entry");
        };
        assert_eq!(text, "a");
        assert!(model.is_none());
        assert!(engine.is_none());
        assert!(stop_reason.is_none());
        assert!(usage.is_none());
    }

    #[tokio::test]
    async fn clean_strips_responses_and_is_byte_identical_on_second_call() {
        let fs = mem_fs! {
            "root": {
                "01.toml": "prompt = \"q\"\n\n[[response]]\nrole = \"assistant\"\ntext = \"a1\"\n\n[[response]]\nrole = \"user\"\ntext = \"u\"\n\n[[response]]\nrole = \"assistant\"\ntext = \"a2\"\n",
            },
        };
        let dir = fs.join("root").unwrap();
        let path = fs.join("root/01.toml").unwrap();

        let mut convo = Conversation::load(dir.clone(), &test_skills(&fs))
            .await
            .unwrap();
        convo.clean().await.unwrap();

        let after_first = path.read_to_string().unwrap();
        assert!(
            !after_first.contains("[[response]]"),
            "first clean kept [[response]]: {after_first}"
        );
        assert!(
            after_first.contains("prompt = \"q\""),
            "first clean dropped prompt: {after_first}"
        );

        let mut convo = Conversation::load(dir, &test_skills(&fs)).await.unwrap();
        convo.clean().await.unwrap();
        let after_second = path.read_to_string().unwrap();

        assert_eq!(
            after_first, after_second,
            "second clean should be byte-identical"
        );
    }

    #[tokio::test]
    async fn clean_skips_directory_when_meta_skip_true() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": "skip = true\n",
                "01.toml": "prompt = \"q\"\n\n[[response]]\nrole = \"assistant\"\ntext = \"a\"\n",
            },
        };
        let dir = fs.join("root").unwrap();
        let path = fs.join("root/01.toml").unwrap();
        let original = path.read_to_string().unwrap();

        let mut convo = Conversation::load(dir, &test_skills(&fs)).await.unwrap();
        assert_eq!(
            convo.turn_count(),
            0,
            "skip = true must yield zero loaded turns"
        );
        convo.clean().await.unwrap();

        let after = path.read_to_string().unwrap();
        assert_eq!(
            after, original,
            "clean must not touch files inside skip = true dirs"
        );
    }

    #[tokio::test]
    async fn clean_does_not_touch_aillyrc_files() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": "system = \"sys\"\n",
                "01.toml": "prompt = \"q\"\n\n[[response]]\nrole = \"assistant\"\ntext = \"a\"\n",
            },
        };
        let dir = fs.join("root").unwrap();
        let aillyrc = fs.join("root/.ailly.toml").unwrap();
        let original = aillyrc.read_to_string().unwrap();

        let mut convo = Conversation::load(dir, &test_skills(&fs)).await.unwrap();
        convo.clean().await.unwrap();

        let after = aillyrc.read_to_string().unwrap();
        assert_eq!(after, original, "clean must not touch .ailly.toml");
    }

    #[tokio::test]
    async fn text_only_response_load_write_is_identical() {
        let fs = mem_fs! {
            "root": {
                "01.toml": "prompt = \"q\"\n\n[[response]]\nrole = \"assistant\"\ntext = \"a\"\n",
            },
        };
        let path = fs.join("root/01.toml").unwrap();
        let original = path.read_to_string().unwrap();

        let turn = ConversationTurn::load(
            path.clone(),
            ContentMeta::default(),
            Vec::new(),
            0,
            Vec::new(),
            Vec::new(),
        )
        .await
        .unwrap();
        turn.write().await.unwrap();

        let after = path.read_to_string().unwrap();
        assert_eq!(after, original);
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
            local_system_count: 0,
            skills: Vec::new(),
            prompt,
            response: Vec::new(),
            tool_names: Vec::new(),
            declared_tools: Vec::new(),
            declared_parent_tools: None,
        };

        let err = turn.write().await.unwrap_err();
        assert!(err.to_string().contains("non-text user prompt"));
    }

    #[tokio::test]
    async fn conversation_write_surfaces_turn_error() {
        use rig::message::Image;

        let fs = mem_fs! { "root": { "01.toml": r#"prompt = "x""# } };
        let dir = fs.join("root").unwrap();
        let mut convo = Conversation::load(dir, &test_skills(&fs)).await.unwrap();

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
                ".ailly.toml": r#"system = "sys""#,
                "01.toml": r#"prompt = "first""#,
                "child": {
                    "02.toml": "prompt = \"second\"\n\n[[response]]\nrole = \"assistant\"\ntext = \"a\"\n",
                },
            },
        };
        let dir = fs.join("root").unwrap();
        let convo = Conversation::load(dir.clone(), &test_skills(&fs))
            .await
            .unwrap();
        assert_eq!(convo.turns.len(), 2);

        convo.write().await.unwrap();

        let reloaded = Conversation::load(dir, &test_skills(&fs)).await.unwrap();
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
        let turn = ConversationTurn::load(
            path.clone(),
            ContentMeta::default(),
            Vec::new(),
            0,
            Vec::new(),
            Vec::new(),
        )
        .await
        .unwrap();

        turn.write().await.unwrap();

        let reloaded = ConversationTurn::load(
            path,
            ContentMeta::default(),
            Vec::new(),
            0,
            Vec::new(),
            Vec::new(),
        )
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
        let turn = ConversationTurn::load(
            path.clone(),
            ContentMeta::default(),
            Vec::new(),
            0,
            Vec::new(),
            Vec::new(),
        )
        .await
        .unwrap();

        turn.write().await.unwrap();

        let reloaded = ConversationTurn::load(
            path,
            ContentMeta::default(),
            Vec::new(),
            0,
            Vec::new(),
            Vec::new(),
        )
        .await
        .unwrap();
        assert_eq!(reloaded.response.len(), 3);
    }

    #[tokio::test]
    async fn write_then_load_round_trips_tool_call_and_tool_result() {
        use serde_json::json;

        let fs = mem_fs! {
        "root": { "01.toml": r#"prompt = "q""# } };
        let path = fs.join("root/01.toml").unwrap();
        let mut turn = ConversationTurn::load(
            path.clone(),
            ContentMeta::default(),
            Vec::new(),
            0,
            Vec::new(),
            Vec::new(),
        )
        .await
        .unwrap();

        turn.response = vec![
            TurnMessage::Assistant(AssistantResponse {
                text: "thinking".to_string(),
                model: None,
                engine: None,
                stop_reason: None,
                usage: None,
            }),
            TurnMessage::ToolCall(ToolCall::new(
                "call_1".to_string(),
                ToolFunction::new("echo".to_string(), json!({"text": "hi"})),
            )),
            TurnMessage::ToolResult(ToolResult {
                id: "call_1".to_string(),
                call_id: None,
                content: OneOrMany::one(ToolResultContent::Text(rig::message::Text {
                    text: "hi".to_string(),
                })),
            }),
            TurnMessage::Assistant(AssistantResponse {
                text: "done".to_string(),
                model: None,
                engine: None,
                stop_reason: None,
                usage: None,
            }),
        ];
        turn.write().await.unwrap();
        let after_first = path.read_to_string().unwrap();

        let turn = ConversationTurn::load(
            path.clone(),
            ContentMeta::default(),
            Vec::new(),
            0,
            Vec::new(),
            Vec::new(),
        )
        .await
        .unwrap();
        turn.write().await.unwrap();
        let after_second = path.read_to_string().unwrap();

        assert_eq!(
            after_first, after_second,
            "load-write cycle must be byte-stable across tool_call and tool_result entries",
        );
        assert!(after_first.contains(r#"role = "tool_call""#));
        assert!(after_first.contains(r#"role = "tool_result""#));
        assert!(after_first.contains(r#"name = "echo""#));
        assert!(after_first.contains(r#"id = "call_1""#));
    }

    #[tokio::test]
    async fn write_then_load_round_trips_prompt_only() {
        let fs = mem_fs! {
            "root": { "01.toml": r#"prompt = "hello""# },
        };
        let path = fs.join("root/01.toml").unwrap();
        let turn = ConversationTurn::load(
            path.clone(),
            ContentMeta::default(),
            Vec::new(),
            0,
            Vec::new(),
            Vec::new(),
        )
        .await
        .unwrap();

        turn.write().await.unwrap();

        let reloaded = ConversationTurn::load(
            path,
            ContentMeta::default(),
            Vec::new(),
            0,
            Vec::new(),
            Vec::new(),
        )
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

    #[tokio::test]
    async fn history_for_single_turn_pushes_system_then_prompt() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": r#"system = "sys""#,
                "01.toml": r#"prompt = "first""#,
            },
        };
        let convo = Conversation::load(fs.join("root").unwrap(), &test_skills(&fs))
            .await
            .unwrap();
        let only = &convo.turns[0].1;

        let history = convo.history_for(only);

        let texts: Vec<String> = history.iter().map(message_text).collect();
        assert_eq!(texts, vec!["sys".to_string(), "first".to_string()]);
    }

    #[tokio::test]
    async fn history_for_two_turns_includes_predecessor_prompt_and_response() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": r#"system = "sys""#,
                "01.toml": "prompt = \"first\"\n\n[[response]]\nrole = \"assistant\"\ntext = \"answer1\"\n",
                "02.toml": r#"prompt = "second""#,
            },
        };
        let convo = Conversation::load(fs.join("root").unwrap(), &test_skills(&fs))
            .await
            .unwrap();
        let second = &convo.turns[1].1;

        let history = convo.history_for(second);

        let texts: Vec<String> = history.iter().map(message_text).collect();
        assert_eq!(
            texts,
            vec![
                "sys".to_string(),
                "first".to_string(),
                "answer1".to_string(),
                "second".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn history_for_skip_head_drops_inherited_system_keeps_predecessors() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": r#"system = "sys""#,
                "01.toml": "prompt = \"first\"\n\n[[response]]\nrole = \"assistant\"\ntext = \"answer1\"\n",
                "02.toml": r#"prompt = "second""#,
            },
        };
        let mut convo = Conversation::load(fs.join("root").unwrap(), &test_skills(&fs))
            .await
            .unwrap();
        convo.turns[1].1.meta.skip_head = true;
        let second = &convo.turns[1].1;

        let history = convo.history_for(second);

        let texts: Vec<String> = history.iter().map(message_text).collect();
        assert_eq!(
            texts,
            vec![
                "first".to_string(),
                "answer1".to_string(),
                "second".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn history_for_isolated_drops_predecessors_keeps_system() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": r#"system = "sys""#,
                "01.toml": "prompt = \"first\"\n\n[[response]]\nrole = \"assistant\"\ntext = \"answer1\"\n",
                "02.toml": r#"prompt = "second""#,
            },
        };
        let mut convo = Conversation::load(fs.join("root").unwrap(), &test_skills(&fs))
            .await
            .unwrap();
        convo.turns[1].1.meta.isolated = true;
        let second = &convo.turns[1].1;

        let history = convo.history_for(second);

        let texts: Vec<String> = history.iter().map(message_text).collect();
        assert_eq!(texts, vec!["sys".to_string(), "second".to_string()]);
    }

    #[tokio::test]
    async fn history_for_pops_trailing_assistant_when_continue_false() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": r#"system = "sys""#,
                "01.toml": r#"prompt = "first""#,
            },
        };
        let mut convo = Conversation::load(fs.join("root").unwrap(), &test_skills(&fs))
            .await
            .unwrap();
        convo.turns[0].1.prompt =
            OneOrMany::many([Message::user("first"), Message::assistant("partial")]).unwrap();
        let only = &convo.turns[0].1;

        let history = convo.history_for(only);

        let texts: Vec<String> = history.iter().map(message_text).collect();
        assert_eq!(texts, vec!["sys".to_string(), "first".to_string()]);
    }

    #[tokio::test]
    async fn history_for_keeps_trailing_assistant_when_continue_true() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": r#"system = "sys""#,
                "01.toml": r#"prompt = "first""#,
            },
        };
        let mut convo = Conversation::load(fs.join("root").unwrap(), &test_skills(&fs))
            .await
            .unwrap();
        convo.turns[0].1.prompt =
            OneOrMany::many([Message::user("first"), Message::assistant("partial")]).unwrap();
        convo.turns[0].1.meta.r#continue = true;
        let only = &convo.turns[0].1;

        let history = convo.history_for(only);

        let texts: Vec<String> = history.iter().map(message_text).collect();
        assert_eq!(
            texts,
            vec![
                "sys".to_string(),
                "first".to_string(),
                "partial".to_string(),
            ]
        );
    }

    /// Feature test for the Knowledge Skills slice
    /// (`docs/developer/2026-05-03-B-knowledge-skills/`).
    ///
    /// User story: an operator declares `skills = ["foo"]` in a
    /// `.ailly.toml`, drops a `SKILL.md` at `<root>/.ailly/skills/foo/`,
    /// and the resulting `ConversationTurn`'s preamble carries the skill
    /// body between the ancestor (inherited) system text and the local
    /// system text from the turn's own directory.
    #[tokio::test]
    async fn skill_body_is_injected_between_inherited_and_local_system() {
        use crate::knowledge::skills::FsSkillRepository;

        let fs = mem_fs! {
            "root": {
                ".ailly": {
                    "skills": {
                        "foo": {
                            "SKILL.md": "---\nname: foo\ndescription: a foo skill\n---\nFOO BODY\n",
                        },
                    },
                },
                "parent": {
                    ".ailly.toml": r#"system = "INHERITED""#,
                    "child": {
                        ".ailly.toml": "system = \"LOCAL\"\nskills = [\"foo\"]\n",
                        "01.toml": r#"prompt = "p""#,
                    },
                },
            },
        };

        let project_root = fs.join("root").unwrap();
        let skills = FsSkillRepository::new(&project_root);
        let convo = Conversation::load(project_root, &skills).await.unwrap();

        assert_eq!(convo.turn_count(), 1);
        let turn = convo.turn(0);
        let preamble = convo.preamble_for(turn);

        let kinds: Vec<&'static str> = preamble
            .blocks
            .iter()
            .map(|b| match b {
                PreambleBlock::InheritedSystem { .. } => "inherited",
                PreambleBlock::Skill(_) => "skill",
                PreambleBlock::LocalSystem { .. } => "local",
            })
            .collect();
        assert_eq!(
            kinds,
            vec!["inherited", "skill", "local"],
            "preamble blocks must be ordered inherited -> skill -> local"
        );

        match &preamble.blocks[0] {
            PreambleBlock::InheritedSystem { text } => assert_eq!(text, "INHERITED"),
            other => panic!("block 0 should be InheritedSystem, got {other:?}"),
        }
        match &preamble.blocks[1] {
            PreambleBlock::Skill(skill) => {
                assert_eq!(skill.name.as_str(), "foo");
                assert_eq!(skill.body.as_str(), "FOO BODY");
            }
            other => panic!("block 1 should be Skill, got {other:?}"),
        }
        match &preamble.blocks[2] {
            PreambleBlock::LocalSystem { text } => assert_eq!(text, "LOCAL"),
            other => panic!("block 2 should be LocalSystem, got {other:?}"),
        }
    }

    /// `Conversation::load` must walk the directory tree without ever
    /// touching the skills repository when no `.ailly.toml` declares
    /// `skills =`. A panicking repository proves it: any `get` call
    /// fails the test loudly.
    struct PanickingSkillRepository;

    impl crate::knowledge::skills::SkillRepository for PanickingSkillRepository {
        fn get(
            &self,
            name: &crate::knowledge::skills::SkillName,
        ) -> Result<crate::knowledge::skills::Skill, crate::knowledge::skills::SkillError> {
            panic!("SkillRepository::get called for `{name}` when no skills were declared");
        }
    }

    #[tokio::test]
    async fn no_skills_declared_performs_zero_skill_md_reads() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": r#"system = "sys""#,
                "01.toml": r#"prompt = "first""#,
                "child": {
                    ".ailly.toml": r#"system = "child""#,
                    "02.toml": r#"prompt = "second""#,
                },
            },
        };
        let project_root = fs.join("root").unwrap();

        let convo = Conversation::load(project_root, &PanickingSkillRepository)
            .await
            .unwrap();

        assert_eq!(convo.turn_count(), 2);
        for idx in 0..convo.turn_count() {
            assert!(convo.skills(convo.turn(idx)).is_empty());
        }
    }

    #[tokio::test]
    async fn multi_skill_ordering_preserves_walk_then_declaration_order() {
        let fs = mem_fs! {
            "root": {
                ".ailly": {
                    "skills": {
                        "alpha": { "SKILL.md": "---\nname: alpha\ndescription: a\n---\nA\n" },
                        "beta":  { "SKILL.md": "---\nname: beta\ndescription: b\n---\nB\n" },
                        "gamma": { "SKILL.md": "---\nname: gamma\ndescription: g\n---\nG\n" },
                        "delta": { "SKILL.md": "---\nname: delta\ndescription: d\n---\nD\n" },
                    },
                },
                ".ailly.toml": r#"skills = ["alpha", "beta"]"#,
                "child": {
                    ".ailly.toml": "skills = [\"gamma\", \"delta\"]\n",
                    "01.toml": r#"prompt = "p""#,
                },
            },
        };
        let project_root = fs.join("root").unwrap();
        let skills = FsSkillRepository::new(&project_root);

        let convo = Conversation::load(project_root, &skills).await.unwrap();

        assert_eq!(convo.turn_count(), 1);
        let turn = convo.turn(0);
        let names: Vec<&str> = convo.skills(turn).iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "beta", "gamma", "delta"]);
    }

    #[tokio::test]
    async fn missing_skill_surfaces_origin_aillyrc_path_in_error() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": r#"skills = ["does-not-exist"]"#,
                "01.toml": r#"prompt = "p""#,
            },
        };
        let project_root = fs.join("root").unwrap();
        let skills = FsSkillRepository::new(&project_root);

        let err = match Conversation::load(project_root, &skills).await {
            Ok(_) => panic!("expected ContentError::Skill"),
            Err(e) => e,
        };

        let msg = format!("{err:#}");
        assert!(
            msg.contains("does-not-exist"),
            "missing skill name absent: {msg}"
        );
        assert!(
            msg.contains(".ailly.toml"),
            "origin .ailly.toml absent: {msg}"
        );
    }

    #[tokio::test]
    async fn invalid_skill_name_surfaces_raw_name_in_error() {
        let fs = mem_fs! {
            "root": {
                ".ailly.toml": r#"skills = ["UPPER-CASE"]"#,
                "01.toml": r#"prompt = "p""#,
            },
        };
        let project_root = fs.join("root").unwrap();
        let skills = FsSkillRepository::new(&project_root);

        let err = match Conversation::load(project_root, &skills).await {
            Ok(_) => panic!("expected ContentError::Skill for invalid name"),
            Err(e) => e,
        };

        let msg = format!("{err}");
        assert!(
            msg.contains("UPPER-CASE"),
            "raw invalid name must appear in top-level error: {msg}"
        );
    }
}
