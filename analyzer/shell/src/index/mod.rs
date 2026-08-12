//! Rebuildable SQLite index for normalized session evidence.

mod aggregate;
#[cfg(test)]
pub(crate) mod conformance;
mod domain;
mod memory;
mod reconcile;
mod source_value;
mod sqlite;

pub use aggregate::SessionIndex;
pub use memory::InMemorySessionIndex;
pub use reconcile::{IndexRefresh, ReconcileProgress};
pub use sqlite::SqliteSessionIndex;

use crate::model::{Event, Harness, SourceValue};
use std::path::Path;

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndexStatus {
    Idle,
    Running,
    Error { message: String },
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ListSessionsQuery {
    pub limit: usize,
    pub offset: usize,
    pub harness: Option<Harness>,
    pub project: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct PageQuery {
    pub limit: usize,
    pub offset: usize,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SearchQuery {
    pub query: String,
    pub limit: usize,
    pub offset: usize,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Paged<T> {
    pub items: Vec<T>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SessionListItem {
    pub id: String,
    pub harness: Harness,
    pub project: SourceValue<String>,
    pub event_count: usize,
    pub token_total: SourceValue<u64>,
    /// Latest recorded event timestamp for the session, else `Absent`.
    pub last_activity: SourceValue<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SessionSummary {
    pub token_total: SourceValue<u64>,
    pub token_recorded_count: usize,
    pub event_count: usize,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct EventPage {
    pub events: Vec<Event>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct SearchHit {
    pub session_id: String,
    pub event_id: String,
    pub snippet: String,
}

/// Production index handle; SQLite is the default durable backend.
pub type Index = SqliteSessionIndex;

pub fn open_index(path: &Path) -> Result<Index, IndexError> {
    SqliteSessionIndex::open(path)
}

#[derive(Debug)]
pub enum IndexError {
    Sqlite(rusqlite::Error),
    Json(serde_json::Error),
    Io(std::io::Error),
    Codec(source_value::IndexCodecError),
    LockPoisoned,
}

impl From<rusqlite::Error> for IndexError {
    fn from(value: rusqlite::Error) -> Self {
        Self::Sqlite(value)
    }
}

impl From<serde_json::Error> for IndexError {
    fn from(value: serde_json::Error) -> Self {
        Self::Json(value)
    }
}

impl From<std::io::Error> for IndexError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<source_value::IndexCodecError> for IndexError {
    fn from(value: source_value::IndexCodecError) -> Self {
        Self::Codec(value)
    }
}

impl std::fmt::Display for IndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sqlite(err) => write!(f, "sqlite error: {err}"),
            Self::Json(err) => write!(f, "json error: {err}"),
            Self::Io(err) => write!(f, "io error: {err}"),
            Self::Codec(err) => write!(f, "codec error: {err}"),
            Self::LockPoisoned => write!(f, "index lock poisoned"),
        }
    }
}

impl std::error::Error for IndexError {}
