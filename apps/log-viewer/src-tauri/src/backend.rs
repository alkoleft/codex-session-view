use std::path::{Path, PathBuf};
use std::sync::RwLock;

use codex_log::error::{AppError, AppResult};
use codex_log::events::projector::{categorize_event, summarize_event_full, EventSummaryCategory};
use codex_log::events::record::EventRecord;
use codex_log::session::{
    IndexedSessionCatalogPage, IndexedSessionSummary, LoadedSession, ResolvedCodexHome, SessionCatalog,
    SessionCatalogPage, SessionLoader, SessionReadContext, SessionReader, TailCursor, TailResult,
};
use codex_log::tree::validate_standalone_rollout_root;
use serde::{Deserialize, Serialize};

const DEFAULT_TEXT_LIMIT: usize = 120;
const DEFAULT_PREVIEW_LIMIT: usize = 80;
const MAX_PREVIEW_LIMIT: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DetectCodexHomeResponse {
    pub detected_home: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InitializeCodexHomeResponse {
    pub resolved_home: ResolvedCodexHome,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPreviewEvent {
    pub seq: u64,
    pub ts: String,
    pub event_type: String,
    pub raw_type: String,
    pub summary: String,
    pub category: EventSummaryCategory,
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionPreview {
    pub session_ref: String,
    pub session_id: String,
    pub event_count: usize,
    pub first_ts: Option<String>,
    pub last_ts: Option<String>,
    pub indexed_summary: Option<IndexedSessionSummary>,
    pub tail_cursor: TailCursor,
    pub recent_events: Vec<SessionPreviewEvent>,
}

#[derive(Debug, Default)]
pub struct ViewerBackend {
    resolved_home: RwLock<Option<ResolvedCodexHome>>,
}

impl ViewerBackend {
    pub fn detect_codex_home(&self) -> DetectCodexHomeResponse {
        DetectCodexHomeResponse {
            detected_home: ResolvedCodexHome::detect(),
        }
    }

    pub fn initialize_codex_home(
        &self,
        selected_home: Option<String>,
    ) -> AppResult<InitializeCodexHomeResponse> {
        let selected_home = selected_home.map(|value| PathBuf::from(value.trim()));
        let resolved_home = ResolvedCodexHome::initialize(selected_home)?;
        *self
            .resolved_home
            .write()
            .expect("viewer backend lock should not be poisoned") = Some(resolved_home.clone());
        Ok(InitializeCodexHomeResponse { resolved_home })
    }

    pub fn list_sessions(
        &self,
        limit: Option<usize>,
        cursor: Option<&str>,
        query: Option<&str>,
    ) -> AppResult<SessionCatalogPage> {
        let catalog = SessionCatalog::new(self.require_home()?);
        catalog.list_sessions(limit, cursor, query)
    }

    pub fn list_indexed_sessions(
        &self,
        limit: Option<usize>,
        cursor: Option<&str>,
        query: Option<&str>,
    ) -> AppResult<IndexedSessionCatalogPage> {
        let catalog = SessionCatalog::new(self.require_home()?);
        catalog.list_indexed_sessions(limit, cursor, query)
    }

    pub fn load_session(
        &self,
        session_ref: &str,
        text_limit: Option<usize>,
    ) -> AppResult<LoadedSession> {
        let loader = SessionLoader::new(self.require_home()?);
        loader.load_session(session_ref, text_limit.unwrap_or(DEFAULT_TEXT_LIMIT))
    }

    pub fn load_session_preview(
        &self,
        session_ref: &str,
        event_limit: Option<usize>,
    ) -> AppResult<SessionPreview> {
        let home = self.require_home()?;
        let catalog = SessionCatalog::new(home.clone());
        let path = home.resolve_session_ref(session_ref)?;
        let context = self.build_read_context(&path, session_ref, None)?;
        let read_result = SessionReader::load_all(&path, &context)?;
        let indexed_summary = catalog.find_indexed_session(&context.session_id)?;
        let limit = event_limit
            .unwrap_or(DEFAULT_PREVIEW_LIMIT)
            .clamp(1, MAX_PREVIEW_LIMIT);

        let recent_events = read_result
            .events
            .iter()
            .rev()
            .take(limit)
            .map(preview_event)
            .collect();

        Ok(SessionPreview {
            session_ref: session_ref.to_string(),
            session_id: context.session_id,
            event_count: read_result.events.len(),
            first_ts: read_result.events.first().map(|event| event.ts.clone()),
            last_ts: read_result.events.last().map(|event| event.ts.clone()),
            indexed_summary,
            tail_cursor: read_result.tail_cursor,
            recent_events,
        })
    }

    pub fn load_session_preview_by_id(
        &self,
        session_id: &str,
        event_limit: Option<usize>,
    ) -> AppResult<SessionPreview> {
        let home = self.require_home()?;
        let catalog = SessionCatalog::new(home.clone());
        let session_ref = catalog
            .resolve_session_ref_by_id(session_id)?
            .ok_or_else(|| {
                AppError::Runner(format!(
                    "session file for session_id={} was not found under CODEX_HOME/sessions",
                    session_id.trim()
                ))
            })?;

        let path = home.resolve_session_ref(&session_ref)?;
        let context = self.build_read_context(&path, &session_ref, None)?;
        let read_result = SessionReader::load_all(&path, &context)?;
        let indexed_summary = catalog.find_indexed_session(&context.session_id)?;
        let limit = event_limit
            .unwrap_or(DEFAULT_PREVIEW_LIMIT)
            .clamp(1, MAX_PREVIEW_LIMIT);

        let recent_events = read_result
            .events
            .iter()
            .rev()
            .take(limit)
            .map(preview_event)
            .collect();

        Ok(SessionPreview {
            session_ref,
            session_id: context.session_id,
            event_count: read_result.events.len(),
            first_ts: read_result.events.first().map(|event| event.ts.clone()),
            last_ts: read_result.events.last().map(|event| event.ts.clone()),
            indexed_summary,
            tail_cursor: read_result.tail_cursor,
            recent_events,
        })
    }

    pub fn tail_session(
        &self,
        session_ref: &str,
        tail_cursor: Option<&TailCursor>,
    ) -> AppResult<TailResult> {
        let home = self.require_home()?;
        let path = home.resolve_session_ref(session_ref)?;
        let context = self.build_read_context(&path, session_ref, tail_cursor)?;
        let cursor = tail_cursor
            .cloned()
            .unwrap_or_else(|| TailCursor::new(session_ref.to_string()));
        SessionReader::tail(&path, &context, &cursor)
    }

    fn require_home(&self) -> AppResult<ResolvedCodexHome> {
        self.resolved_home
            .read()
            .expect("viewer backend lock should not be poisoned")
            .clone()
            .ok_or_else(|| AppError::Runner("codex home is not initialized".to_string()))
    }

    fn build_read_context(
        &self,
        path: &Path,
        session_ref: &str,
        tail_cursor: Option<&TailCursor>,
    ) -> AppResult<SessionReadContext> {
        let session_id = validate_standalone_rollout_root(path)?;
        Ok(SessionReadContext {
            task_id: "standalone-rollout".to_string(),
            run_id: "standalone-rollout".to_string(),
            session_ref: session_ref.to_string(),
            session_id,
            default_parent_thread_id: tail_cursor
                .and_then(|cursor| cursor.resolved_parent_thread_id.clone())
                .unwrap_or_else(|| "standalone-root".to_string()),
        })
    }
}

fn preview_event(event: &EventRecord) -> SessionPreviewEvent {
    SessionPreviewEvent {
        seq: event.seq,
        ts: event.ts.clone(),
        event_type: event.event_type.clone(),
        raw_type: event.raw_type.clone(),
        summary: summarize_event_full(event),
        category: categorize_event(event),
        text: preview_text(event),
    }
}

fn preview_text(event: &EventRecord) -> Option<String> {
    ["text", "message", "summary"]
        .iter()
        .find_map(|key| event.payload.get(*key).and_then(|value| value.as_str()))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::ViewerBackend;
    use std::path::PathBuf;

    use tempfile::tempdir;

    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    fn write_rollout_file(root: &std::path::Path, session_id: &str, body: &[&str]) -> PathBuf {
        let path = root
            .join("sessions")
            .join("2026")
            .join("04")
            .join("07")
            .join(format!("rollout-2026-04-07T10-00-00-{session_id}.jsonl"));
        std::fs::create_dir_all(path.parent().expect("session dir should exist"))
            .expect("session dir should be created");

        let mut content = format!(
            "{{\"timestamp\":\"2026-04-07T10:00:00Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"{session_id}\",\"cwd\":\"/repo\"}}}}\n"
        );
        for line in body {
            content.push_str(line);
            content.push('\n');
        }
        std::fs::write(&path, content).expect("rollout should be written");
        path
    }

    fn create_codex_home() -> (tempfile::TempDir, PathBuf, String) {
        let tmp = tempdir().expect("tmpdir should exist");
        let home = tmp.path().join(".codex");
        let path = write_rollout_file(
            &home,
            "session-a",
            &[
                r#"{"timestamp":"2026-04-07T10:00:01Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"text":"hello"}]}}"#,
            ],
        );
        let session_ref = path
            .strip_prefix(home.join("sessions"))
            .expect("session path should be relative to sessions dir")
            .to_string_lossy()
            .replace('\\', "/");
        (tmp, home, session_ref)
    }

    #[test]
    fn read_commands_require_explicit_initialization() {
        let backend = ViewerBackend::default();

        let err = backend
            .list_sessions(Some(10), None, None)
            .expect_err("list should fail before initialization");
        assert!(err.to_string().contains("not initialized"));
    }

    #[test]
    fn initialize_and_load_session_from_canonical_home() {
        let (_tmp, home, session_ref) = create_codex_home();
        let backend = ViewerBackend::default();

        let initialized = backend
            .initialize_codex_home(Some(home.display().to_string()))
            .expect("home should initialize");
        assert!(initialized.resolved_home.root.is_absolute());

        let page = backend
            .list_sessions(Some(10), None, None)
            .expect("sessions should list");
        assert_eq!(page.items.len(), 1);

        let loaded = backend
            .load_session(&session_ref, Some(240))
            .expect("session should load");
        assert_eq!(loaded.session_id, "session-a");
        assert_eq!(loaded.tree.event_count, 2);

        let preview = backend
            .load_session_preview(&session_ref, Some(10))
            .expect("session preview should load");
        assert_eq!(preview.session_id, "session-a");
        assert_eq!(preview.event_count, 2);
        assert_eq!(preview.recent_events.len(), 2);
    }

    #[test]
    fn indexed_catalog_and_preview_by_id_work_without_catalog_scan_path_output() {
        let (_tmp, home, session_ref) = create_codex_home();
        std::fs::write(
            home.join("session_index.jsonl"),
            "{\"id\":\"session-a\",\"thread_name\":\"Alpha\",\"updated_at\":\"2026-04-07T10:01:00Z\"}\n",
        )
        .expect("index should be written");

        let backend = ViewerBackend::default();
        backend
            .initialize_codex_home(Some(home.display().to_string()))
            .expect("home should initialize");

        let indexed_page = backend
            .list_indexed_sessions(Some(10), None, None)
            .expect("indexed sessions should list");
        assert_eq!(indexed_page.items.len(), 1);
        assert_eq!(indexed_page.items[0].session_id, "session-a");
        assert_eq!(indexed_page.items[0].thread_name.as_deref(), Some("Alpha"));

        let preview = backend
            .load_session_preview_by_id("session-a", Some(10))
            .expect("session preview by id should load");
        assert_eq!(preview.session_id, "session-a");
        assert_eq!(preview.session_ref, session_ref);
        assert_eq!(
            preview
                .indexed_summary
                .as_ref()
                .and_then(|summary| summary.thread_name.as_deref()),
            Some("Alpha")
        );
    }

    #[test]
    fn tail_session_returns_incremental_events() {
        let (_tmp, home, session_ref) = create_codex_home();
        let backend = ViewerBackend::default();
        backend
            .initialize_codex_home(Some(home.display().to_string()))
            .expect("home should initialize");

        let loaded = backend
            .load_session(&session_ref, None)
            .expect("session should load");

        let session_path = home.join("sessions").join(&session_ref);
        let existing = std::fs::read_to_string(&session_path).expect("session should be readable");
        std::fs::write(
            &session_path,
            format!(
                "{existing}{{\"timestamp\":\"2026-04-07T10:00:02Z\",\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{{\"text\":\"tail update\"}}]}}}}\n"
            ),
        )
        .expect("tail event should be appended");

        let tailed = backend
            .tail_session(&session_ref, Some(&loaded.tail_cursor))
            .expect("tail should succeed");
        assert!(!tailed.reset);
        assert_eq!(tailed.events.len(), 1);
        assert_eq!(tailed.events[0].payload["text"], "tail update");
    }

    #[test]
    fn load_session_rejects_path_traversal() {
        let (_tmp, home, _session_ref) = create_codex_home();
        let backend = ViewerBackend::default();
        backend
            .initialize_codex_home(Some(home.display().to_string()))
            .expect("home should initialize");

        assert!(backend.load_session("../secret.txt", None).is_err());
        assert!(backend.load_session("/tmp/secret.txt", None).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn load_session_rejects_symlink_escape() {
        let (_tmp, home, _session_ref) = create_codex_home();
        let sessions_dir = home.join("sessions").join("2026").join("04").join("07");
        let outside = home
            .parent()
            .expect("tmp dir parent should exist")
            .join("outside.jsonl");
        std::fs::write(
            &outside,
            "{\"timestamp\":\"2026-04-07T10:00:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"foreign\"}}\n",
        )
        .expect("outside file should be written");
        symlink(&outside, sessions_dir.join("escape.jsonl"))
            .expect("escape symlink should be created");

        let backend = ViewerBackend::default();
        backend
            .initialize_codex_home(Some(home.display().to_string()))
            .expect("home should initialize");

        let err = backend
            .load_session("2026/04/07/escape.jsonl", None)
            .expect_err("symlink escape must be rejected");
        assert!(err.to_string().contains("outside sessions root"));
    }

    #[test]
    fn capabilities_and_permissions_are_locked_to_viewer_commands() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let permissions = std::fs::read_to_string(manifest_dir.join("permissions/default.toml"))
            .expect("default permission file should exist");
        assert!(permissions.contains("allow-detect-codex-home"));
        assert!(permissions.contains("allow-initialize-codex-home"));
        assert!(permissions.contains("allow-list-sessions"));
        assert!(permissions.contains("allow-load-session-preview"));
        assert!(permissions.contains("allow-load-session"));
        assert!(permissions.contains("allow-tail-session"));
        assert!(!permissions.contains("shell"));
        assert!(!permissions.contains("process"));

        let capabilities = std::fs::read_to_string(manifest_dir.join("capabilities/main.json"))
            .expect("main capability should exist");
        assert!(capabilities.contains("\"default\""));
        assert!(!capabilities.contains("shell:"));
        assert!(!capabilities.contains("process:"));
    }

    #[test]
    fn can_initialize_detected_codex_home_when_present() {
        let backend = ViewerBackend::default();
        let detected = backend.detect_codex_home();
        if let Some(path) = detected.detected_home {
            let initialized = backend
                .initialize_codex_home(Some(path.display().to_string()))
                .expect("detected home should initialize");
            assert!(initialized.resolved_home.sessions_dir.is_dir());
        }
    }
}
