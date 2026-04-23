mod backend;
#[cfg(feature = "desktop")]
mod commands;

pub use backend::{
    DetectCodexHomeResponse, InitializeCodexHomeResponse, SessionPreview, ViewerBackend,
};

#[cfg(feature = "desktop")]
use tauri::webview::PageLoadEvent;
#[cfg(feature = "desktop")]
use tauri_plugin_log::{Target, TargetKind};
#[cfg(feature = "desktop")]
use tauri_plugin_opener::OpenerExt;

#[cfg(feature = "desktop")]
fn viewer_builder_base() -> tauri::Builder<tauri::Wry> {
    tauri::Builder::default().manage(ViewerBackend::default())
}

#[cfg(feature = "desktop")]
fn external_navigation_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::<R>::new("external-navigation")
        .on_navigation(|webview, url| {
            let is_internal_host = matches!(
                url.host_str(),
                Some("localhost") | Some("127.0.0.1") | Some("tauri.localhost") | Some("::1")
            );

            let is_internal = url.scheme() == "tauri" || is_internal_host;

            if is_internal {
                return true;
            }

            let is_external_link = matches!(url.scheme(), "http" | "https" | "mailto" | "tel");

            if is_external_link {
                log::info!("opening external link in system browser: {}", url);
                let _ = webview.opener().open_url(url.as_str(), None::<&str>);
                return false;
            }

            true
        })
        .build()
}

#[cfg(feature = "desktop")]
fn desktop_builder() -> tauri::Builder<tauri::Wry> {
    viewer_builder_base()
        .invoke_handler(tauri::generate_handler![
            commands::detect_codex_home,
            commands::initialize_codex_home,
            commands::list_sessions,
            commands::list_indexed_sessions,
            commands::load_session_preview,
            commands::load_session_preview_by_id,
            commands::load_session,
            commands::load_session_metrics,
            commands::query_project_metrics,
            commands::tail_session
        ])
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: None }),
                    Target::new(TargetKind::Webview),
                ])
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(external_navigation_plugin())
        .on_page_load(|webview, payload| {
            if webview.label() == "main" && matches!(payload.event(), PageLoadEvent::Finished) {
                log::info!("main webview finished loading");
                let _ = webview.window().show();
            }
        })
}

#[cfg(feature = "desktop")]
pub fn run() {
    desktop_builder()
        .run(tauri::generate_context!())
        .expect("log viewer should run");
}
