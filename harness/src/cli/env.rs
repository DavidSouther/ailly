//! Project-scoped `.env` loader for inference CLI handlers.
//!
//! The function in this module is invoked once per `ailly run` and once per
//! `ailly eval` invocation, immediately after
//! [`crate::content::project::Project::open`] resolves the project root. It is
//! not invoked from `ailly assemble`, from
//! [`crate::cli::run::run_with_engine`], or from any test that constructs an
//! engine directly.
//!
//! Invariants:
//! - Loading is opportunistic: a missing `.env` is a silent no-op.
//! - Existing process env vars win over `.env` entries. The user's exported
//!   shell var is authoritative; the file is the fallback.
//! - The loader never returns an error. Malformed `.env` produces a
//!   `tracing::warn!` and the run continues; the engine factory will still
//!   surface `EngineError::Auth` if the key it needs is absent.

use std::path::Path;

/// Load `<project_root>/.env` into the process environment without overriding
/// values already exported in the parent shell. Missing file is a silent
/// no-op. Malformed file emits a `tracing::warn!`. Never fails — env loading
/// is a convenience, not a contract.
pub fn load_project_env(project_root: &Path) {
    let env_path = project_root.join(".env");
    match dotenvy::from_path(&env_path) {
        Ok(()) => tracing::debug!(path = %env_path.display(), "loaded .env"),
        Err(dotenvy::Error::Io(err)) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => {
            tracing::warn!(path = %env_path.display(), %err, "ignoring malformed .env");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::Mutex;

    use tempfile::TempDir;

    use super::load_project_env;

    /// Process env is global. Tests in this module mutate it and therefore
    /// must not run in parallel with each other.
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    fn clear_test_vars() {
        // SAFETY: tests hold `ENV_MUTEX`, so no other thread observes
        // these vars concurrently.
        unsafe {
            std::env::remove_var("AILLY_DOTENV_TEST_KEY");
            std::env::remove_var("AILLY_DOTENV_TEST_NEW");
            std::env::remove_var("AILLY_DOTENV_TEST_CANARY");
        }
    }

    #[test]
    fn load_project_env_missing_file_is_silent() {
        let _guard = ENV_MUTEX.lock().expect("env mutex poisoned");
        clear_test_vars();
        let dir = TempDir::new().expect("tempdir");
        // SAFETY: serialized by ENV_MUTEX.
        unsafe { std::env::set_var("AILLY_DOTENV_TEST_CANARY", "preserved") };

        load_project_env(dir.path());

        let canary = std::env::var("AILLY_DOTENV_TEST_CANARY").ok();
        clear_test_vars();
        assert_eq!(canary.as_deref(), Some("preserved"));
    }

    #[test]
    fn load_project_env_malformed_file_warns_and_continues() {
        let _guard = ENV_MUTEX.lock().expect("env mutex poisoned");
        clear_test_vars();
        let dir = TempDir::new().expect("tempdir");
        fs::write(dir.path().join(".env"), "not=valid=syntax\n garbage")
            .expect("write malformed .env");

        load_project_env(dir.path());

        let leaked = std::env::vars().any(|(k, _)| k.starts_with("AILLY_DOTENV_TEST_"));
        clear_test_vars();
        assert!(!leaked, "no AILLY_DOTENV_TEST_* var should be set");
    }

    #[test]
    fn load_project_env_existing_var_wins_over_file() {
        let _guard = ENV_MUTEX.lock().expect("env mutex poisoned");
        clear_test_vars();
        let dir = TempDir::new().expect("tempdir");
        fs::write(
            dir.path().join(".env"),
            "AILLY_DOTENV_TEST_KEY=file\nAILLY_DOTENV_TEST_NEW=file\n",
        )
        .expect("write .env");
        // SAFETY: serialized by ENV_MUTEX.
        unsafe { std::env::set_var("AILLY_DOTENV_TEST_KEY", "shell") };

        load_project_env(dir.path());

        let existing = std::env::var("AILLY_DOTENV_TEST_KEY").ok();
        let added = std::env::var("AILLY_DOTENV_TEST_NEW").ok();
        clear_test_vars();
        assert_eq!(existing.as_deref(), Some("shell"));
        assert_eq!(added.as_deref(), Some("file"));
    }
}
