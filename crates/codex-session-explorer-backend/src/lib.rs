use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use codex_log::error::{AppError, AppResult};
use codex_log::events::projector::{categorize_event, summarize_event_full, EventSummaryCategory};
use codex_log::events::record::EventRecord;
use codex_log::session::{
    IndexedSessionCatalogPage, IndexedSessionSummary, LoadedSession, ResolvedCodexHome,
    SessionCatalog, SessionCatalogPage, SessionLoader, SessionReadContext, SessionReader,
    TailCursor, TailResult,
};
use codex_log::session_metrics::{
    compute_session_metrics, extract_project_identity, ProjectMetricsResponse, SessionMetrics,
    SessionMetricsQuery, SessionMetricsStore, METRICS_PROJECTION_VERSION, METRICS_SCHEMA_VERSION,
};
use codex_log::tree::{
    build_event_tree_with_standalone_startup_metadata, load_records_from_standalone_rollout,
    validate_standalone_rollout_root,
};
use serde::{Deserialize, Serialize};

const DEFAULT_TEXT_LIMIT: usize = 120;
const DEFAULT_PREVIEW_LIMIT: usize = 80;
const MAX_PREVIEW_LIMIT: usize = 200;
const PROJECT_INDEX_PAGE_LIMIT: usize = 500;

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

    pub fn resolved_home(&self) -> Option<ResolvedCodexHome> {
        self.resolved_home
            .read()
            .expect("viewer backend lock should not be poisoned")
            .clone()
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
        exact_session_id: Option<&str>,
    ) -> AppResult<IndexedSessionCatalogPage> {
        let catalog = SessionCatalog::new(self.require_home()?);
        catalog.list_indexed_sessions(limit, cursor, query, exact_session_id)
    }

    pub fn load_session(
        &self,
        session_ref: &str,
        text_limit: Option<usize>,
    ) -> AppResult<LoadedSession> {
        let loader = SessionLoader::new(self.require_home()?);
        let mut loaded =
            loader.load_session(session_ref, text_limit.unwrap_or(DEFAULT_TEXT_LIMIT))?;
        loaded.metrics = Some(self.load_session_metrics(session_ref, text_limit)?);
        Ok(loaded)
    }

    pub fn load_session_metrics(
        &self,
        session_ref: &str,
        text_limit: Option<usize>,
    ) -> AppResult<SessionMetrics> {
        let home = self.require_home()?;
        let path = home.resolve_session_ref(session_ref)?;
        let session_id = validate_standalone_rollout_root(&path)?;
        let store = SessionMetricsStore::open(&metrics_storage_path(&home))?;
        if !store.is_stale(
            &session_id,
            METRICS_SCHEMA_VERSION,
            METRICS_PROJECTION_VERSION,
        )? {
            if let Some(metrics) = store.get_session_metrics(&session_id)? {
                return Ok(metrics);
            }
        }

        let catalog = SessionCatalog::new(home.clone());
        let indexed_summary = catalog.find_indexed_session(&session_id)?;
        let standalone = load_records_from_standalone_rollout(&path, &session_id)?;
        let tree = build_event_tree_with_standalone_startup_metadata(
            &path,
            &standalone.events,
            Some(standalone.startup_metadata),
            text_limit.unwrap_or(DEFAULT_TEXT_LIMIT),
        );
        let metrics = compute_session_metrics(
            &session_id,
            &standalone.events,
            Some(&tree),
            indexed_summary.as_ref(),
        );
        store.upsert_session_metrics(&metrics)?;
        Ok(metrics)
    }

    pub fn query_project_metrics(
        &self,
        query: SessionMetricsQuery,
    ) -> AppResult<ProjectMetricsResponse> {
        let home = self.require_home()?;
        self.materialize_project_metrics(&home, &query.project_key)?;
        let store = SessionMetricsStore::open(&metrics_storage_path(&home))?;
        store.list_project_sessions(&query)
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

    fn materialize_project_metrics(
        &self,
        home: &ResolvedCodexHome,
        project_key: &str,
    ) -> AppResult<()> {
        let catalog = SessionCatalog::new(home.clone());
        let mut cursor: Option<String> = None;
        let mut session_refs = BTreeSet::new();

        loop {
            let page = catalog.list_indexed_sessions(
                Some(PROJECT_INDEX_PAGE_LIMIT),
                cursor.as_deref(),
                None,
                None,
            )?;

            for summary in page.items {
                if extract_project_identity(Some(&summary)).project_key != project_key {
                    continue;
                }

                if let Some(session_ref) = catalog.resolve_session_ref_by_id(&summary.session_id)? {
                    session_refs.insert(session_ref);
                }
            }

            match page.next_cursor {
                Some(next_cursor) => cursor = Some(next_cursor),
                None => break,
            }
        }

        for session_ref in session_refs {
            self.load_session_metrics(&session_ref, Some(DEFAULT_TEXT_LIMIT))?;
        }

        Ok(())
    }
}

fn metrics_storage_path(home: &ResolvedCodexHome) -> PathBuf {
    home.root
        .join("codex-session-explorer")
        .join("session-metrics.sqlite")
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

    use codex_log::session_metrics::{extract_project_identity, SessionMetricsQuery};
    use rusqlite::Connection;
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

    struct StateThreadRow<'a> {
        session_id: &'a str,
        rollout_path: &'a std::path::Path,
        updated_at: i64,
        cwd: &'a str,
        title: &'a str,
        first_user_message: &'a str,
        agent_nickname: Option<&'a str>,
        tokens_used: i64,
    }

    fn write_state_threads_db(home: &std::path::Path, rows: &[StateThreadRow<'_>]) {
        let path = home.join("state_5.sqlite");
        let connection = Connection::open(&path).expect("state db should open");
        connection
            .execute_batch(
                "create table threads (
                    id text primary key,
                    rollout_path text not null,
                    created_at integer not null,
                    updated_at integer not null,
                    source text not null,
                    model_provider text not null,
                    cwd text not null,
                    title text not null,
                    sandbox_policy text not null,
                    approval_mode text not null,
                    tokens_used integer not null default 0,
                    has_user_event integer not null default 0,
                    archived integer not null default 0,
                    archived_at integer,
                    git_sha text,
                    git_branch text,
                    git_origin_url text,
                    cli_version text not null default '',
                    first_user_message text not null default '',
                    agent_nickname text,
                    agent_role text,
                    memory_mode text not null default 'enabled',
                    model text,
                    reasoning_effort text,
                    agent_path text
                );",
            )
            .expect("threads table should be created");

        let mut statement = connection
            .prepare(
                "insert into threads (
                    id, rollout_path, created_at, updated_at, source, model_provider, cwd, title,
                    sandbox_policy, approval_mode, tokens_used, has_user_event, archived,
                    cli_version, first_user_message, agent_nickname, memory_mode
                ) values (?1, ?2, ?3, ?4, 'cli', 'openai', ?5, ?6, '{\"type\":\"danger-full-access\"}', 'never', ?7, 0, 0, '0.118.0', ?8, ?9, 'enabled')",
            )
            .expect("insert statement should prepare");
        for row in rows {
            statement
                .execute((
                    row.session_id,
                    row.rollout_path.display().to_string(),
                    row.updated_at - 60,
                    row.updated_at,
                    row.cwd,
                    row.title,
                    row.tokens_used,
                    row.first_user_message,
                    row.agent_nickname,
                ))
                .expect("thread row should insert");
        }
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
            .list_indexed_sessions(Some(10), None, None, None)
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
    fn query_project_metrics_materializes_missing_sessions_from_catalog() {
        let tmp = tempdir().expect("tmpdir should exist");
        let home = tmp.path().join(".codex");
        let first_path = write_rollout_file(
            &home,
            "session-a",
            &[
                r#"{"timestamp":"2026-04-07T10:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"output_tokens":5}}}}"#,
            ],
        );
        let second_path = write_rollout_file(
            &home,
            "session-b",
            &[
                r#"{"timestamp":"2026-04-07T10:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":20,"output_tokens":10}}}}"#,
            ],
        );
        write_state_threads_db(
            &home,
            &[
                StateThreadRow {
                    session_id: "session-a",
                    rollout_path: &first_path,
                    updated_at: 1_744_021_000,
                    cwd: "/repo/project-alpha",
                    title: "Alpha",
                    first_user_message: "one",
                    agent_nickname: Some("codex"),
                    tokens_used: 15,
                },
                StateThreadRow {
                    session_id: "session-b",
                    rollout_path: &second_path,
                    updated_at: 1_744_021_100,
                    cwd: "/repo/project-alpha",
                    title: "Alpha",
                    first_user_message: "two",
                    agent_nickname: Some("codex"),
                    tokens_used: 30,
                },
            ],
        );

        let backend = ViewerBackend::default();
        backend
            .initialize_codex_home(Some(home.display().to_string()))
            .expect("home should initialize");

        let indexed_page = backend
            .list_indexed_sessions(Some(10), None, None, None)
            .expect("indexed sessions should list");
        assert_eq!(indexed_page.items.len(), 2);
        let project_key = extract_project_identity(Some(&indexed_page.items[0])).project_key;

        let project = backend
            .query_project_metrics(SessionMetricsQuery {
                project_key,
                start_ts: None,
                end_ts: None,
                include_spawn_agents: true,
            })
            .expect("project metrics should load");

        assert_eq!(project.session_count, 2);
        assert_eq!(
            project.contributing_session_ids,
            vec!["session-a", "session-b"]
        );
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
