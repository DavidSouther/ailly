use vfs::VfsPath;

#[derive(Debug, thiserror::Error)]
#[error("reading directory {path}")]
pub(super) struct WalkError {
    pub path: String,
    #[source]
    pub source: vfs::VfsError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EntryKind {
    File,
    Dir,
}

/// Walk children of `dir`. With `recursive = false` only the immediate
/// children are visited. With `recursive = true` every reachable
/// descendant is visited via depth-first traversal. Both files and
/// directories are returned. `user_path` is the caller-supplied path
/// string used to label `WalkError` so the failure reads in terms of
/// the input the caller saw.
pub(super) fn walk_under(
    dir: &VfsPath,
    recursive: bool,
    user_path: &str,
) -> Result<Vec<(VfsPath, EntryKind)>, WalkError> {
    let mut out = Vec::new();
    let mut stack = vec![dir.clone()];
    while let Some(current) = stack.pop() {
        let iter = current.read_dir().map_err(|source| WalkError {
            path: user_path.to_string(),
            source,
        })?;
        for child in iter {
            match child.metadata().map(|m| m.file_type) {
                Ok(vfs::VfsFileType::Directory) => {
                    out.push((child.clone(), EntryKind::Dir));
                    if recursive {
                        stack.push(child);
                    }
                }
                Ok(vfs::VfsFileType::File) => {
                    out.push((child, EntryKind::File));
                }
                _ => {}
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem_fs;

    #[test]
    fn non_recursive_returns_only_immediate_children() {
        let fs = mem_fs! {
            "root": {
                "a.txt": "x\n",
                "sub": {
                    "deep.txt": "y\n"
                }
            }
        };
        let root = fs.join("root").unwrap();

        let entries = walk_under(&root, false, "root").unwrap();

        let names: Vec<String> = entries.iter().map(|(p, _)| p.filename()).collect();
        assert!(names.contains(&"a.txt".to_string()));
        assert!(names.contains(&"sub".to_string()));
        assert!(!names.contains(&"deep.txt".to_string()));
    }

    #[test]
    fn recursive_returns_every_descendant() {
        let fs = mem_fs! {
            "root": {
                "a.txt": "x\n",
                "sub": {
                    "deep.txt": "y\n"
                }
            }
        };
        let root = fs.join("root").unwrap();

        let entries = walk_under(&root, true, "root").unwrap();

        let names: Vec<String> = entries.iter().map(|(p, _)| p.filename()).collect();
        assert!(names.contains(&"a.txt".to_string()));
        assert!(names.contains(&"sub".to_string()));
        assert!(names.contains(&"deep.txt".to_string()));
    }

    #[test]
    fn entries_carry_kind() {
        let fs = mem_fs! {
            "root": {
                "a.txt": "x\n",
                "sub": {}
            }
        };
        let root = fs.join("root").unwrap();

        let entries = walk_under(&root, false, "root").unwrap();

        let kinds: Vec<(String, EntryKind)> =
            entries.iter().map(|(p, k)| (p.filename(), *k)).collect();
        assert!(kinds.contains(&("a.txt".to_string(), EntryKind::File)));
        assert!(kinds.contains(&("sub".to_string(), EntryKind::Dir)));
    }
}
