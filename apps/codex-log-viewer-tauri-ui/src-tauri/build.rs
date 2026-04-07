fn main() {
    println!("cargo:rerun-if-changed=tauri.conf.json");
    println!("cargo:rerun-if-changed=capabilities");
    println!("cargo:rerun-if-changed=permissions");
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_DESKTOP");

    if std::env::var_os("CARGO_FEATURE_DESKTOP").is_none() {
        return;
    }

    let attributes =
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(&[
            "detect_codex_home",
            "initialize_codex_home",
            "list_sessions",
            "load_session_preview",
            "tail_session",
        ]));
    tauri_build::try_build(attributes).expect("tauri build script should succeed");
}
