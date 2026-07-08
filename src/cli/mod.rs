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

/// Requested values in `cases` that do not appear in `available`, in the
/// order they were requested. Empty means every requested value matched
/// something in `available`; also empty when `cases` itself is empty (no
/// filter requested, so nothing can be "unmatched").
pub(crate) fn unmatched_cases(cases: &[String], available: &[&str]) -> Vec<String> {
    cases
        .iter()
        .filter(|c| !available.contains(&c.as_str()))
        .cloned()
        .collect()
}

/// Validate `cases` against `available` once, at the point each command
/// already has both lists, instead of re-deriving the missing/matched check
/// per call site. `unknown_case` builds the caller's own error variant from
/// the missing and available names; each of `assemble`/`run`/`eval` keeps
/// its own `UnknownCase`-shaped variant, so this only shares the check
/// itself, not the error type.
pub(crate) fn check_cases<E>(
    cases: &[String],
    available: &[&str],
    unknown_case: impl FnOnce(Vec<String>, Vec<String>) -> E,
) -> Result<(), E> {
    let missing = unmatched_cases(cases, available);
    if missing.is_empty() {
        Ok(())
    } else {
        let available = available.iter().map(|s| (*s).to_string()).collect();
        Err(unknown_case(missing, available))
    }
}

/// Render `names` as the repeatable `--case <name>` flags a user would type
/// to request them again, so an `UnknownCase` message is copy-pastable
/// rather than a bare `Debug`-printed list (`["a", "b"]`).
pub(crate) fn format_case_flags(names: &[String]) -> String {
    names
        .iter()
        .map(|n| format!("--case {n}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::case_filter_matches;
    use super::check_cases;
    use super::format_case_flags;
    use super::unmatched_cases;

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

    #[test]
    fn unmatched_cases_is_empty_when_every_requested_value_is_available() {
        let cases = vec![String::from("a"), String::from("b")];
        assert_eq!(
            unmatched_cases(&cases, &["a", "b", "c"]),
            Vec::<String>::new()
        );
    }

    #[test]
    fn unmatched_cases_lists_only_the_misses_in_requested_order() {
        let cases = vec![String::from("a"), String::from("nope"), String::from("b")];
        assert_eq!(
            unmatched_cases(&cases, &["a", "b"]),
            vec![String::from("nope")]
        );
    }

    #[test]
    fn unmatched_cases_lists_every_value_when_all_miss() {
        let cases = vec![String::from("x"), String::from("y")];
        assert_eq!(
            unmatched_cases(&cases, &["a", "b"]),
            vec![String::from("x"), String::from("y")]
        );
    }

    #[test]
    fn unmatched_cases_is_empty_when_cases_is_empty() {
        assert_eq!(unmatched_cases(&[], &["a", "b"]), Vec::<String>::new());
    }

    #[test]
    fn check_cases_is_ok_when_every_requested_value_matches() {
        let cases = vec![String::from("a")];
        let result: Result<(), String> =
            check_cases(&cases, &["a", "b"], |requested, available| {
                format!("{requested:?} {available:?}")
            });
        assert!(result.is_ok());
    }

    #[test]
    fn check_cases_builds_the_callers_error_from_missing_and_available() {
        let cases = vec![String::from("a"), String::from("nope")];
        let err = check_cases(&cases, &["a", "b"], |requested, available| {
            (requested, available)
        })
        .expect_err("nope is not available");
        assert_eq!(err.0, vec![String::from("nope")]);
        assert_eq!(err.1, vec![String::from("a"), String::from("b")]);
    }

    #[test]
    fn format_case_flags_joins_names_as_repeatable_flags() {
        let names = vec![String::from("a"), String::from("b")];
        assert_eq!(format_case_flags(&names), "--case a --case b");
    }

    #[test]
    fn format_case_flags_is_empty_for_an_empty_list() {
        assert_eq!(format_case_flags(&[]), "");
    }
}
