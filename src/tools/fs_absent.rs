use rig::completion::ToolDefinition;
use rig::tool::Tool;
use vfs::VfsPath;

use super::root::{ResolveError, resolve_under_root};

pub struct FsAbsent {
    root: VfsPath,
}

impl FsAbsent {
    pub const NAME: &'static str = "fs.absent";

    pub fn new(root: VfsPath) -> Self {
        Self { root }
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct FsAbsentArgs {
    pub path: String,
    pub needle: String,
}

#[derive(Debug, thiserror::Error)]
pub enum FsAbsentError {
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    #[error("checking existence of {path}")]
    Exists {
        path: String,
        #[source]
        source: vfs::VfsError,
    },
    #[error("artifact {path} does not exist")]
    Missing { path: String },
    #[error("reading {path}")]
    Read {
        path: String,
        #[source]
        source: vfs::VfsError,
    },
}

impl Tool for FsAbsent {
    const NAME: &'static str = "fs.absent";

    type Error = FsAbsentError;
    type Args = FsAbsentArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Read a text file under the conversation root and \
                report whether it lacks (\"cleared\") or contains \
                (\"paused\") the supplied needle. Errors when the file \
                does not exist or cannot be read."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Path to the artifact, interpreted \
                            relative to the conversation root."
                    },
                    "needle": {
                        "type": "string",
                        "description": "Substring whose presence flips the \
                            result from \"cleared\" to \"paused\"."
                    }
                },
                "required": ["path", "needle"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let path = resolve_under_root(&self.root, &args.path)?;

        let exists = path.exists().map_err(|source| FsAbsentError::Exists {
            path: args.path.clone(),
            source,
        })?;
        if !exists {
            return Err(FsAbsentError::Missing { path: args.path });
        }
        let body = path
            .read_to_string()
            .map_err(|source| FsAbsentError::Read {
                path: args.path.clone(),
                source,
            })?;
        Ok(if body.contains(&args.needle) {
            "paused".to_string()
        } else {
            "cleared".to_string()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem_fs;

    fn args(path: &str, needle: &str) -> FsAbsentArgs {
        FsAbsentArgs {
            path: path.to_string(),
            needle: needle.to_string(),
        }
    }

    #[tokio::test]
    async fn returns_cleared_when_file_lacks_needle() {
        let fs = mem_fs! {
            "root": {
                "design.md": "# Design\n\nbody\n",
            }
        };
        let root = fs.join("root").unwrap();
        let tool = FsAbsent::new(root);

        let result = tool.call(args("design.md", "*Draft")).await.unwrap();

        assert_eq!(result, "cleared");
    }

    #[tokio::test]
    async fn returns_paused_when_file_contains_needle() {
        let fs = mem_fs! {
            "root": {
                "design.md": "# Design\n\n*Draft 2026-05-04*\n\nbody\n",
            }
        };
        let root = fs.join("root").unwrap();
        let tool = FsAbsent::new(root);

        let result = tool.call(args("design.md", "*Draft")).await.unwrap();

        assert_eq!(result, "paused");
    }

    #[tokio::test]
    async fn returns_missing_error_when_file_absent() {
        let fs = mem_fs! { "root": {} };
        let root = fs.join("root").unwrap();
        let tool = FsAbsent::new(root);

        let err = tool.call(args("design.md", "*Draft")).await.unwrap_err();

        match err {
            FsAbsentError::Missing { path } => assert_eq!(path, "design.md"),
            other => panic!("expected Missing, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn path_is_resolved_relative_to_captured_root() {
        let fs = mem_fs! {
            "root": {
                "sub": {
                    "design.md": "# Design\n\nbody\n",
                },
            }
        };
        let scoped_root = fs.join("root").unwrap().join("sub").unwrap();
        let tool = FsAbsent::new(scoped_root);

        let result = tool.call(args("design.md", "*Draft")).await.unwrap();

        assert_eq!(result, "cleared");
    }

    #[tokio::test]
    async fn path_is_rejected_if_outside_root() {
        let fs = mem_fs! {
            "root": {
                "file": "contents"
            }
        };

        let scoped_root = fs.join("root").unwrap();
        let tool = FsAbsent::new(scoped_root);

        let result = tool.call(args("../design.md", "*Draft")).await;

        assert!(result.is_err());
        let Err(FsAbsentError::Resolve(ResolveError::OutsideRoot { path, root })) = result else {
            panic!("unexpected Err variant")
        };
        assert_eq!(path, String::from("/design.md"));
        assert_eq!(root, String::from("/root"));
    }
}
