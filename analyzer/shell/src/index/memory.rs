//! In-memory [`SessionIndex`](crate::index::aggregate::SessionIndex) for tests and fast conformance.

use crate::index::aggregate::SessionIndex;
use crate::index::domain::{
    batch_from_parsed, token_total_from_events, FileIdentity, ParsedBatch, SearchRow,
};
use crate::index::reconcile::{run_reconcile, IndexRefresh, ReconcileBackend, ReconcileProgress};
use crate::index::{
    EventPage, IndexError, IndexStatus, ListSessionsQuery, PageQuery, Paged, SearchHit,
    SearchQuery, SessionListItem, SessionSummary,
};
use crate::model::{Diagnostic, Event, Harness, ParsedSession, Relationship, Session, SourceValue};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Store {
    source_files: HashMap<String, FileIdentity>,
    sessions: HashMap<String, StoredSession>,
    events: Vec<Event>,
    relationships: Vec<Relationship>,
    diagnostics: Vec<Diagnostic>,
    search: Vec<SearchRow>,
}

#[derive(Clone)]
struct StoredSession {
    session: Session,
    source_path: String,
}

pub struct InMemorySessionIndex {
    store: Mutex<Store>,
    interrupt: Arc<Mutex<bool>>,
    status: Arc<Mutex<IndexStatus>>,
}

impl InMemorySessionIndex {
    pub fn new() -> Self {
        Self {
            store: Mutex::new(Store::default()),
            interrupt: Arc::new(Mutex::new(false)),
            status: Arc::new(Mutex::new(IndexStatus::Idle)),
        }
    }
}

impl Default for InMemorySessionIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl ReconcileBackend for Store {
    fn stored_paths(&self) -> Result<HashSet<String>, IndexError> {
        Ok(self.source_files.keys().cloned().collect())
    }

    fn is_unchanged(&self, identity: &FileIdentity) -> Result<bool, IndexError> {
        Ok(self.source_files.get(&identity.path) == Some(identity))
    }

    fn remove_file(&mut self, path: &str) -> Result<(), IndexError> {
        let removed_event_ids: HashSet<String> = self
            .events
            .iter()
            .filter(|event| event.source.path == path)
            .map(|event| event.id.clone())
            .collect();
        self.source_files.remove(path);
        self.sessions.retain(|_, stored| stored.source_path != path);
        self.events.retain(|event| event.source.path != path);
        self.relationships
            .retain(|relationship| relationship.source.path != path);
        self.diagnostics
            .retain(|diagnostic| diagnostic.source.path != path);
        self.search
            .retain(|row| !removed_event_ids.contains(&row.event_id));
        Ok(())
    }

    fn upsert_file(
        &mut self,
        harness: Harness,
        identity: &FileIdentity,
        parsed: ParsedSession,
    ) -> Result<(), IndexError> {
        self.remove_file(&identity.path)?;
        apply_batch(
            self,
            harness,
            &identity.path,
            batch_from_parsed(harness, &identity.path, parsed),
        )?;
        self.source_files
            .insert(identity.path.clone(), identity.clone());
        Ok(())
    }
}

fn apply_batch(
    store: &mut Store,
    _harness: Harness,
    source_path: &str,
    batch: ParsedBatch,
) -> Result<(), IndexError> {
    for (id, session) in batch.sessions {
        store.sessions.insert(
            id,
            StoredSession {
                session,
                source_path: source_path.to_string(),
            },
        );
    }
    store.events.extend(batch.events);
    store.relationships.extend(batch.relationships);
    store.diagnostics.extend(batch.diagnostics);
    store.search.extend(batch.search);
    Ok(())
}

impl SessionIndex for InMemorySessionIndex {
    fn refresh_with_progress(
        &self,
        refresh: IndexRefresh,
        progress: &mut dyn FnMut(ReconcileProgress),
    ) -> Result<(), IndexError> {
        *self.status.lock().expect("status lock") = IndexStatus::Running;
        match run_reconcile(&self.store, refresh, self.interrupt.clone(), progress) {
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
        let store = self.store.lock().map_err(|_| IndexError::LockPoisoned)?;
        let mut items: Vec<SessionListItem> = store
            .sessions
            .values()
            .filter(|stored| {
                query
                    .harness
                    .is_none_or(|harness| stored.session.harness == harness)
            })
            .filter(|stored| {
                query.project.as_ref().is_none_or(|project| {
                    matches!(
                        &stored.session.project,
                        SourceValue::Recorded(value) if value == project
                    )
                })
            })
            .map(|stored| {
                let event_count = store
                    .events
                    .iter()
                    .filter(|event| event.session_id == stored.session.id)
                    .count();
                let last_activity = match latest_timestamp(&store.events, &stored.session.id) {
                    Some(ts) => SourceValue::Recorded(ts),
                    None => SourceValue::Absent,
                };
                SessionListItem {
                    id: stored.session.id.clone(),
                    harness: stored.session.harness,
                    project: stored.session.project.clone(),
                    event_count,
                    token_total: SourceValue::Absent,
                    last_activity,
                }
            })
            .collect();

        items.sort_by(|left, right| {
            let left_ts = latest_timestamp(&store.events, &left.id);
            let right_ts = latest_timestamp(&store.events, &right.id);
            match (left_ts.is_none(), right_ts.is_none()) {
                (false, true) => std::cmp::Ordering::Less,
                (true, false) => std::cmp::Ordering::Greater,
                _ => right_ts.cmp(&left_ts).then_with(|| left.id.cmp(&right.id)),
            }
        });

        let offset = query.offset.min(items.len());
        let end = offset.saturating_add(query.limit).min(items.len());
        Ok(Paged {
            items: items[offset..end].to_vec(),
        })
    }

    fn get_session_summary(&self, session_id: &str) -> Result<SessionSummary, IndexError> {
        let store = self.store.lock().map_err(|_| IndexError::LockPoisoned)?;
        let events: Vec<Event> = store
            .events
            .iter()
            .filter(|event| event.session_id == session_id)
            .cloned()
            .collect();
        let (token_total, token_recorded_count) = token_total_from_events(&events);
        Ok(SessionSummary {
            token_total,
            token_recorded_count,
            event_count: events.len(),
        })
    }

    fn get_event_page(&self, session_id: &str, query: PageQuery) -> Result<EventPage, IndexError> {
        let store = self.store.lock().map_err(|_| IndexError::LockPoisoned)?;
        let mut events: Vec<Event> = store
            .events
            .iter()
            .filter(|event| event.session_id == session_id)
            .cloned()
            .collect();
        events.sort_by_key(|event| event.source.ordinal);
        let offset = query.offset.min(events.len());
        let end = offset.saturating_add(query.limit).min(events.len());
        Ok(EventPage {
            events: events[offset..end].to_vec(),
        })
    }

    fn search_index(&self, query: SearchQuery) -> Result<Paged<SearchHit>, IndexError> {
        let store = self.store.lock().map_err(|_| IndexError::LockPoisoned)?;
        let needle = query.query.to_ascii_lowercase();
        let mut hits: Vec<SearchHit> = store
            .search
            .iter()
            .filter(|row| row.text.to_ascii_lowercase().contains(&needle))
            .map(|row| SearchHit {
                session_id: row.session_id.clone(),
                event_id: row.event_id.clone(),
                snippet: snippet_around(&row.text, &query.query),
            })
            .collect();
        let offset = query.offset.min(hits.len());
        let end = offset.saturating_add(query.limit).min(hits.len());
        hits = hits[offset..end].to_vec();
        Ok(Paged { items: hits })
    }
}

fn latest_timestamp(events: &[Event], session_id: &str) -> Option<String> {
    events
        .iter()
        .filter(|event| event.session_id == session_id)
        .filter_map(|event| match &event.timestamp {
            SourceValue::Recorded(timestamp) => Some(timestamp.clone()),
            _ => None,
        })
        .max()
}

fn snippet_around(text: &str, needle: &str) -> String {
    let lower_text = text.to_ascii_lowercase();
    let lower_needle = needle.to_ascii_lowercase();
    let Some(index) = lower_text.find(&lower_needle) else {
        return text.chars().take(40).collect();
    };
    let start = index.saturating_sub(10);
    let end = (index + needle.len() + 10).min(text.len());
    format!("…{}…", &text[start..end])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::conformance::conformance_tests;

    conformance_tests!(InMemorySessionIndex::new, "memory");
}
