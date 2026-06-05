//! Command-line surface for Ailly. One submodule per subcommand.

pub mod assemble;
pub mod env;
pub mod eval;
pub mod report;
pub mod run;

use crate::content::project::Project;
use crate::content::repository::RunId;

/// Return `path` relative to the project's host root as a [`RunId`].
/// Uses forward-slash separators; falls back to the raw path string when
/// the project has no host root (in-memory) or `path` is not under it.
pub(crate) fn project_relative(project: &Project, path: &std::path::Path) -> RunId {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let s = if let Some(host_root) = project.host_root() {
        canonical
            .strip_prefix(host_root)
            .ok()
            .and_then(|rel| rel.to_str())
            .map(|s| s.replace(std::path::MAIN_SEPARATOR, "/"))
            .unwrap_or_default()
    } else {
        path.to_str()
            .unwrap_or("")
            .replace(std::path::MAIN_SEPARATOR, "/")
    };
    RunId::from(s)
}
