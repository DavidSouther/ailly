pub mod index;
pub mod loader;
pub mod model;

use index::{
    EventPage, Index, IndexRefresh, IndexStatus, ListSessionsQuery, PageQuery, Paged, SearchHit,
    SearchQuery, SessionIndex, SessionListItem, SessionSummary,
};
use loader::DiscoveryRoots;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

static INDEX: Mutex<Option<Arc<Index>>> = Mutex::new(None);

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

#[tauri::command]
fn index_init(app_data_dir: String) -> Result<(), String> {
    let path = PathBuf::from(app_data_dir).join("index.sqlite");
    let index = index::open_index(&path).map_err(|err| err.to_string())?;
    *INDEX.lock().expect("index lock") = Some(Arc::new(index));
    Ok(())
}

#[tauri::command]
fn index_refresh(roots: DiscoveryRoots) -> Result<IndexStatus, String> {
    let index = index_handle()?;
    index
        .refresh(IndexRefresh { roots })
        .map_err(|err| err.to_string())?;
    Ok(index.status())
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

#[tauri::command]
fn list_sessions(query: ListSessionsQuery) -> Result<Paged<SessionListItem>, String> {
    index_handle()?
        .list_sessions(query)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn get_session_summary(session_id: String) -> Result<SessionSummary, String> {
    index_handle()?
        .get_session_summary(&session_id)
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn get_event_page(session_id: String, query: PageQuery) -> Result<EventPage, String> {
    index_handle()?
        .get_event_page(&session_id, query)
        .map_err(|err| err.to_string())
}

#[tauri::command]
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
