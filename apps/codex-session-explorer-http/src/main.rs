use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderValue, Response, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use codex_log::session::TailCursor;
use codex_log::session_metrics::SessionMetricsQuery;
use codex_session_explorer_backend::{
    RecomputeMetricsRequest, RecomputeMetricsResponse, ViewerBackend,
};
use include_dir::{include_dir, Dir};
use mime_guess::from_path;
use serde::Deserialize;
use serde_json::json;

static FRONTEND_DIST: Dir<'_> = include_dir!("$OUT_DIR/frontend-dist");

#[derive(Debug, Parser)]
#[command(
    name = "codex-session-explorer-http",
    about = "Локальный HTTP backend для browser-based codex-session-explorer"
)]
struct Args {
    #[arg(long, default_value_t = 4321)]
    port: u16,
    #[arg(long)]
    codex_home: Option<String>,
    #[arg(long, default_value_t = IpAddr::V4(Ipv4Addr::LOCALHOST))]
    host: IpAddr,
}

#[derive(Clone)]
struct AppState {
    backend: Arc<ViewerBackend>,
}

#[derive(Debug, Deserialize)]
struct ListIndexedSessionsRequest {
    limit: Option<usize>,
    cursor: Option<String>,
    query: Option<String>,
    exact_session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SessionRefRequest {
    session_ref: String,
    text_limit: Option<usize>,
    event_limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct SessionIdPreviewRequest {
    session_id: String,
    event_limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct TailSessionRequest {
    session_ref: String,
    tail_cursor: Option<TailCursor>,
}

#[derive(Debug, Deserialize)]
struct QueryProjectMetricsRequest {
    query: SessionMetricsQuery,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let backend = Arc::new(ViewerBackend::default());
    backend.initialize_codex_home(args.codex_home.clone())?;

    let state = AppState { backend };
    let app = build_router(state);
    let address = SocketAddr::new(args.host, args.port);

    println!("codex-session-explorer-http listening on http://{address}");

    let listener = tokio::net::TcpListener::bind(address).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route(
            "/api/viewer/list_indexed_sessions",
            post(list_indexed_sessions),
        )
        .route(
            "/api/viewer/list_project_metrics_catalog",
            post(list_project_metrics_catalog),
        )
        .route(
            "/api/viewer/load_session_preview",
            post(load_session_preview),
        )
        .route(
            "/api/viewer/load_session_preview_by_id",
            post(load_session_preview_by_id),
        )
        .route("/api/viewer/load_session", post(load_session))
        .route(
            "/api/viewer/load_session_metrics",
            post(load_session_metrics),
        )
        .route(
            "/api/viewer/query_project_metrics",
            post(query_project_metrics),
        )
        .route("/api/viewer/recompute_metrics", post(recompute_metrics))
        .route("/api/viewer/tail_session", post(tail_session))
        .route("/", get(serve_index))
        .route("/{*asset_path}", get(serve_asset))
        .with_state(state)
}

async fn list_indexed_sessions(
    State(state): State<AppState>,
    Json(request): Json<ListIndexedSessionsRequest>,
) -> Result<Json<codex_log::session::IndexedSessionCatalogPage>, ApiError> {
    state
        .backend
        .list_indexed_sessions(
            request.limit,
            request.cursor.as_deref(),
            request.query.as_deref(),
            request.exact_session_id.as_deref(),
        )
        .map(Json)
        .map_err(ApiError::from)
}

async fn list_project_metrics_catalog(
    State(state): State<AppState>,
) -> Result<Json<Vec<codex_session_explorer_backend::ProjectMetricsCatalogEntry>>, ApiError> {
    state
        .backend
        .list_project_metrics_catalog()
        .map(Json)
        .map_err(ApiError::from)
}

async fn load_session_preview(
    State(state): State<AppState>,
    Json(request): Json<SessionRefRequest>,
) -> Result<Json<codex_session_explorer_backend::SessionPreview>, ApiError> {
    state
        .backend
        .load_session_preview(&request.session_ref, request.event_limit)
        .map(Json)
        .map_err(ApiError::from)
}

async fn load_session_preview_by_id(
    State(state): State<AppState>,
    Json(request): Json<SessionIdPreviewRequest>,
) -> Result<Json<codex_session_explorer_backend::SessionPreview>, ApiError> {
    state
        .backend
        .load_session_preview_by_id(&request.session_id, request.event_limit)
        .map(Json)
        .map_err(ApiError::from)
}

async fn load_session(
    State(state): State<AppState>,
    Json(request): Json<SessionRefRequest>,
) -> Result<Json<codex_log::session::LoadedSession>, ApiError> {
    state
        .backend
        .load_session(&request.session_ref, request.text_limit)
        .map(Json)
        .map_err(ApiError::from)
}

async fn load_session_metrics(
    State(state): State<AppState>,
    Json(request): Json<SessionRefRequest>,
) -> Result<Json<codex_log::session_metrics::SessionMetrics>, ApiError> {
    state
        .backend
        .load_session_metrics(&request.session_ref, request.text_limit)
        .map(Json)
        .map_err(ApiError::from)
}

async fn query_project_metrics(
    State(state): State<AppState>,
    Json(request): Json<QueryProjectMetricsRequest>,
) -> Result<Json<codex_log::session_metrics::ProjectMetricsResponse>, ApiError> {
    state
        .backend
        .query_project_metrics(request.query)
        .map(Json)
        .map_err(ApiError::from)
}

async fn recompute_metrics(
    State(state): State<AppState>,
    Json(request): Json<RecomputeMetricsRequest>,
) -> Result<Json<RecomputeMetricsResponse>, ApiError> {
    state
        .backend
        .recompute_metrics(request)
        .map(Json)
        .map_err(ApiError::from)
}

async fn tail_session(
    State(state): State<AppState>,
    Json(request): Json<TailSessionRequest>,
) -> Result<Json<codex_log::session::TailResult>, ApiError> {
    state
        .backend
        .tail_session(&request.session_ref, request.tail_cursor.as_ref())
        .map(Json)
        .map_err(ApiError::from)
}

async fn serve_index() -> impl IntoResponse {
    respond_asset("index.html")
}

async fn serve_asset(Path(asset_path): Path<String>) -> impl IntoResponse {
    let normalized = asset_path.trim_start_matches('/');
    if normalized.starts_with("api/") {
        return StatusCode::NOT_FOUND.into_response();
    }

    if normalized.is_empty() {
        return respond_asset("index.html");
    }

    match FRONTEND_DIST.get_file(normalized) {
        Some(_) => respond_asset(normalized),
        None => respond_asset("index.html"),
    }
}

fn respond_asset(path: &str) -> Response<Body> {
    match FRONTEND_DIST.get_file(path) {
        Some(file) => {
            let mime = from_path(path).first_or_octet_stream();
            let mut response = Response::new(Body::from(file.contents().to_vec()));
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_str(mime.as_ref())
                    .expect("mime type from mime_guess should be valid"),
            );
            response
        }
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("Not found"))
            .expect("not found response should build"),
    }
}

#[derive(Debug)]
struct ApiError(String);

impl From<codex_log::error::AppError> for ApiError {
    fn from(value: codex_log::error::AppError) -> Self {
        Self(value.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (
            StatusCode::BAD_REQUEST,
            Json(json!({
                "message": self.0,
            })),
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::{build_router, AppState};
    use std::path::PathBuf;
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::{Method, Request, StatusCode};
    use codex_session_explorer_backend::ViewerBackend;
    use serde_json::json;
    use tempfile::tempdir;
    use tower::util::ServiceExt;

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

    #[tokio::test]
    async fn serves_embedded_index_at_root() {
        let backend = Arc::new(ViewerBackend::default());
        let app = build_router(AppState { backend });

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::GET)
                    .uri("/")
                    .body(Body::empty())
                    .expect("request should build"),
            )
            .await
            .expect("response should return");

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn serves_viewer_api_from_shared_backend() {
        let tmp = tempdir().expect("tmpdir should exist");
        let home = tmp.path().join(".codex");
        write_rollout_file(
            &home,
            "session-a",
            &[
                r#"{"timestamp":"2026-04-07T10:00:01Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"text":"hello"}]}}"#,
            ],
        );
        std::fs::write(
            home.join("session_index.jsonl"),
            "{\"id\":\"session-a\",\"thread_name\":\"Alpha\",\"updated_at\":\"2026-04-07T10:01:00Z\"}\n",
        )
        .expect("index should be written");

        let backend = Arc::new(ViewerBackend::default());
        backend
            .initialize_codex_home(Some(home.display().to_string()))
            .expect("home should initialize");
        let app = build_router(AppState { backend });

        let response = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/viewer/list_indexed_sessions")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        json!({
                            "limit": 10,
                            "cursor": null,
                            "query": null,
                            "exact_session_id": null
                        })
                        .to_string(),
                    ))
                    .expect("request should build"),
            )
            .await
            .expect("response should return");

        assert_eq!(response.status(), StatusCode::OK);
    }
}
