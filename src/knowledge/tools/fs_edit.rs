use std::io::Write;

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use vfs::VfsPath;

use super::range::{LineRange, LineRangeError};
use super::root::{ResolveError, resolve_under_root};
use crate::project::ProjectRoot;

pub struct FsEdit {
    root: VfsPath,
}

impl FsEdit {
    pub const NAME: &'static str = "fs-edit";

    /// Construct an `FsEdit` over a project root.
    ///
    /// The constructor accepts only `&ProjectRoot`. Handing it a
    /// `&ConversationRoot` or `&KnowledgeRoot` is a type error caught at
    /// compile time:
    ///
    /// ```compile_fail
    /// use ailly::project::{ConversationRoot, ProjectRoot};
    /// use ailly::knowledge::tools::FsEdit;
    /// use vfs::{MemoryFS, VfsPath};
    ///
    /// let fs: VfsPath = VfsPath::new(MemoryFS::new());
    /// fs.join("conv").unwrap().create_dir().unwrap();
    /// let conv = ConversationRoot::try_from(fs.join("conv").unwrap()).unwrap();
    /// // The next line must not compile.
    /// let _ = FsEdit::new(&conv);
    /// ```
    pub fn new(project: &ProjectRoot) -> Self {
        Self {
            root: project.as_path().clone(),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
pub struct FsEditArgs {
    pub path: String,
    pub range: LineRange,
    pub replacement: String,
}

#[derive(Debug, thiserror::Error)]
pub enum FsEditError {
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
    #[error("writing {path}")]
    Write {
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

impl Tool for FsEdit {
    const NAME: &'static str = "fs-edit";

    type Error = FsEditError;
    type Args = FsEditArgs;
    type Output = String;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Replace a 1-indexed inclusive line range in a file \
                under the captured root with the supplied replacement text. \
                An empty replacement deletes the range. Line breaks in the \
                replacement create new lines. The original file's trailing \
                newline state is preserved."
                .to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string"},
                    "range": {
                        "type": "object",
                        "properties": {
                            "start": {"type": "integer", "minimum": 1},
                            "end": {"type": "integer", "minimum": 1}
                        },
                        "required": ["start", "end"]
                    },
                    "replacement": {"type": "string"}
                },
                "required": ["path", "range", "replacement"]
            }),
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let path = resolve_under_root(&self.root, &args.path)?;
        match path.metadata().ok().map(|m| m.file_type) {
            Some(vfs::VfsFileType::Directory) => {
                return Err(FsEditError::NotAFile { path: args.path });
            }
            None => return create_file(&path, &args.path, &args.replacement),
            Some(vfs::VfsFileType::File) => {}
        }
        let body = path.read_to_string().map_err(|source| FsEditError::Read {
            path: args.path.clone(),
            source,
        })?;
        let trailing_newline = body.ends_with('\n');
        let body_no_trailing = if trailing_newline {
            &body[..body.len() - 1]
        } else {
            &body[..]
        };
        let mut lines: Vec<String> = body_no_trailing.split('\n').map(String::from).collect();
        let total_lines = lines.len();
        if args.range.0.end > total_lines {
            return Err(FsEditError::OutOfRange {
                path: args.path,
                end: args.range.end_one_indexed(),
                total_lines,
            });
        }

        let replacement_lines: Vec<String> = if args.replacement.is_empty() {
            Vec::new()
        } else {
            args.replacement.split('\n').map(String::from).collect()
        };
        let n_new = replacement_lines.len();
        let start_one = args.range.start_one_indexed();
        let end_one = args.range.end_one_indexed();

        lines.splice(args.range.0.clone(), replacement_lines);

        let mut new_body = lines.join("\n");
        if trailing_newline {
            new_body.push('\n');
        }

        let mut writer = path.create_file().map_err(|source| FsEditError::Write {
            path: args.path.clone(),
            source,
        })?;
        writer
            .write_all(new_body.as_bytes())
            .map_err(|err| FsEditError::Write {
                path: args.path.clone(),
                source: vfs::VfsError::from(err),
            })?;

        Ok(format!(
            "edited {path}: replaced lines {start}-{end} with {n} lines",
            path = args.path,
            start = start_one,
            end = end_one,
            n = n_new
        ))
    }
}

fn create_file(path: &VfsPath, user_path: &str, replacement: &str) -> Result<String, FsEditError> {
    let mut writer = path.create_file().map_err(|source| FsEditError::Write {
        path: user_path.to_string(),
        source,
    })?;
    writer
        .write_all(replacement.as_bytes())
        .map_err(|err| FsEditError::Write {
            path: user_path.to_string(),
            source: vfs::VfsError::from(err),
        })?;
    let n_new = if replacement.is_empty() {
        0
    } else {
        replacement.matches('\n').count() + 1
    };
    Ok(format!(
        "created {path} with {n} lines",
        path = user_path,
        n = n_new
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem_fs;

    fn args(path: &str, range: (u32, u32), replacement: &str) -> FsEditArgs {
        let json = format!(r#"{{"start":{},"end":{}}}"#, range.0, range.1);
        FsEditArgs {
            path: path.to_string(),
            range: serde_json::from_str(&json).unwrap(),
            replacement: replacement.to_string(),
        }
    }

    fn read(root: &VfsPath, path: &str) -> String {
        root.join(path).unwrap().read_to_string().unwrap()
    }

    #[tokio::test]
    async fn rewrites_a_middle_line_and_preserves_trailing_newline() {
        let fs = mem_fs! { "root": { "a.txt": "one\ntwo\nthree\n" } };
        let root = fs.join("root").unwrap();
        let tool = FsEdit::new(&crate::project::ProjectRoot::try_from(root.clone()).unwrap());

        let confirmation = tool.call(args("a.txt", (2, 2), "TWO")).await.unwrap();
        assert!(confirmation.contains("a.txt"));
        assert!(confirmation.contains("2-2"));
        assert!(confirmation.contains("1 lines"));
        assert_eq!(read(&root, "a.txt"), "one\nTWO\nthree\n");
    }

    #[tokio::test]
    async fn rewrites_first_line() {
        let fs = mem_fs! { "root": { "a.txt": "one\ntwo\nthree\n" } };
        let root = fs.join("root").unwrap();
        let tool = FsEdit::new(&crate::project::ProjectRoot::try_from(root.clone()).unwrap());

        tool.call(args("a.txt", (1, 1), "ONE")).await.unwrap();
        assert_eq!(read(&root, "a.txt"), "ONE\ntwo\nthree\n");
    }

    #[tokio::test]
    async fn rewrites_last_line_without_changing_trailing_newline() {
        let fs = mem_fs! { "root": { "a.txt": "one\ntwo\nthree\n" } };
        let root = fs.join("root").unwrap();
        let tool = FsEdit::new(&crate::project::ProjectRoot::try_from(root.clone()).unwrap());

        tool.call(args("a.txt", (3, 3), "THREE")).await.unwrap();
        assert_eq!(read(&root, "a.txt"), "one\ntwo\nTHREE\n");
    }

    #[tokio::test]
    async fn no_trailing_newline_is_preserved_after_edit() {
        let fs = mem_fs! { "root": { "a.txt": "one\ntwo\nthree" } };
        let root = fs.join("root").unwrap();
        let tool = FsEdit::new(&crate::project::ProjectRoot::try_from(root.clone()).unwrap());

        tool.call(args("a.txt", (2, 2), "TWO")).await.unwrap();
        assert_eq!(read(&root, "a.txt"), "one\nTWO\nthree");
    }

    #[tokio::test]
    async fn multi_line_replacement_writes_two_lines() {
        let fs = mem_fs! { "root": { "a.txt": "one\ntwo\nthree\n" } };
        let root = fs.join("root").unwrap();
        let tool = FsEdit::new(&crate::project::ProjectRoot::try_from(root.clone()).unwrap());

        let confirmation = tool.call(args("a.txt", (2, 2), "a\nb")).await.unwrap();
        assert!(confirmation.contains("2 lines"));
        assert_eq!(read(&root, "a.txt"), "one\na\nb\nthree\n");
    }

    #[tokio::test]
    async fn empty_replacement_deletes_the_range() {
        let fs = mem_fs! { "root": { "a.txt": "one\ntwo\nthree\n" } };
        let root = fs.join("root").unwrap();
        let tool = FsEdit::new(&crate::project::ProjectRoot::try_from(root.clone()).unwrap());

        let confirmation = tool.call(args("a.txt", (2, 2), "")).await.unwrap();
        assert!(confirmation.contains("0 lines"));
        assert_eq!(read(&root, "a.txt"), "one\nthree\n");
    }

    #[tokio::test]
    async fn out_of_range_when_end_exceeds_total_lines() {
        let fs = mem_fs! { "root": { "a.txt": "one\ntwo\n" } };
        let root = fs.join("root").unwrap();
        let tool = FsEdit::new(&crate::project::ProjectRoot::try_from(root.clone()).unwrap());

        let err = tool.call(args("a.txt", (1, 10), "x")).await.unwrap_err();
        assert!(matches!(err, FsEditError::OutOfRange { .. }));
    }

    #[tokio::test]
    async fn not_a_file_when_path_is_a_directory() {
        let fs = mem_fs! { "root": { "sub": { "a.txt": "x\n" } } };
        let root = fs.join("root").unwrap();
        let tool = FsEdit::new(&crate::project::ProjectRoot::try_from(root.clone()).unwrap());

        let err = tool.call(args("sub", (1, 1), "x")).await.unwrap_err();
        assert!(matches!(err, FsEditError::NotAFile { .. }));
    }

    #[tokio::test]
    async fn creates_a_missing_file_with_replacement_as_body() {
        let fs = mem_fs! { "root": {} };
        let root = fs.join("root").unwrap();
        let tool = FsEdit::new(&crate::project::ProjectRoot::try_from(root.clone()).unwrap());

        let confirmation = tool
            .call(args("new.txt", (1, 1), "hello\nworld\n"))
            .await
            .unwrap();

        assert!(
            confirmation.contains("created"),
            "creation confirmation announces creation: {confirmation}"
        );
        assert!(confirmation.contains("new.txt"));
        assert_eq!(read(&root, "new.txt"), "hello\nworld\n");
    }

    #[tokio::test]
    async fn creating_ignores_the_supplied_range() {
        let fs = mem_fs! { "root": {} };
        let root = fs.join("root").unwrap();
        let tool = FsEdit::new(&crate::project::ProjectRoot::try_from(root.clone()).unwrap());

        let confirmation = tool.call(args("new.txt", (5, 10), "body")).await.unwrap();

        assert!(confirmation.contains("created"));
        assert_eq!(read(&root, "new.txt"), "body");
    }

    #[tokio::test]
    async fn empty_replacement_creates_an_empty_file() {
        let fs = mem_fs! { "root": {} };
        let root = fs.join("root").unwrap();
        let tool = FsEdit::new(&crate::project::ProjectRoot::try_from(root.clone()).unwrap());

        let confirmation = tool.call(args("blank.txt", (1, 1), "")).await.unwrap();

        assert!(confirmation.contains("created"));
        assert!(confirmation.contains("0 lines"));
        assert_eq!(read(&root, "blank.txt"), "");
    }

    #[tokio::test]
    async fn creating_does_not_make_parent_directories() {
        let fs = mem_fs! { "root": {} };
        let root = fs.join("root").unwrap();
        let tool = FsEdit::new(&crate::project::ProjectRoot::try_from(root.clone()).unwrap());

        let err = tool
            .call(args("missing/dir/new.txt", (1, 1), "body"))
            .await
            .unwrap_err();

        assert!(
            matches!(err, FsEditError::Write { .. }),
            "missing parent directory surfaces as a Write error: {err:?}"
        );
    }

    #[tokio::test]
    async fn creating_a_path_outside_root_is_rejected() {
        let fs = mem_fs! { "root": {} };
        let root = fs.join("root").unwrap();
        let tool = FsEdit::new(&crate::project::ProjectRoot::try_from(root.clone()).unwrap());

        let err = tool
            .call(args("../escape.txt", (1, 1), "body"))
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            FsEditError::Resolve(super::ResolveError::OutsideRoot { .. })
        ));
    }
}
