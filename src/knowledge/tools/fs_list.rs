use rig::completion::ToolDefinition;
use rig::tool::Tool;
use vfs::VfsPath;

use super::root::{ResolveError, resolve_under_root};
use super::walk::{EntryKind, WalkError, walk_under};

pub struct FsList {
    root: VfsPath,
}

impl FsList {
    pub const NAME: &'static str = "fs-list";

    pub fn new(project: &crate::project::ProjectRoot) -> Self {
        Self {
            root: project.as_path().clone(),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct FsListArgs {
    pub path: String,
    #[serde(default)]
    pub glob: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum FsListError {
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    #[error("path {path} is not a directory")]
    NotADirectory { path: String },
    #[error("reading directory {path}")]
    ReadDir {
        path: String,
        #[source]
        source: vfs::VfsError,
    },
    #[error("invalid glob {glob}")]
    InvalidGlob {
        glob: String,
        #[source]
        source: globset::Error,
    },
}

impl From<WalkError> for FsListError {
    fn from(err: WalkError) -> Self {
        FsListError::ReadDir {
            path: err.path,
            source: err.source,
        }
    }
}

#[derive(Debug, serde::Serialize)]
struct ListEntry {
    name: String,
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<u64>,
}

impl Tool for FsList {
    const NAME: &'static str = "fs-list";

    type Error = FsListError;
    type Args = FsListArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "List directory entries under the captured root. \
                Returns a JSON array sorted directories-first then files. \
                A glob whose first segment is `**` recurses; anything else \
                lists only the immediate children of `path`."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Directory path relative to the captured root."
                    },
                    "glob": {
                        "type": "string",
                        "description": "Optional gitignore-flavored glob filter."
                    }
                },
                "required": ["path"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let dir = resolve_under_root(&self.root, &args.path)?;
        if !is_dir(&dir) {
            return Err(FsListError::NotADirectory {
                path: args.path.clone(),
            });
        }
        let recursive = args
            .glob
            .as_deref()
            .map(|g| g.starts_with("**"))
            .unwrap_or(false);
        let matcher = match args.glob.as_deref() {
            Some(pattern) => Some(
                globset::Glob::new(pattern)
                    .map_err(|source| FsListError::InvalidGlob {
                        glob: pattern.to_string(),
                        source,
                    })?
                    .compile_matcher(),
            ),
            None => None,
        };

        let mut entries = walk_under(&dir, recursive, &args.path)?;

        if let Some(matcher) = matcher.as_ref() {
            entries.retain(|(path, _)| matcher.is_match(path.as_str()));
        }

        let mut listed: Vec<ListEntry> = entries
            .into_iter()
            .map(|(path, kind)| {
                let name = path.filename();
                let (kind_label, size) = match kind {
                    EntryKind::File => ("file", path.metadata().ok().map(|m| m.len)),
                    EntryKind::Dir => ("dir", None),
                };
                ListEntry {
                    name,
                    kind: kind_label,
                    size,
                }
            })
            .collect();

        listed.sort_by(|a, b| match (a.kind, b.kind) {
            ("dir", "file") => std::cmp::Ordering::Less,
            ("file", "dir") => std::cmp::Ordering::Greater,
            _ => a.name.cmp(&b.name),
        });

        Ok(serde_json::to_string(&listed).expect("ListEntry serializes"))
    }
}

fn is_dir(path: &VfsPath) -> bool {
    path.metadata()
        .map(|m| matches!(m.file_type, vfs::VfsFileType::Directory))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem_fs;

    fn args(path: &str, glob: Option<&str>) -> FsListArgs {
        FsListArgs {
            path: path.to_string(),
            glob: glob.map(|g| g.to_string()),
        }
    }

    #[tokio::test]
    async fn lists_immediate_children_dirs_first_then_files_lexicographic() {
        let fs = mem_fs! {
            "root": {
                "src": {
                    "lib.rs": "//\n",
                    "main.rs": "//\n",
                    "sub": {
                        "deep.rs": "//\n"
                    }
                }
            }
        };
        let root = fs.join("root").unwrap();
        let tool = FsList::new(&crate::project::ProjectRoot::try_from(root).unwrap());

        let raw = tool.call(args("src", None)).await.unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let arr = value.as_array().unwrap();
        let names: Vec<&str> = arr.iter().map(|e| e["name"].as_str().unwrap()).collect();
        assert_eq!(names, ["sub", "lib.rs", "main.rs"]);
        assert_eq!(arr[0]["kind"].as_str(), Some("dir"));
        assert_eq!(arr[1]["kind"].as_str(), Some("file"));
    }

    #[tokio::test]
    async fn glob_with_double_star_recurses_and_filters_extension() {
        let fs = mem_fs! {
            "root": {
                "src": {
                    "lib.rs": "//\n",
                    "main.rs": "//\n",
                    "notes.md": "doc\n",
                    "sub": {
                        "deep.rs": "//\n"
                    }
                }
            }
        };
        let root = fs.join("root").unwrap();
        let tool = FsList::new(&crate::project::ProjectRoot::try_from(root).unwrap());

        let raw = tool.call(args("src", Some("**/*.rs"))).await.unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let names: Vec<&str> = value
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"lib.rs"));
        assert!(names.contains(&"main.rs"));
        assert!(names.contains(&"deep.rs"));
        assert!(!names.iter().any(|n| n.ends_with(".md")));
    }

    #[tokio::test]
    async fn glob_without_double_star_filters_only_immediate_children() {
        let fs = mem_fs! {
            "root": {
                "src": {
                    "lib.rs": "//\n",
                    "main.rs": "//\n",
                    "sub": {
                        "deep.rs": "//\n"
                    }
                }
            }
        };
        let root = fs.join("root").unwrap();
        let tool = FsList::new(&crate::project::ProjectRoot::try_from(root).unwrap());

        let raw = tool.call(args("src", Some("*.rs"))).await.unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        let names: Vec<&str> = value
            .as_array()
            .unwrap()
            .iter()
            .map(|e| e["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"lib.rs"));
        assert!(names.contains(&"main.rs"));
        assert!(!names.contains(&"deep.rs"), "non-** glob does not descend");
    }

    #[tokio::test]
    async fn outside_root_is_rejected() {
        let fs = mem_fs! { "root": { "src": { "lib.rs": "//\n" } } };
        let root = fs.join("root").unwrap();
        let tool = FsList::new(&crate::project::ProjectRoot::try_from(root).unwrap());

        let err = tool.call(args("../oops", None)).await.unwrap_err();
        assert!(matches!(
            err,
            FsListError::Resolve(ResolveError::OutsideRoot { .. })
        ));
    }

    #[tokio::test]
    async fn not_a_directory_when_path_is_a_file() {
        let fs = mem_fs! { "root": { "src": { "lib.rs": "//\n" } } };
        let root = fs.join("root").unwrap();
        let tool = FsList::new(&crate::project::ProjectRoot::try_from(root).unwrap());

        let err = tool.call(args("src/lib.rs", None)).await.unwrap_err();
        assert!(matches!(err, FsListError::NotADirectory { .. }));
    }

    #[tokio::test]
    async fn invalid_glob_surfaces() {
        let fs = mem_fs! { "root": { "src": { "lib.rs": "//\n" } } };
        let root = fs.join("root").unwrap();
        let tool = FsList::new(&crate::project::ProjectRoot::try_from(root).unwrap());

        let err = tool.call(args("src", Some("[broken"))).await.unwrap_err();
        assert!(matches!(err, FsListError::InvalidGlob { .. }));
    }
}
