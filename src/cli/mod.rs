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

/// `true` when `cases` is empty (no `--case` filter requested) or contains
/// `name` exactly. Shared by `assemble`/`run`/`eval` so all three commands
/// apply the same exact-match rule at the point where they already iterate
/// cases.
pub(crate) fn case_filter_matches(name: &str, cases: &[String]) -> bool {
    cases.is_empty() || cases.iter().any(|c| c == name)
}

#[cfg(test)]
mod tests {
    use super::case_filter_matches;

    #[test]
    fn case_filter_matches_everything_when_cases_is_empty() {
        assert!(case_filter_matches("anything", &[]));
    }

    #[test]
    fn case_filter_matches_only_exact_names_in_cases() {
        let cases = vec![String::from("newtype"), String::from("logging")];
        assert!(case_filter_matches("newtype", &cases));
        assert!(case_filter_matches("logging", &cases));
        assert!(!case_filter_matches("tracing", &cases));
    }

    #[test]
    fn case_filter_matches_is_exact_not_prefix_or_substring() {
        let cases = vec![String::from("new")];
        assert!(!case_filter_matches("newtype", &cases));
    }
}
