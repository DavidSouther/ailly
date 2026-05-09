use rig::completion::ToolDefinition;
use rig::tool::Tool;
use vfs::VfsPath;

use super::range::{LineRange, LineRangeError};
use super::root::{ResolveError, resolve_under_root};

pub struct FsRead {
    root: VfsPath,
}

impl FsRead {
    pub const NAME: &'static str = "fs-read";

    pub fn new(project: &crate::project::ProjectRoot) -> Self {
        Self {
            root: project.as_path().clone(),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct FsReadArgs {
    pub path: String,
    #[serde(default)]
    pub range: Option<LineRange>,
}

#[derive(Debug, thiserror::Error)]
pub enum FsReadError {
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    #[error("path {path} is not a file")]
    NotAFile { path: String },
    #[error("reading {path}")]
    Read {
        path: String,
        #[source]
        source: vfs::VfsError,
    },
    #[error("file {path} is not valid UTF-8")]
    NotUtf8 { path: String },
    #[error(transparent)]
    LineRange(#[from] LineRangeError),
    #[error("range end {end} exceeds total lines {total_lines} of {path}")]
    OutOfRange {
        path: String,
        end: usize,
        total_lines: usize,
    },
}

impl Tool for FsRead {
    const NAME: &'static str = "fs-read";

    type Error = FsReadError;
    type Args = FsReadArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Read a file body under the captured root. \
                When `range` is supplied, returns the 1-indexed inclusive \
                line span joined by newlines. No line numbers are added."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "File path relative to the captured root."
                    },
                    "range": {
                        "type": "object",
                        "description": "Optional 1-indexed inclusive line span.",
                        "properties": {
                            "start": {"type": "integer", "minimum": 1},
                            "end": {"type": "integer", "minimum": 1}
                        },
                        "required": ["start", "end"]
                    }
                },
                "required": ["path"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let path = resolve_under_root(&self.root, &args.path)?;
        if !is_file(&path) {
            return Err(FsReadError::NotAFile {
                path: args.path.clone(),
            });
        }
        let body = read_text(&path, &args.path)?;
        match args.range {
            None => Ok(body),
            Some(range) => {
                let lines: Vec<&str> = body.split('\n').collect();
                let total_lines = lines.len();
                if range.0.end > total_lines {
                    return Err(FsReadError::OutOfRange {
                        path: args.path,
                        end: range.end_one_indexed(),
                        total_lines,
                    });
                }
                Ok(lines[range.0.clone()].join("\n"))
            }
        }
    }
}

fn is_file(path: &VfsPath) -> bool {
    path.metadata()
        .map(|m| matches!(m.file_type, vfs::VfsFileType::File))
        .unwrap_or(false)
}

fn read_text(path: &VfsPath, user_path: &str) -> Result<String, FsReadError> {
    path.read_to_string().map_err(|source| match source {
        vfs::VfsError { .. } if format!("{source}").contains("UTF-8") => FsReadError::NotUtf8 {
            path: user_path.to_string(),
        },
        other => FsReadError::Read {
            path: user_path.to_string(),
            source: other,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem_fs;

    fn args(path: &str, range: Option<(u32, u32)>) -> FsReadArgs {
        FsReadArgs {
            path: path.to_string(),
            range: range.map(|(s, e)| {
                let json = format!(r#"{{"start":{s},"end":{e}}}"#);
                serde_json::from_str(&json).unwrap()
            }),
        }
    }

    #[tokio::test]
    async fn returns_full_body_when_range_is_none() {
        let fs = mem_fs! { "root": { "a.txt": "one\ntwo\nthree\n" } };
        let root = fs.join("root").unwrap();
        let tool = FsRead::new(&crate::project::ProjectRoot::try_from(root).unwrap());

        let body = tool.call(args("a.txt", None)).await.unwrap();
        assert_eq!(body, "one\ntwo\nthree\n");
    }

    #[tokio::test]
    async fn returns_first_line_for_one_one_range() {
        let fs = mem_fs! { "root": { "a.txt": "one\ntwo\nthree\n" } };
        let root = fs.join("root").unwrap();
        let tool = FsRead::new(&crate::project::ProjectRoot::try_from(root).unwrap());

        let body = tool.call(args("a.txt", Some((1, 1)))).await.unwrap();
        assert_eq!(body, "one");
    }

    #[tokio::test]
    async fn returns_mid_file_range_joined_by_newlines() {
        let fs = mem_fs! { "root": { "a.txt": "one\ntwo\nthree\nfour\n" } };
        let root = fs.join("root").unwrap();
        let tool = FsRead::new(&crate::project::ProjectRoot::try_from(root).unwrap());

        let body = tool.call(args("a.txt", Some((2, 3)))).await.unwrap();
        assert_eq!(body, "two\nthree");
    }

    #[tokio::test]
    async fn out_of_range_when_end_exceeds_total_lines() {
        let fs = mem_fs! { "root": { "a.txt": "one\ntwo\n" } };
        let root = fs.join("root").unwrap();
        let tool = FsRead::new(&crate::project::ProjectRoot::try_from(root).unwrap());

        let err = tool.call(args("a.txt", Some((1, 10)))).await.unwrap_err();
        match err {
            FsReadError::OutOfRange { path, end, .. } => {
                assert_eq!(path, "a.txt");
                assert_eq!(end, 10);
            }
            other => panic!("expected OutOfRange, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn not_a_file_when_path_is_a_directory() {
        let fs = mem_fs! { "root": { "sub": { "a.txt": "x\n" } } };
        let root = fs.join("root").unwrap();
        let tool = FsRead::new(&crate::project::ProjectRoot::try_from(root).unwrap());

        let err = tool.call(args("sub", None)).await.unwrap_err();
        assert!(matches!(err, FsReadError::NotAFile { .. }));
    }

    #[tokio::test]
    async fn outside_root_is_rejected() {
        let fs = mem_fs! { "root": { "a.txt": "x\n" } };
        let root = fs.join("root").unwrap();
        let tool = FsRead::new(&crate::project::ProjectRoot::try_from(root).unwrap());

        let err = tool.call(args("../oops", None)).await.unwrap_err();
        assert!(matches!(
            err,
            FsReadError::Resolve(ResolveError::OutsideRoot { .. })
        ));
    }
}
