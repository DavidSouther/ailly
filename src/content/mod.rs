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
    prompt: OneOrMany<UserContent>,
    response: Option<OneOrMany<AssistantContent>>,
}

/// Accumulated state from walking up `.aillyrc.toml` files.
#[derive(Debug, Default, Clone)]
pub struct AillyRc {
    pub system: Vec<Message>,
    pub meta: ContentMeta,
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

/// Read `.aillyrc.toml` at `dir` and merge it into `prior` per the `parent` mode.
///
/// Mirrors the TypeScript `loadAillyRc` from
/// `ailly_typescript/core/src/content/content.ts`.
pub async fn load_aillyrc(dir: &VfsPath, prior: AillyRc) -> Result<AillyRc> {
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
                let recursed = Box::pin(load_aillyrc(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::test_util::mem_fs;

    #[tokio::test]
    async fn at_root_with_no_aillyrc_in_cwd() {
        let fs = mem_fs! { "root": {} };
        let cwd = fs.join("root").unwrap();

        let acc = load_aillyrc(&cwd, AillyRc::default()).await.unwrap();

        let texts: Vec<String> = acc.system.iter().map(message_text).collect();
        assert_eq!(texts, Vec::<String>::new());
    }

    #[tokio::test]
    async fn at_root_with_aillyrc_in_cwd() {
        let fs = mem_fs! {
            "root": { ".aillyrc.toml": r#"system = "system""# },
        };
        let cwd = fs.join("root").unwrap();

        let acc = load_aillyrc(&cwd, AillyRc::default()).await.unwrap();

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
        let acc = load_aillyrc(&cwd, prior).await.unwrap();

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
        let acc = load_aillyrc(&cwd, prior).await.unwrap();

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
        let acc = load_aillyrc(&cwd, prior).await.unwrap();

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
        let acc = load_aillyrc(&cwd, prior).await.unwrap();

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
        let acc = load_aillyrc(&cwd, prior).await.unwrap();

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
        let acc = load_aillyrc(&cwd, prior).await.unwrap();

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
        let acc = load_aillyrc(&cwd, prior).await.unwrap();

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
        let acc = load_aillyrc(&cwd, prior).await.unwrap();

        let texts: Vec<String> = acc.system.iter().map(message_text).collect();
        assert_eq!(texts, Vec::<String>::new());
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
