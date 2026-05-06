use rig::completion::ToolDefinition;
use rig::tool::Tool;
use vfs::VfsPath;

use super::root::{ResolveError, resolve_under_root};
use super::walk::{EntryKind, WalkError, walk_under};

pub struct FsGrep {
    root: VfsPath,
}

impl FsGrep {
    pub const NAME: &'static str = "fs.grep";

    pub fn new(root: VfsPath) -> Self {
        Self { root }
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct FsGrepArgs {
    pub path: String,
    pub pattern: String,
    #[serde(default)]
    pub glob: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum FsGrepError {
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    #[error("path {path} does not exist")]
    NotFound { path: String },
    #[error("reading {path}")]
    Read {
        path: String,
        #[source]
        source: vfs::VfsError,
    },
    #[error("reading directory {path}")]
    ReadDir {
        path: String,
        #[source]
        source: vfs::VfsError,
    },
    #[error("file {path} is not valid UTF-8")]
    NotUtf8 { path: String },
    #[error("invalid pattern {pattern}")]
    InvalidPattern {
        pattern: String,
        #[source]
        source: regex::Error,
    },
    #[error("invalid glob {glob}")]
    InvalidGlob {
        glob: String,
        #[source]
        source: globset::Error,
    },
}

impl From<WalkError> for FsGrepError {
    fn from(err: WalkError) -> Self {
        FsGrepError::ReadDir {
            path: err.path,
            source: err.source,
        }
    }
}

#[derive(Debug, serde::Serialize)]
struct GrepMatch {
    line: usize,
    text: String,
}

#[derive(Debug, serde::Serialize)]
struct GrepResult {
    path: String,
    matches: Vec<GrepMatch>,
}

#[derive(Debug, serde::Serialize)]
struct GrepEnvelope {
    results: Vec<GrepResult>,
    total: usize,
}

impl Tool for FsGrep {
    const NAME: &'static str = "fs.grep";

    type Error = FsGrepError;
    type Args = FsGrepArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Search a file or a directory tree under the captured root \
                for lines matching a Rust regex. Returns a JSON envelope \
                with `results[*].path`, `results[*].matches[*].{line,text}`, \
                and `total`. A glob whose first segment is `**` recurses; \
                anything else searches only the immediate children of `path`."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "pattern": {"type": "string"},
                    "glob": {"type": "string"}
                },
                "required": ["path", "pattern"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let resolved = resolve_under_root(&self.root, &args.path)?;
        let pattern =
            regex::Regex::new(&args.pattern).map_err(|source| FsGrepError::InvalidPattern {
                pattern: args.pattern.clone(),
                source,
            })?;
        let metadata = match resolved.metadata() {
            Ok(m) => m,
            Err(_) => {
                return Err(FsGrepError::NotFound { path: args.path });
            }
        };

        let mut results: Vec<GrepResult> = Vec::new();
        let mut total = 0usize;

        match metadata.file_type {
            vfs::VfsFileType::File => {
                let body = read_text(&resolved, &args.path)?;
                let matches = grep_lines(&body, &pattern);
                total += matches.len();
                results.push(GrepResult {
                    path: resolved.as_str().to_string(),
                    matches,
                });
            }
            vfs::VfsFileType::Directory => {
                let recursive = args
                    .glob
                    .as_deref()
                    .map(|g| g.starts_with("**"))
                    .unwrap_or(false);
                let matcher = match args.glob.as_deref() {
                    Some(pattern) => Some(
                        globset::Glob::new(pattern)
                            .map_err(|source| FsGrepError::InvalidGlob {
                                glob: pattern.to_string(),
                                source,
                            })?
                            .compile_matcher(),
                    ),
                    None => None,
                };

                let entries = walk_under(&resolved, recursive, &args.path)?;
                let files = entries
                    .into_iter()
                    .filter(|(_, kind)| *kind == EntryKind::File)
                    .map(|(path, _)| path);

                for file in files {
                    if let Some(m) = matcher.as_ref() {
                        if !m.is_match(file.as_str()) {
                            continue;
                        }
                    }
                    let path_str = file.as_str().to_string();
                    let body = read_text(&file, &path_str)?;
                    let matches = grep_lines(&body, &pattern);
                    total += matches.len();
                    results.push(GrepResult {
                        path: path_str,
                        matches,
                    });
                }
            }
        }

        let envelope = GrepEnvelope { results, total };
        Ok(serde_json::to_string(&envelope).expect("envelope serializes"))
    }
}

fn read_text(path: &VfsPath, user_path: &str) -> Result<String, FsGrepError> {
    path.read_to_string().map_err(|source| {
        let msg = format!("{source}");
        if msg.contains("UTF-8") || msg.contains("utf-8") || msg.contains("invalid utf") {
            FsGrepError::NotUtf8 {
                path: user_path.to_string(),
            }
        } else {
            FsGrepError::Read {
                path: user_path.to_string(),
                source,
            }
        }
    })
}

fn grep_lines(body: &str, pattern: &regex::Regex) -> Vec<GrepMatch> {
    let mut matches = Vec::new();
    for (idx, line) in body.split('\n').enumerate() {
        if pattern.is_match(line) {
            matches.push(GrepMatch {
                line: idx + 1,
                text: line.to_string(),
            });
        }
    }
    matches
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem_fs;

    fn args(path: &str, pattern: &str, glob: Option<&str>) -> FsGrepArgs {
        FsGrepArgs {
            path: path.to_string(),
            pattern: pattern.to_string(),
            glob: glob.map(|g| g.to_string()),
        }
    }

    #[tokio::test]
    async fn single_file_happy_path_returns_one_result_with_one_match() {
        let fs = mem_fs! { "root": { "a.txt": "TODO: write\nbody\n" } };
        let root = fs.join("root").unwrap();
        let tool = FsGrep::new(root);

        let raw = tool.call(args("a.txt", "TODO", None)).await.unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["total"].as_u64(), Some(1));
        assert_eq!(value["results"].as_array().unwrap().len(), 1);
        let m = &value["results"][0]["matches"][0];
        assert_eq!(m["line"].as_u64(), Some(1));
        assert_eq!(m["text"].as_str(), Some("TODO: write"));
    }

    #[tokio::test]
    async fn directory_walk_with_double_star_glob_aggregates_total() {
        let fs = mem_fs! {
            "root": {
                "a.rs": "TODO\n",
                "b.rs": "TODO\nbody\nTODO again\n",
                "c.md": "TODO\n",
                "sub": { "d.rs": "TODO\n" }
            }
        };
        let root = fs.join("root").unwrap();
        let tool = FsGrep::new(root);

        let raw = tool.call(args(".", "TODO", Some("**/*.rs"))).await.unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["total"].as_u64(), Some(4));
        let results = value["results"].as_array().unwrap();
        assert!(
            results
                .iter()
                .all(|r| r["path"].as_str().unwrap().ends_with(".rs"))
        );
    }

    #[tokio::test]
    async fn directory_walk_without_double_star_does_not_descend() {
        let fs = mem_fs! {
            "root": {
                "a.rs": "TODO\n",
                "sub": { "deep.rs": "TODO\n" }
            }
        };
        let root = fs.join("root").unwrap();
        let tool = FsGrep::new(root);

        let raw = tool.call(args(".", "TODO", Some("*.rs"))).await.unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["total"].as_u64(), Some(1));
    }

    #[tokio::test]
    async fn invalid_pattern_surfaces() {
        let fs = mem_fs! { "root": { "a.txt": "x\n" } };
        let root = fs.join("root").unwrap();
        let tool = FsGrep::new(root);

        let err = tool.call(args("a.txt", "[broken", None)).await.unwrap_err();
        assert!(matches!(err, FsGrepError::InvalidPattern { .. }));
    }

    #[tokio::test]
    async fn invalid_glob_surfaces() {
        let fs = mem_fs! { "root": { "a.txt": "x\n", "sub": { "b.txt": "x\n" } } };
        let root = fs.join("root").unwrap();
        let tool = FsGrep::new(root);

        let err = tool
            .call(args(".", "x", Some("[broken")))
            .await
            .unwrap_err();
        assert!(matches!(err, FsGrepError::InvalidGlob { .. }));
    }

    #[tokio::test]
    async fn not_found_when_path_does_not_exist() {
        let fs = mem_fs! { "root": {} };
        let root = fs.join("root").unwrap();
        let tool = FsGrep::new(root);

        let err = tool.call(args("missing", "x", None)).await.unwrap_err();
        assert!(matches!(err, FsGrepError::NotFound { .. }));
    }

    #[tokio::test]
    async fn outside_root_is_rejected() {
        let fs = mem_fs! { "root": { "a.txt": "x\n" } };
        let root = fs.join("root").unwrap();
        let tool = FsGrep::new(root);

        let err = tool.call(args("../oops", "x", None)).await.unwrap_err();
        assert!(matches!(
            err,
            FsGrepError::Resolve(ResolveError::OutsideRoot { .. })
        ));
    }

    #[tokio::test]
    async fn match_text_is_the_full_source_line() {
        let fs = mem_fs! { "root": { "a.txt": "  leading TODO trailing  \n" } };
        let root = fs.join("root").unwrap();
        let tool = FsGrep::new(root);

        let raw = tool.call(args("a.txt", "TODO", None)).await.unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            value["results"][0]["matches"][0]["text"].as_str(),
            Some("  leading TODO trailing  ")
        );
    }
}
