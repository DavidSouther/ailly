//! SQLite-backed [`SessionIndex`](crate::index::aggregate::SessionIndex) implementation.

mod backend;
mod schema;

use crate::index::aggregate::SessionIndex;
use crate::index::reconcile::IndexRefresh;
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
    fn refresh(&self, refresh: IndexRefresh) -> Result<(), IndexError> {
        *self.status.lock().expect("status lock") = IndexStatus::Running;
        let mut backend = self.backend.lock().map_err(|_| IndexError::LockPoisoned)?;
        match backend.refresh(refresh, self.interrupt.clone()) {
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
    use crate::index::conformance::conformance_tests;

    fn open_ephemeral() -> SqliteSessionIndex {
        let path = std::env::temp_dir().join(format!(
            "ailly-sqlite-conformance-{}-{}.sqlite",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock after epoch")
                .as_nanos()
        ));
        let _ = std::fs::remove_file(&path);
        SqliteSessionIndex::open(&path).expect("open sqlite index")
    }

    conformance_tests!(open_ephemeral, "sqlite");
}
