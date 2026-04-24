use codex_log::session::{
    IndexedSessionCatalogPage, LoadedSession, SessionCatalogPage, TailCursor, TailResult,
};
use codex_log::session_metrics::{ProjectMetricsResponse, SessionMetrics, SessionMetricsQuery};
use codex_session_explorer_backend::{
    DetectCodexHomeResponse, InitializeCodexHomeResponse, ProjectMetricsCatalogEntry,
    RecomputeMetricsRequest, RecomputeMetricsResponse, SessionPreview, ViewerBackend,
};
use tauri::State;

type CommandResult<T> = Result<T, String>;

#[tauri::command]
pub fn detect_codex_home(state: State<'_, ViewerBackend>) -> DetectCodexHomeResponse {
    state.detect_codex_home()
}

#[tauri::command]
pub fn initialize_codex_home(
    state: State<'_, ViewerBackend>,
    selected_home: Option<String>,
) -> CommandResult<InitializeCodexHomeResponse> {
    state
        .initialize_codex_home(selected_home)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn list_sessions(
    state: State<'_, ViewerBackend>,
    limit: Option<usize>,
    cursor: Option<String>,
    query: Option<String>,
) -> CommandResult<SessionCatalogPage> {
    state
        .list_sessions(limit, cursor.as_deref(), query.as_deref())
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn list_indexed_sessions(
    state: State<'_, ViewerBackend>,
    limit: Option<usize>,
    cursor: Option<String>,
    query: Option<String>,
    exact_session_id: Option<String>,
) -> CommandResult<IndexedSessionCatalogPage> {
    state
        .list_indexed_sessions(
            limit,
            cursor.as_deref(),
            query.as_deref(),
            exact_session_id.as_deref(),
        )
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn list_project_metrics_catalog(
    state: State<'_, ViewerBackend>,
) -> CommandResult<Vec<ProjectMetricsCatalogEntry>> {
    state
        .list_project_metrics_catalog()
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn load_session_preview(
    state: State<'_, ViewerBackend>,
    session_ref: String,
    event_limit: Option<usize>,
) -> CommandResult<SessionPreview> {
    state
        .load_session_preview(&session_ref, event_limit)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn load_session_preview_by_id(
    state: State<'_, ViewerBackend>,
    session_id: String,
    event_limit: Option<usize>,
) -> CommandResult<SessionPreview> {
    state
        .load_session_preview_by_id(&session_id, event_limit)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn load_session(
    state: State<'_, ViewerBackend>,
    session_ref: String,
    text_limit: Option<usize>,
) -> CommandResult<LoadedSession> {
    state
        .load_session(&session_ref, text_limit)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn load_session_metrics(
    state: State<'_, ViewerBackend>,
    session_ref: String,
    text_limit: Option<usize>,
) -> CommandResult<SessionMetrics> {
    state
        .load_session_metrics(&session_ref, text_limit)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn query_project_metrics(
    state: State<'_, ViewerBackend>,
    query: SessionMetricsQuery,
) -> CommandResult<ProjectMetricsResponse> {
    state
        .query_project_metrics(query)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn recompute_metrics(
    state: State<'_, ViewerBackend>,
    request: RecomputeMetricsRequest,
) -> CommandResult<RecomputeMetricsResponse> {
    state
        .recompute_metrics(request)
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub fn tail_session(
    state: State<'_, ViewerBackend>,
    session_ref: String,
    tail_cursor: Option<TailCursor>,
) -> CommandResult<TailResult> {
    state
        .tail_session(&session_ref, tail_cursor.as_ref())
        .map_err(|err| err.to_string())
}
