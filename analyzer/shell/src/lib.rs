pub mod loader;
pub mod model;

#[tauri::command]
fn app_ready() -> &'static str {
    "ailly-analyzer"
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::<tauri::Wry>::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![app_ready])
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
