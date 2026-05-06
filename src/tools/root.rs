use vfs::VfsPath;

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("resolving {path}")]
    ResolvePath {
        path: String,
        #[source]
        source: vfs::VfsError,
    },
    #[error("artifact {path} outside {root}")]
    OutsideRoot { path: String, root: String },
}

pub(super) fn resolve_under_root(root: &VfsPath, path: &str) -> Result<VfsPath, ResolveError> {
    let resolved = root
        .join(path)
        .map_err(|source| ResolveError::ResolvePath {
            path: path.to_string(),
            source,
        })?;
    if !resolved.as_str().starts_with(root.as_str()) {
        return Err(ResolveError::OutsideRoot {
            path: resolved.as_str().into(),
            root: root.as_str().into(),
        });
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem_fs;

    #[test]
    fn resolves_a_relative_child() {
        let fs = mem_fs! {
            "root": {
                "sub": {
                    "file.txt": "body"
                }
            }
        };
        let root = fs.join("root").unwrap();

        let resolved = resolve_under_root(&root, "sub/file.txt").unwrap();
        assert_eq!(resolved.as_str(), "/root/sub/file.txt");
    }

    #[test]
    fn rejects_a_dotdot_escape_with_outside_root() {
        let fs = mem_fs! {
            "root": {
                "file.txt": "body"
            }
        };
        let root = fs.join("root").unwrap();

        let err = resolve_under_root(&root, "../file.txt").unwrap_err();
        match err {
            ResolveError::OutsideRoot { path, root } => {
                assert_eq!(path, "/file.txt");
                assert_eq!(root, "/root");
            }
            other => panic!("expected OutsideRoot, got {other:?}"),
        }
    }
}
