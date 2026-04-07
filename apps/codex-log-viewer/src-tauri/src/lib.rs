mod backend;
#[cfg(feature = "desktop")]
mod commands;

pub use backend::{
    DetectCodexHomeResponse, InitializeCodexHomeResponse, SessionPreview, ViewerBackend,
};

#[cfg(feature = "desktop")]
pub fn run() {
    tauri::Builder::default()
        .manage(ViewerBackend::default())
        .invoke_handler(tauri::generate_handler![
            commands::detect_codex_home,
            commands::initialize_codex_home,
            commands::list_sessions,
            commands::load_session_preview,
            commands::load_session,
            commands::tail_session
        ])
        .run(tauri::generate_context!())
        .expect("tauri application should run");
}
