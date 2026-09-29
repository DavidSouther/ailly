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

/// How far a reconcile has progressed, reported after each discovered file.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ReconcileProgress {
    pub indexed: usize,
    pub total: usize,
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

/// Reconciles discovered sources into `backend`, reporting progress per file.
///
/// The backend lock is taken per file rather than for the whole run, and
/// discovery and parsing happen outside it. That keeps investigator queries
/// answerable while a reconcile is still walking the remaining sources, so a
/// long scan streams results instead of blocking the caller until it finishes.
pub(crate) fn run_reconcile<B: ReconcileBackend>(
    backend: &Mutex<B>,
    refresh: IndexRefresh,
    interrupt: Arc<Mutex<bool>>,
    progress: &mut dyn FnMut(ReconcileProgress),
) -> Result<(), IndexError> {
    *interrupt.lock().expect("interrupt lock") = false;
    let discovered = discover_sessions(&refresh.roots);
    let discovered_paths: HashSet<String> = discovered
        .iter()
        .map(|(_, path)| path.to_string_lossy().into_owned())
        .collect();

    {
        let mut locked = backend.lock().map_err(|_| IndexError::LockPoisoned)?;
        for path in locked.stored_paths()? {
            if discovered_paths.contains(&path) {
                continue;
            }
            locked.remove_file(&path)?;
        }
    }

    let total = discovered.len();
    for (indexed, (harness, path)) in discovered.into_iter().enumerate() {
        if *interrupt.lock().expect("interrupt lock") {
            break;
        }
        // A source that vanished or turned unreadable mid-scan is skipped rather
        // than failing the whole reconcile; the next refresh prunes its rows.
        let Ok(identity) = file_identity(&path) else {
            continue;
        };
        let unchanged = backend
            .lock()
            .map_err(|_| IndexError::LockPoisoned)?
            .is_unchanged(&identity)?;
        if !unchanged {
            let parsed = parse_file(harness, &path);
            backend
                .lock()
                .map_err(|_| IndexError::LockPoisoned)?
                .upsert_file(harness, &identity, parsed)?;
        }
        progress(ReconcileProgress {
            indexed: indexed + 1,
            total,
        });
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
