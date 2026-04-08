mod backend;
#[cfg(feature = "desktop")]
mod commands;

pub use backend::{
    DetectCodexHomeResponse, InitializeCodexHomeResponse, SessionPreview, ViewerBackend,
};

#[cfg(feature = "desktop")]
fn viewer_builder_base() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default().manage(ViewerBackend::default())
}

#[cfg(feature = "desktop")]
pub fn slim_desktop_builder() -> tauri::Builder<tauri::Wry> {
    viewer_builder_base().invoke_handler(tauri::generate_handler![
        commands::detect_codex_home,
        commands::initialize_codex_home,
        commands::list_sessions,
        commands::list_indexed_sessions,
        commands::load_session_preview,
        commands::load_session_preview_by_id,
        commands::load_session,
        commands::tail_session
    ])
}

#[cfg(feature = "desktop")]
pub fn desktop_builder() -> tauri::Builder<tauri::Wry> {
    viewer_builder_base().invoke_handler(tauri::generate_handler![
        commands::detect_codex_home,
        commands::initialize_codex_home,
        commands::list_sessions,
        commands::list_indexed_sessions,
        commands::load_session_preview,
        commands::load_session_preview_by_id,
        commands::load_session,
        commands::tail_session
    ])
}

#[cfg(feature = "desktop")]
pub fn run() {
    desktop_builder()
        .run(tauri::generate_context!())
        .expect("tauri application should run");
}
