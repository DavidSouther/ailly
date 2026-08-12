//! Shared incremental reconcile algorithm for every storage backend.

use crate::index::domain::FileIdentity;
use crate::index::IndexError;
use crate::loader::{discover_sessions, parse_claude, parse_codex, parse_pi, DiscoveryRoots};
use crate::model::{Harness, ParsedSession};
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

pub struct IndexRefresh {
    pub roots: DiscoveryRoots,
}

/// Per-file persistence seam invoked by the shared reconcile loop.
pub(crate) trait ReconcileBackend {
    fn stored_paths(&self) -> Result<HashSet<String>, IndexError>;
    fn is_unchanged(&self, identity: &FileIdentity) -> Result<bool, IndexError>;
    fn remove_file(&mut self, path: &str) -> Result<(), IndexError>;
    fn upsert_file(
        &mut self,
        harness: Harness,
        identity: &FileIdentity,
        parsed: ParsedSession,
    ) -> Result<(), IndexError>;
}

pub(crate) fn run_reconcile(
    backend: &mut dyn ReconcileBackend,
    refresh: IndexRefresh,
    interrupt: Arc<Mutex<bool>>,
) -> Result<(), IndexError> {
    *interrupt.lock().expect("interrupt lock") = false;
    let discovered = discover_sessions(&refresh.roots);
    let discovered_paths: HashSet<String> = discovered
        .iter()
        .map(|(_, path)| path.to_string_lossy().into_owned())
        .collect();

    for path in backend.stored_paths()? {
        if discovered_paths.contains(&path) {
            continue;
        }
        backend.remove_file(&path)?;
    }

    for (harness, path) in discovered {
        if *interrupt.lock().expect("interrupt lock") {
            break;
        }
        let identity = file_identity(&path)?;
        if backend.is_unchanged(&identity)? {
            continue;
        }
        let parsed = parse_file(harness, &path);
        backend.upsert_file(harness, &identity, parsed)?;
    }

    Ok(())
}

pub(crate) fn file_identity(path: &Path) -> Result<FileIdentity, IndexError> {
    let metadata = fs::metadata(path)?;
    let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
    let duration = modified
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    Ok(FileIdentity {
        path: path.to_string_lossy().into_owned(),
        mtime_secs: duration.as_secs() as i64,
        mtime_nanos: duration.subsec_nanos(),
        size: metadata.len() as i64,
    })
}

fn parse_file(harness: Harness, path: &Path) -> ParsedSession {
    match harness {
        Harness::ClaudeCode => parse_claude(path),
        Harness::Codex => parse_codex(path),
        Harness::Pi => parse_pi(path),
    }
}
