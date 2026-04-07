#[cfg(feature = "desktop")]
use tauri::webview::PageLoadEvent;

#[cfg(feature = "desktop")]
pub fn run() {
    codex_log_viewer::slim_desktop_builder()
        .on_page_load(|webview, payload| {
            if webview.label() == "main" && matches!(payload.event(), PageLoadEvent::Finished) {
                let _ = webview.window().show();
            }
        })
        .run(tauri::generate_context!())
        .expect("tauri-ui viewer should run");
}
