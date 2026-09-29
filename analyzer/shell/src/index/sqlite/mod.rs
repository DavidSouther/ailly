//! SQLite-backed [`SessionIndex`](crate::index::aggregate::SessionIndex) implementation.

mod backend;
mod schema;

use crate::index::aggregate::SessionIndex;
use crate::index::reconcile::{run_reconcile, IndexRefresh, ReconcileProgress};
use crate::index::{
    EventPage, IndexError, IndexStatus, ListSessionsQuery, PageQuery, Paged, SearchHit,
    SearchQuery, SessionListItem, SessionSummary,
};
use backend::SqliteBackend;
use schema::open_connection;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub struct SqliteSessionIndex {
    backend: Mutex<SqliteBackend>,
    path: PathBuf,
    interrupt: Arc<Mutex<bool>>,
    status: Arc<Mutex<IndexStatus>>,
}

impl SqliteSessionIndex {
    pub fn open(path: &Path) -> Result<Self, IndexError> {
        let conn = open_connection(path)?;
        Ok(Self {
            backend: Mutex::new(SqliteBackend::new(conn)),
            path: path.to_path_buf(),
            interrupt: Arc::new(Mutex::new(false)),
            status: Arc::new(Mutex::new(IndexStatus::Idle)),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl SessionIndex for SqliteSessionIndex {
    fn refresh_with_progress(
        &self,
        refresh: IndexRefresh,
        progress: &mut dyn FnMut(ReconcileProgress),
    ) -> Result<(), IndexError> {
        *self.status.lock().expect("status lock") = IndexStatus::Running;
        match run_reconcile(&self.backend, refresh, self.interrupt.clone(), progress) {
            Ok(()) => {
                *self.status.lock().expect("status lock") = IndexStatus::Idle;
                Ok(())
            }
            Err(err) => {
                *self.status.lock().expect("status lock") = IndexStatus::Error {
                    message: err.to_string(),
                };
                Err(err)
            }
        }
    }

    fn cancel(&self) {
        *self.interrupt.lock().expect("interrupt lock") = true;
    }

    fn status(&self) -> IndexStatus {
        self.status.lock().expect("status lock").clone()
    }

    fn list_sessions(
        &self,
        query: ListSessionsQuery,
    ) -> Result<Paged<SessionListItem>, IndexError> {
        self.backend
            .lock()
            .map_err(|_| IndexError::LockPoisoned)?
            .list_sessions(query)
    }

    fn get_session_summary(&self, session_id: &str) -> Result<SessionSummary, IndexError> {
        self.backend
            .lock()
            .map_err(|_| IndexError::LockPoisoned)?
            .get_session_summary(session_id)
    }

    fn get_event_page(&self, session_id: &str, query: PageQuery) -> Result<EventPage, IndexError> {
        self.backend
            .lock()
            .map_err(|_| IndexError::LockPoisoned)?
            .get_event_page(session_id, query)
    }

    fn search_index(&self, query: SearchQuery) -> Result<Paged<SearchHit>, IndexError> {
        self.backend
            .lock()
            .map_err(|_| IndexError::LockPoisoned)?
            .search_index(query)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::conformance::{build_three_harness_home, conformance_tests};
    use crate::index::ListSessionsQuery;
    use crate::loader::DiscoveryRoots;
    use crate::model::SourceValue;
    use std::fs;

    fn temp_index_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "ailly-sqlite-{}-{}-{}.sqlite",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock after epoch")
                .as_nanos()
        ))
    }

    fn open_ephemeral() -> SqliteSessionIndex {
        let path = temp_index_path("conformance");
        let _ = std::fs::remove_file(&path);
        SqliteSessionIndex::open(&path).expect("open sqlite index")
    }

    fn list_all(index: &SqliteSessionIndex) -> Vec<SessionListItem> {
        index
            .list_sessions(ListSessionsQuery {
                limit: 50,
                offset: 0,
                harness: None,
                project: None,
            })
            .expect("list sessions")
            .items
    }

    conformance_tests!(open_ephemeral, "sqlite");

    /// The stored schema version is TEXT, so reading it as an integer used to
    /// fail on every launch after the first, leaving the app unable to open its
    /// own index.
    #[test]
    fn reopening_an_existing_index_file_keeps_its_indexed_rows() {
        let path = temp_index_path("reopen");
        let _ = std::fs::remove_file(&path);
        let home = build_three_harness_home("sqlite-reopen");
        let roots = DiscoveryRoots {
            home: Some(home.clone()),
            pi_session_roots: Vec::new(),
        };

        {
            let index = SqliteSessionIndex::open(&path).expect("first open of a new index file");
            index
                .refresh(IndexRefresh { roots })
                .expect("reconcile fixtures");
            assert_eq!(list_all(&index).len(), 3, "fixtures should be indexed");
        }

        let reopened = SqliteSessionIndex::open(&path).expect("reopen an existing index file");
        assert_eq!(
            list_all(&reopened).len(),
            3,
            "reopening must reuse the stored schema version rather than rebuilding"
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn list_sessions_returns_decoded_iso_last_activity_ordered_most_recent_first() {
        let home = std::env::temp_dir().join(format!(
            "ailly-session-timestamps-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock after epoch")
                .as_nanos(),
            "home"
        ));
        let _ = fs::remove_dir_all(&home);
        let sessions = home.join(".claude/sessions");
        fs::create_dir_all(&sessions).expect("create claude sessions root");

        let older = "2026-01-02T12:00:00.000Z";
        let newer = "2026-01-02T13:00:00.000Z";
        fs::write(
            sessions.join("older.jsonl"),
            format!(
                r#"{{"type":"user","uuid":"u-older","sessionId":"session-older","cwd":"/work","timestamp":"{older}","message":{{"role":"user","content":"older"}}}}
"#
            ),
        )
        .expect("write older transcript");
        fs::write(
            sessions.join("newer.jsonl"),
            format!(
                r#"{{"type":"user","uuid":"u-newer","sessionId":"session-newer","cwd":"/work","timestamp":"{newer}","message":{{"role":"user","content":"newer"}}}}
"#
            ),
        )
        .expect("write newer transcript");

        let path = temp_index_path("session-timestamps");
        let _ = fs::remove_file(&path);
        let index = SqliteSessionIndex::open(&path).expect("open sqlite index");
        index
            .refresh(IndexRefresh {
                roots: DiscoveryRoots {
                    home: Some(home.clone()),
                    pi_session_roots: Vec::new(),
                },
            })
            .expect("index the two transcripts");

        let items = list_all(&index);
        assert_eq!(items.len(), 2, "both transcripts must be indexed");

        for item in &items {
            match &item.last_activity {
                SourceValue::Recorded(ts) => {
                    assert!(
                        !ts.starts_with('"'),
                        "last_activity must be the unquoted ISO string, got {ts:?}"
                    );
                }
                other => panic!(
                    "expected Recorded last_activity, got {other:?} for session {}",
                    item.id
                ),
            }
        }

        let SourceValue::Recorded(first_ts) = &items[0].last_activity else {
            unreachable!("asserted Recorded above");
        };
        let SourceValue::Recorded(second_ts) = &items[1].last_activity else {
            unreachable!("asserted Recorded above");
        };
        assert_eq!(first_ts, newer);
        assert_eq!(second_ts, older);

        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir_all(&home);
    }
}
