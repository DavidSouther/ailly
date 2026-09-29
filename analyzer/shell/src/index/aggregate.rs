//! Aggregate root contract for the rebuildable session index.
//!
//! [`SessionIndex`] is the sole entry point for reconcile mutations and
//! bounded investigator queries. Storage technology stays behind this trait.

use super::{
    EventPage, IndexError, IndexRefresh, IndexStatus, ListSessionsQuery, PageQuery, Paged,
    ReconcileProgress, SearchHit, SearchQuery, SessionListItem, SessionSummary,
};

/// Rebuildable local cache of normalized session evidence.
///
/// One reconcile touches source-file identity rows, session headers, events,
/// relationships, diagnostics, and search content atomically per source file.
pub trait SessionIndex {
    /// Reconciles sources, invoking `progress` after each discovered file.
    fn refresh_with_progress(
        &self,
        refresh: IndexRefresh,
        progress: &mut dyn FnMut(ReconcileProgress),
    ) -> Result<(), IndexError>;

    /// Reconciles sources without observing progress.
    fn refresh(&self, refresh: IndexRefresh) -> Result<(), IndexError> {
        self.refresh_with_progress(refresh, &mut |_| {})
    }

    fn cancel(&self);
    fn status(&self) -> IndexStatus;
    fn list_sessions(&self, query: ListSessionsQuery)
        -> Result<Paged<SessionListItem>, IndexError>;
    fn get_session_summary(&self, session_id: &str) -> Result<SessionSummary, IndexError>;
    fn get_event_page(&self, session_id: &str, query: PageQuery) -> Result<EventPage, IndexError>;
    fn search_index(&self, query: SearchQuery) -> Result<Paged<SearchHit>, IndexError>;
}
