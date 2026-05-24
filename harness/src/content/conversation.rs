//! Conversation domain object and its multi-document YAML serde.
//!
//! See `docs/developer/2026-05-23-A-content-conversation/design.md` for the
//! full contract; the type shapes below mirror the schema in `DESIGN.md`.

use std::collections::BTreeMap;
use std::fmt;
use std::marker::PhantomData;

use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use serde::de::Error as DeError;

/// Matrix binding values that produced a conversation. Keys are axis names;
/// values are opaque YAML so map-shaped axes (provider, model) round-trip
/// without per-axis variants.
pub type BindingMap = BTreeMap<String, serde_yaml_ng::Value>;

macro_rules! string_newtype {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

string_newtype!(
    /// Names a provider model, e.g. `claude-opus-4-7`.
    ModelId
);

string_newtype!(
    /// The `id` on a `tool_use` block that a later `tool_result` references.
    ToolUseId
);

string_newtype!(
    /// OTEL span id on a [`Trace`].
    SpanId
);

/// A parsed conversation file: a meta header plus an ordered list of messages.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conversation {
    pub meta: Meta,
    pub session: Vec<Message>,
}

/// The first YAML document of a conversation file.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Meta {
    pub model: ModelId,
    #[serde(default, skip_serializing_if = "is_false")]
    pub debug: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assembly: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub binding: BindingMap,
}

/// Sender role for a single message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

mod sealed {
    pub trait Sealed {}
}

/// Phase marker for [`Message<P>`]. Sealed so consumers outside this crate
/// cannot invent new phases. Each phase chooses the type of [`Message::body`].
pub trait MessagePhase: sealed::Sealed {
    /// What [`Message::body`] carries at this phase.
    type Body;
}

/// Templated turn — the shape stored inside an `Assembly`. `User` carries a
/// path to a prompt file (templated against the binding at render time);
/// `Assistant` is blank by construction. No `Content` enum at this phase; the
/// turn has not yet read any file from disk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Template;
impl sealed::Sealed for Template {}
impl MessagePhase for Template {
    type Body = TurnBody;
}

/// Rendered message — the shape written to a conversation file on disk and
/// consumed by `ailly run`. Existing `Message` callers see this as the default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rendered;
impl sealed::Sealed for Rendered {}
impl MessagePhase for Rendered {
    type Body = Option<Content>;
}

/// One turn slot in an assembly's conversation template.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TurnBody {
    /// User turn: path to a prompt file, may contain `{{ var }}` placeholders.
    UserPath { path: String },
    /// Assistant turn: blank, to be filled by `ailly run`.
    AssistantBlank,
}

/// One YAML document after the meta header, parameterized by lifecycle phase.
///
/// A `Message::<Rendered> { role: Role::Assistant, body: None, .. }` is the
/// only shape that represents a blank assistant slot left by `ailly assemble`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message<P: MessagePhase = Rendered> {
    pub role: Role,
    pub body: P::Body,
    pub cache: bool,
    pub trace: Option<Trace>,
    pub _phase: PhantomData<P>,
}

/// Wire-format repr for `Message<Rendered>`: matches the historical
/// `{ role, content, cache, trace }` shape.
#[derive(Serialize, Deserialize)]
struct MessageRenderedRepr {
    role: Role,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content: Option<Content>,
    #[serde(default, skip_serializing_if = "is_false")]
    cache: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    trace: Option<Trace>,
}

impl Serialize for Message<Rendered> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        MessageRenderedRepr {
            role: self.role,
            content: self.body.clone(),
            cache: self.cache,
            trace: self.trace.clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Message<Rendered> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let repr = MessageRenderedRepr::deserialize(deserializer)?;
        Ok(Self {
            role: repr.role,
            body: repr.content,
            cache: repr.cache,
            trace: repr.trace,
            _phase: PhantomData,
        })
    }
}

/// Wire-format repr for `Message<Template>`: `{ role, path?, cache }`. The
/// custom impls enforce role-aware presence/absence of `path`.
#[derive(Serialize, Deserialize)]
struct MessageTemplateRepr {
    role: Role,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    path: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    cache: bool,
}

impl Serialize for Message<Template> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let path = match &self.body {
            TurnBody::UserPath { path } => Some(path.clone()),
            TurnBody::AssistantBlank => None,
        };
        MessageTemplateRepr {
            role: self.role,
            path,
            cache: self.cache,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Message<Template> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let repr = MessageTemplateRepr::deserialize(deserializer)?;
        let body = match (repr.role, repr.path) {
            (Role::User, Some(path)) => TurnBody::UserPath { path },
            (Role::User, None) => {
                return Err(D::Error::custom(
                    "template user turn requires a `path` field",
                ));
            }
            (Role::Assistant, None) => TurnBody::AssistantBlank,
            (Role::Assistant, Some(_)) => {
                return Err(D::Error::custom(
                    "template assistant turn must not carry a `path` field",
                ));
            }
            (other, _) => {
                return Err(D::Error::custom(format!(
                    "template turn role must be `user` or `assistant`, got `{other:?}`"
                )));
            }
        };
        Ok(Self {
            role: repr.role,
            body,
            cache: repr.cache,
            trace: None,
            _phase: PhantomData,
        })
    }
}

/// `string | ContentBlock[]` from the conversation schema.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Content {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

impl From<String> for Content {
    fn from(value: String) -> Self {
        Content::Text(value)
    }
}

impl From<Vec<ContentBlock>> for Content {
    fn from(value: Vec<ContentBlock>) -> Self {
        Content::Blocks(value)
    }
}

/// Tagged block variant; mirrors Anthropic's Messages API shapes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: ToolUseId,
        name: String,
        input: serde_yaml_ng::Value,
    },
    ToolResult {
        tool_use_id: ToolUseId,
        content: Content,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        is_error: Option<bool>,
    },
    Thinking {
        thinking: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        signature: Option<String>,
    },
    Image {
        source: ImageSource,
    },
}

/// Opaque image source; typed shape lands when an e2e fixture emits one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ImageSource(serde_yaml_ng::Value);

/// Inline per-message trace populated by `ailly run`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trace {
    pub span_id: SpanId,
    pub model: ModelId,
    pub tokens: TokenCounts,
    pub latency_ms: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<TraceEvent>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenCounts {
    pub input: u64,
    pub output: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_hit: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceEvent {
    pub name: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, serde_yaml_ng::Value>,
}

#[expect(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde skip_serializing_if requires &T"
)]
fn is_false(b: &bool) -> bool {
    !*b
}

/// Errors produced by the conversation domain object.
#[derive(thiserror::Error, Debug)]
pub enum ConversationError {
    #[error("conversation file is empty")]
    Empty,
    #[error("first document failed to deserialize as `meta`: {source}")]
    MissingMeta {
        #[source]
        source: serde_yaml_ng::Error,
    },
    #[error("message at document index {index} failed to deserialize: {source}")]
    ParseMessage {
        index: usize,
        #[source]
        source: serde_yaml_ng::Error,
    },
    #[error("failed to emit conversation as yaml: {source}")]
    Emit {
        #[source]
        source: serde_yaml_ng::Error,
    },
    #[error("message {index} is not a blank assistant slot")]
    NotBlankAssistant { index: usize },
    #[error("message index {index} out of range (session length {len})")]
    IndexOutOfRange { index: usize, len: usize },
}

impl Conversation {
    /// Parse a multi-document YAML conversation file.
    ///
    /// # Errors
    ///
    /// Returns [`ConversationError::Empty`] when `input` has no documents,
    /// [`ConversationError::MissingMeta`] when the first document is not a
    /// valid [`Meta`], and [`ConversationError::ParseMessage`] when a later
    /// document is not a valid [`Message`].
    pub fn from_yaml_str(input: &str) -> Result<Self, ConversationError> {
        if input.trim().is_empty() {
            return Err(ConversationError::Empty);
        }

        let mut documents = serde_yaml_ng::Deserializer::from_str(input);

        let meta_doc = documents.next().ok_or(ConversationError::Empty)?;
        let meta = Meta::deserialize(meta_doc)
            .map_err(|source| ConversationError::MissingMeta { source })?;

        let mut session = Vec::new();
        for (index, doc) in documents.enumerate() {
            let message = Message::deserialize(doc)
                .map_err(|source| ConversationError::ParseMessage { index, source })?;
            session.push(message);
        }

        Ok(Self { meta, session })
    }

    /// Serialize this conversation back to multi-document YAML.
    ///
    /// # Errors
    ///
    /// Returns [`ConversationError::Emit`] if the underlying YAML emitter
    /// rejects any document body.
    pub fn to_yaml_string(&self) -> Result<String, ConversationError> {
        let mut out = String::from("---\n");
        let meta_body = serde_yaml_ng::to_string(&self.meta)
            .map_err(|source| ConversationError::Emit { source })?;
        out.push_str(&meta_body);
        for message in &self.session {
            out.push_str("---\n");
            let body = serde_yaml_ng::to_string(message)
                .map_err(|source| ConversationError::Emit { source })?;
            out.push_str(&body);
        }
        Ok(out)
    }

    /// Index of the first message that is a blank assistant slot, if any.
    #[must_use]
    pub fn next_blank_assistant(&self) -> Option<usize> {
        self.session
            .iter()
            .position(|message| matches!(message.role, Role::Assistant) && message.body.is_none())
    }

    /// Fill the blank assistant slot at `index` with content and trace.
    ///
    /// # Errors
    ///
    /// Returns [`ConversationError::IndexOutOfRange`] when `index` is past the
    /// end of `session`, and [`ConversationError::NotBlankAssistant`] when the
    /// message at `index` is not a blank assistant slot.
    pub fn fill_blank_assistant(
        &mut self,
        index: usize,
        content: Content,
        trace: Trace,
    ) -> Result<(), ConversationError> {
        let len = self.session.len();
        let message = self
            .session
            .get_mut(index)
            .ok_or(ConversationError::IndexOutOfRange { index, len })?;
        if !matches!(message.role, Role::Assistant) || message.body.is_some() {
            return Err(ConversationError::NotBlankAssistant { index });
        }
        message.body = Some(content);
        message.trace = Some(trace);
        Ok(())
    }

    /// Prefix of messages preceding `index`; saturates at `session.len()`.
    #[must_use]
    pub fn messages_up_to(&self, index: usize) -> &[Message] {
        let bounded = index.min(self.session.len());
        &self.session[..bounded]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const THREE_MESSAGE_FIXTURE: &str = "\
---
model: claude-opus-4-7
assembly: claim-handler
binding:
  domain: prose-bio
---
role: system
content: You classify insurance claims.
cache: true
---
role: user
content:
  - type: text
    text: \"Claim 42: a tree fell on my fence.\"
---
role: assistant
";

    #[test]
    fn parses_minimal_three_message_fixture() {
        let conv = Conversation::from_yaml_str(THREE_MESSAGE_FIXTURE).expect("fixture parses");

        assert_eq!(conv.meta.model.as_ref(), "claude-opus-4-7");
        assert_eq!(conv.meta.assembly.as_deref(), Some("claim-handler"));
        assert_eq!(conv.session.len(), 3);
        assert!(matches!(conv.session[0].role, Role::System));
        assert!(conv.session[0].cache);
        assert!(matches!(conv.session[1].role, Role::User));
        assert!(matches!(conv.session[2].role, Role::Assistant));
        assert!(conv.session[2].body.is_none());
    }

    #[test]
    fn empty_input_returns_empty_error() {
        let err = Conversation::from_yaml_str("").expect_err("empty input rejected");
        assert!(matches!(err, ConversationError::Empty), "got {err:?}");
    }

    #[test]
    fn garbage_meta_returns_missing_meta() {
        let err = Conversation::from_yaml_str("---\n42\n").expect_err("garbage meta rejected");
        assert!(
            matches!(err, ConversationError::MissingMeta { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn malformed_message_reports_zero_based_index() {
        let yaml = "\
---
model: claude-opus-4-7
---
role: system
content: ok
---
role: not-a-role
";
        let err = Conversation::from_yaml_str(yaml).expect_err("bad role rejected");
        match err {
            ConversationError::ParseMessage { index, .. } => assert_eq!(index, 1),
            other => panic!("expected ParseMessage {{ index: 1, .. }}, got {other:?}"),
        }
    }

    fn meta() -> Meta {
        Meta {
            model: ModelId::from("claude-opus-4-7"),
            debug: false,
            assembly: None,
            binding: BindingMap::new(),
        }
    }

    fn message(role: Role, body: Option<Content>) -> Message {
        Message {
            role,
            body,
            cache: false,
            trace: None,
            _phase: PhantomData,
        }
    }

    #[test]
    fn next_blank_assistant_returns_none_when_no_assistant_present() {
        let conv = Conversation {
            meta: meta(),
            session: vec![
                message(Role::System, Some(Content::from(String::from("s")))),
                message(Role::User, Some(Content::from(String::from("u")))),
            ],
        };
        assert!(conv.next_blank_assistant().is_none());
    }

    #[test]
    fn next_blank_assistant_skips_filled_assistants() {
        let conv = Conversation {
            meta: meta(),
            session: vec![
                message(Role::Assistant, Some(Content::from(String::from("done")))),
                message(Role::User, Some(Content::from(String::from("u")))),
                message(Role::Assistant, None),
            ],
        };
        assert_eq!(conv.next_blank_assistant(), Some(2));
    }

    fn sample_trace() -> Trace {
        Trace {
            span_id: SpanId::from("span-001"),
            model: ModelId::from("claude-opus-4-7"),
            tokens: TokenCounts {
                input: 1,
                output: 1,
                cache_hit: None,
                cache_write: None,
            },
            latency_ms: 0,
            events: Vec::new(),
        }
    }

    #[test]
    fn fill_blank_assistant_sets_content_and_trace_only() {
        let mut conv = Conversation {
            meta: meta(),
            session: vec![
                Message {
                    role: Role::System,
                    body: Some(Content::from(String::from("s"))),
                    cache: true,
                    trace: None,
                    _phase: PhantomData,
                },
                message(Role::Assistant, None),
            ],
        };
        let prefix_snapshot = conv.session[0].clone();

        conv.fill_blank_assistant(1, Content::from(String::from("hi")), sample_trace())
            .expect("blank assistant accepts fill");

        assert_eq!(conv.session[0], prefix_snapshot);
        let filled = &conv.session[1];
        assert!(matches!(filled.role, Role::Assistant));
        assert!(!filled.cache);
        assert!(matches!(filled.body, Some(Content::Text(ref t)) if t == "hi"));
        assert!(filled.trace.is_some());
    }

    #[test]
    fn fill_blank_assistant_rejects_user_message() {
        let mut conv = Conversation {
            meta: meta(),
            session: vec![message(Role::User, Some(Content::from(String::from("u"))))],
        };
        let err = conv
            .fill_blank_assistant(0, Content::from(String::from("x")), sample_trace())
            .expect_err("user message rejected");
        assert!(matches!(
            err,
            ConversationError::NotBlankAssistant { index: 0 }
        ));
    }

    #[test]
    fn fill_blank_assistant_rejects_filled_assistant() {
        let mut conv = Conversation {
            meta: meta(),
            session: vec![message(
                Role::Assistant,
                Some(Content::from(String::from("done"))),
            )],
        };
        let err = conv
            .fill_blank_assistant(0, Content::from(String::from("x")), sample_trace())
            .expect_err("filled assistant rejected");
        assert!(matches!(
            err,
            ConversationError::NotBlankAssistant { index: 0 }
        ));
    }

    #[test]
    fn fill_blank_assistant_rejects_out_of_range_index() {
        let mut conv = Conversation {
            meta: meta(),
            session: vec![message(Role::Assistant, None)],
        };
        let err = conv
            .fill_blank_assistant(5, Content::from(String::from("x")), sample_trace())
            .expect_err("out of range rejected");
        assert!(matches!(
            err,
            ConversationError::IndexOutOfRange { index: 5, len: 1 }
        ));
    }

    #[test]
    fn blank_assistant_serializes_to_role_only_document() {
        let conv = Conversation {
            meta: meta(),
            session: vec![message(Role::Assistant, None)],
        };
        let emitted = conv.to_yaml_string().expect("emits");
        assert_eq!(
            emitted,
            "---\nmodel: claude-opus-4-7\n---\nrole: assistant\n"
        );
    }

    #[test]
    fn round_trip_preserves_three_message_fixture() {
        let conv = Conversation::from_yaml_str(THREE_MESSAGE_FIXTURE).expect("fixture parses");
        let emitted = conv.to_yaml_string().expect("emits");
        let reparsed = Conversation::from_yaml_str(&emitted).expect("emitted re-parses");
        assert_eq!(reparsed, conv);
    }

    #[test]
    fn meta_defaults_are_skipped_on_emit() {
        let conv = Conversation {
            meta: meta(),
            session: Vec::new(),
        };
        let emitted = conv.to_yaml_string().expect("emits");
        assert!(!emitted.contains("debug:"));
        assert!(!emitted.contains("assembly:"));
        assert!(!emitted.contains("binding:"));
    }

    #[test]
    fn template_user_turn_parses_path() {
        let yaml = "role: user\npath: prompts/{{ case }}.md\n";
        let msg: Message<Template> = serde_yaml_ng::from_str(yaml).expect("user template parses");
        assert!(matches!(msg.role, Role::User));
        match msg.body {
            TurnBody::UserPath { path } => assert_eq!(path, "prompts/{{ case }}.md"),
            TurnBody::AssistantBlank => panic!("expected UserPath, got AssistantBlank"),
        }
    }

    #[test]
    fn template_assistant_turn_parses_blank() {
        let yaml = "role: assistant\n";
        let msg: Message<Template> =
            serde_yaml_ng::from_str(yaml).expect("assistant template parses");
        assert!(matches!(msg.role, Role::Assistant));
        assert!(matches!(msg.body, TurnBody::AssistantBlank));
    }

    #[test]
    fn template_user_turn_without_path_is_rejected() {
        let yaml = "role: user\n";
        let err: serde_yaml_ng::Error =
            serde_yaml_ng::from_str::<Message<Template>>(yaml).expect_err("user without path");
        assert!(
            err.to_string().contains("user turn requires a `path`"),
            "got {err}"
        );
    }

    #[test]
    fn template_assistant_turn_with_path_is_rejected() {
        let yaml = "role: assistant\npath: prompts/x.md\n";
        let err: serde_yaml_ng::Error =
            serde_yaml_ng::from_str::<Message<Template>>(yaml).expect_err("assistant with path");
        assert!(
            err.to_string().contains("must not carry a `path`"),
            "got {err}"
        );
    }

    #[test]
    fn template_round_trips_through_yaml() {
        let original: Message<Template> = Message {
            role: Role::User,
            body: TurnBody::UserPath {
                path: String::from("prompts/{{ case }}.md"),
            },
            cache: false,
            trace: None,
            _phase: PhantomData,
        };
        let emitted = serde_yaml_ng::to_string(&original).expect("emit");
        let reparsed: Message<Template> = serde_yaml_ng::from_str(&emitted).expect("reparse");
        assert_eq!(reparsed, original);
    }

    #[test]
    fn messages_up_to_saturates_and_bounds_correctly() {
        let conv = Conversation {
            meta: meta(),
            session: vec![
                message(Role::System, Some(Content::from(String::from("s")))),
                message(Role::User, Some(Content::from(String::from("u")))),
                message(Role::Assistant, None),
            ],
        };
        assert!(conv.messages_up_to(0).is_empty());
        assert_eq!(conv.messages_up_to(2).len(), 2);
        assert_eq!(conv.messages_up_to(3).len(), 3);
        assert_eq!(conv.messages_up_to(99).len(), 3);
    }
}
