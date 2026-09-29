pub mod index;
pub mod loader;
pub mod model;

use index::{
    EventPage, Index, IndexRefresh, IndexStatus, ListSessionsQuery, PageQuery, Paged,
    ReconcileProgress, SearchHit, SearchQuery, SessionIndex, SessionListItem, SessionSummary,
};
use loader::DiscoveryRoots;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

static INDEX: Mutex<Option<Arc<Index>>> = Mutex::new(None);

/// Event carrying `ReconcileProgress` while a refresh walks discovered sources.
const EVENT_PROGRESS: &str = "index-progress";
/// Event carrying the terminal `IndexStatus` when a refresh stops.
const EVENT_COMPLETE: &str = "index-complete";

/// Floor between progress events, so a large scan cannot flood the webview.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

fn index_handle() -> Result<Arc<Index>, String> {
    INDEX
        .lock()
        .expect("index lock")
        .clone()
        .ok_or_else(|| "index not initialized".to_string())
}

#[tauri::command]
fn app_ready() -> &'static str {
    "ailly-analyzer"
}

/// Opens the index under the platform app-data directory, creating it if needed.
///
/// The backend resolves its own cache location so the frontend needs no path
/// capability, and every failure reports the path it was working on.
#[tauri::command(async)]
fn index_init(app: tauri::AppHandle) -> Result<(), String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("no app data directory available: {err}"))?;
    std::fs::create_dir_all(&dir)
        .map_err(|err| format!("could not create {}: {err}", dir.display()))?;
    let path = dir.join("index.sqlite");
    let index = index::open_index(&path)
        .map_err(|err| format!("could not open index at {}: {err}", path.display()))?;
    *INDEX.lock().expect("index lock") = Some(Arc::new(index));
    index::start_catalog_refresh(pricing_cache_path(&dir));
    Ok(())
}

/// Where a refreshed rate table is cached: beside the index whose rows it
/// prices, under the same app-data directory the backend already owns.
fn pricing_cache_path(app_data_dir: &std::path::Path) -> std::path::PathBuf {
    app_data_dir.join("pricing").join("litellm-snapshot.json")
}

/// Starts a reconcile on a background thread and returns immediately.
///
/// Progress arrives as `index-progress` events and the terminal state as
/// `index-complete`, so the window stays interactive and the frontend can
/// re-query the growing index while the scan is still running.
#[tauri::command(async)]
fn index_refresh(app: tauri::AppHandle, roots: DiscoveryRoots) -> Result<IndexStatus, String> {
    let index = index_handle()?;
    if index.status() == IndexStatus::Running {
        return Ok(IndexStatus::Running);
    }

    // A rescan is the moment a stale catalog matters, since it is when new
    // sessions get their prices. Nothing waits on it: the refresh has its own
    // thread and its own once-a-day guard, and the reconcile below starts
    // whether it succeeds or not.
    if let Ok(dir) = app.path().app_data_dir() {
        index::start_catalog_refresh(pricing_cache_path(&dir));
    }

    std::thread::spawn(move || {
        let mut last_emit: Option<Instant> = None;
        let mut progress = |progress: ReconcileProgress| {
            let due = last_emit.is_none_or(|at| at.elapsed() >= PROGRESS_INTERVAL);
            if due || progress.indexed == progress.total {
                last_emit = Some(Instant::now());
                let _ = app.emit(EVENT_PROGRESS, progress);
            }
        };
        let status = match index.refresh_with_progress(IndexRefresh { roots }, &mut progress) {
            Ok(()) => index.status(),
            Err(err) => {
                eprintln!("ailly-analyzer: reconcile failed: {err}");
                IndexStatus::Error {
                    message: err.to_string(),
                }
            }
        };
        let _ = app.emit(EVENT_COMPLETE, status);
    });

    Ok(IndexStatus::Running)
}

#[tauri::command]
fn index_cancel() -> Result<IndexStatus, String> {
    let index = index_handle()?;
    index.cancel();
    Ok(index.status())
}

#[tauri::command]
fn index_status() -> Result<IndexStatus, String> {
    Ok(index_handle()?.status())
}

#[tauri::command(async)]
fn list_sessions(query: ListSessionsQuery) -> Result<Paged<SessionListItem>, String> {
    index_handle()?
        .list_sessions(query)
        .map_err(|err| err.to_string())
}

#[tauri::command(async)]
fn get_session_summary(session_id: String) -> Result<SessionSummary, String> {
    index_handle()?
        .get_session_summary(&session_id)
        .map_err(|err| err.to_string())
}

#[tauri::command(async)]
fn get_event_page(session_id: String, query: PageQuery) -> Result<EventPage, String> {
    index_handle()?
        .get_event_page(&session_id, query)
        .map_err(|err| err.to_string())
}

#[tauri::command(async)]
fn search_index(query: SearchQuery) -> Result<Paged<SearchHit>, String> {
    index_handle()?
        .search_index(query)
        .map_err(|err| err.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::<tauri::Wry>::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            app_ready,
            index_init,
            index_refresh,
            index_cancel,
            index_status,
            list_sessions,
            get_session_summary,
            get_event_page,
            search_index,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Ailly Analyzer");
}

#[cfg(test)]
mod tests {
    use super::app_ready;

    #[test]
    fn reports_the_application_identity() {
        assert_eq!(app_ready(), "ailly-analyzer");
    }
}
