use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, TimeZone, Utc};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::error::{AppError, AppResult};
use crate::events::readers::{
    imported_subagent_session_meta, JsonOutputEventReader, RunEventContext,
};
use crate::events::record::EventRecord;
use crate::session_metrics::SessionMetrics;
use crate::tree::{
    build_event_tree_with_standalone_startup_metadata, load_records_from_standalone_rollout,
    validate_standalone_rollout_root, EventTree,
};
use crate::util::{hash8, normalize_path, utc_now_iso};

const DEFAULT_PAGE_LIMIT: usize = 50;
const MAX_PAGE_LIMIT: usize = 500;
const RECENT_DEDUP_WINDOW: usize = 128;
const STATE_DB_PREFIX: &str = "state_";
const STATE_DB_SUFFIX: &str = ".sqlite";
const INDEXED_SESSION_CURSOR_PREFIX: &str = "indexed:v1:";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedCodexHome {
    pub root: PathBuf,
    pub sessions_dir: PathBuf,
    pub session_index_path: PathBuf,
}

impl ResolvedCodexHome {
    pub fn detect() -> Option<PathBuf> {
        let raw = std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("~/.codex"));
        let normalized = normalize_path(&raw);
        if normalized.exists() {
            Some(normalized)
        } else {
            None
        }
    }

    pub fn initialize(selected_home: Option<PathBuf>) -> AppResult<Self> {
        let raw = selected_home
            .or_else(Self::detect)
            .unwrap_or_else(|| normalize_path(Path::new("~/.codex")));
        if raw.as_os_str().is_empty() {
            return Err(AppError::EmptyPath(raw));
        }

        let root = fs::canonicalize(&raw).map_err(|err| {
            AppError::Runner(format!(
                "codex home is not accessible ({}): {err}",
                raw.display()
            ))
        })?;
        let sessions_dir = root.join("sessions");
        if !sessions_dir.is_dir() {
            return Err(AppError::Runner(format!(
                "codex home does not contain sessions directory: {}",
                sessions_dir.display()
            )));
        }

        Ok(Self {
            session_index_path: root.join("session_index.jsonl"),
            sessions_dir,
            root,
        })
    }

    pub fn resolve_session_ref(&self, session_ref: &str) -> AppResult<PathBuf> {
        if session_ref.trim().is_empty() {
            return Err(AppError::Validation {
                field: "session_ref",
                reason: "must not be empty".to_string(),
            });
        }

        let relative = Path::new(session_ref);
        if relative.is_absolute() {
            return Err(AppError::Runner(format!(
                "session_ref must be relative to CODEX_HOME/sessions: {session_ref}"
            )));
        }

        let mut sanitized = PathBuf::new();
        for component in relative.components() {
            match component {
                Component::Normal(value) => sanitized.push(value),
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(AppError::Runner(format!(
                        "session_ref escapes sessions root: {session_ref}"
                    )));
                }
            }
        }

        let candidate = self.sessions_dir.join(&sanitized);
        let canonical = fs::canonicalize(&candidate).map_err(|err| {
            AppError::Runner(format!(
                "session_ref cannot be resolved ({session_ref}): {err}"
            ))
        })?;
        if !canonical.starts_with(&self.sessions_dir) {
            return Err(AppError::Runner(format!(
                "session_ref resolved outside sessions root: {session_ref}"
            )));
        }
        if !canonical.is_file() {
            return Err(AppError::Runner(format!(
                "session_ref does not point to a file: {session_ref}"
            )));
        }
        Ok(canonical)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSummary {
    pub session_id: String,
    pub session_ref: String,
    pub updated_at: String,
    pub thread_name: Option<String>,
    pub cwd: Option<String>,
    pub is_active_like: bool,
    pub index_status: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionDiagnostic {
    pub kind: String,
    pub message: String,
    pub session_id: Option<String>,
    pub session_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionCatalogPage {
    pub items: Vec<SessionSummary>,
    pub next_cursor: Option<String>,
    pub diagnostics: Vec<SessionDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexedSessionSummary {
    pub session_id: String,
    pub updated_at: String,
    pub thread_name: Option<String>,
    pub thread_name_source: Option<String>,
    pub cwd: Option<String>,
    pub agent_name: Option<String>,
    pub tokens_used: Option<u64>,
    pub created_at: Option<String>,
    pub source: Option<String>,
    pub model_provider: Option<String>,
    pub sandbox_policy_kind: Option<String>,
    pub approval_mode: Option<String>,
    pub has_user_event: Option<bool>,
    pub archived: Option<bool>,
    pub archived_at: Option<String>,
    pub git_sha: Option<String>,
    pub git_branch: Option<String>,
    pub git_origin_url: Option<String>,
    pub cli_version: Option<String>,
    pub agent_role: Option<String>,
    pub memory_mode: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub agent_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexedSessionCatalogPage {
    pub items: Vec<IndexedSessionSummary>,
    pub next_cursor: Option<String>,
    pub diagnostics: Vec<SessionDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IndexedSessionCatalogCursor {
    offset: usize,
    exact_fallback_consumed: bool,
}

impl IndexedSessionCatalogCursor {
    fn parse(cursor: Option<&str>) -> AppResult<Self> {
        let Some(value) = cursor.map(str::trim).filter(|value| !value.is_empty()) else {
            return Ok(Self {
                offset: 0,
                exact_fallback_consumed: false,
            });
        };

        if let Some(raw) = value.strip_prefix(INDEXED_SESSION_CURSOR_PREFIX) {
            let Some((offset_raw, consumed_raw)) = raw.split_once(':') else {
                return Err(invalid_indexed_session_cursor());
            };
            let offset = offset_raw
                .parse::<usize>()
                .map_err(|_| invalid_indexed_session_cursor())?;
            let exact_fallback_consumed = match consumed_raw {
                "0" => false,
                "1" => true,
                _ => return Err(invalid_indexed_session_cursor()),
            };
            return Ok(Self {
                offset,
                exact_fallback_consumed,
            });
        }

        let offset = value
            .parse::<usize>()
            .map_err(|_| invalid_indexed_session_cursor())?;
        Ok(Self {
            offset,
            exact_fallback_consumed: false,
        })
    }

    fn allows_exact_fallback(&self) -> bool {
        self.offset == 0 && !self.exact_fallback_consumed
    }

    fn encode(&self) -> String {
        format!(
            "{INDEXED_SESSION_CURSOR_PREFIX}{}:{}",
            self.offset,
            usize::from(self.exact_fallback_consumed)
        )
    }
}

fn invalid_indexed_session_cursor() -> AppError {
    AppError::Validation {
        field: "cursor",
        reason: "must be a numeric offset or opaque indexed session cursor token".to_string(),
    }
}

#[derive(Debug, Clone)]
pub struct SessionCatalog {
    home: ResolvedCodexHome,
}

impl SessionCatalog {
    pub fn new(home: ResolvedCodexHome) -> Self {
        Self { home }
    }

    pub fn resolved_home(&self) -> &ResolvedCodexHome {
        &self.home
    }

    pub fn list_sessions(
        &self,
        limit: Option<usize>,
        cursor: Option<&str>,
        query: Option<&str>,
    ) -> AppResult<SessionCatalogPage> {
        let limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT).clamp(1, MAX_PAGE_LIMIT);
        let offset = cursor
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| {
                value.parse::<usize>().map_err(|_| AppError::Validation {
                    field: "cursor",
                    reason: "must be a numeric offset".to_string(),
                })
            })
            .transpose()?
            .unwrap_or(0);

        let query = query
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_lowercase);

        let mut diagnostics = Vec::new();
        let index_map = self.load_index_overlay(&mut diagnostics)?;
        let mut files = self.discover_session_files(&mut diagnostics)?;
        files.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.session_ref.cmp(&right.session_ref))
        });

        let mut grouped: BTreeMap<String, Vec<SessionFileEntry>> = BTreeMap::new();
        for file in files {
            grouped
                .entry(file.session_id.clone())
                .or_default()
                .push(file);
        }

        let mut sessions = Vec::new();
        for (session_id, mut entries) in grouped {
            entries.sort_by(|left, right| {
                right
                    .modified_at
                    .cmp(&left.modified_at)
                    .then_with(|| left.session_ref.cmp(&right.session_ref))
            });
            if entries.len() > 1 {
                diagnostics.push(SessionDiagnostic {
                    kind: "duplicate_session_files".to_string(),
                    message: format!(
                        "multiple session files found for session_id={session_id}; newest mtime wins"
                    ),
                    session_id: Some(session_id.clone()),
                    session_refs: entries.iter().map(|item| item.session_ref.clone()).collect(),
                });
            }
            let primary = entries
                .into_iter()
                .next()
                .expect("grouped session entries must be non-empty");
            let overlay = index_map.get(&session_id);
            let thread_name = overlay.and_then(|entry| entry.thread_name.clone());
            let updated_at = overlay
                .map(|entry| entry.updated_at.clone())
                .unwrap_or_else(|| primary.updated_at.clone());

            let summary = SessionSummary {
                session_id: session_id.clone(),
                session_ref: primary.session_ref,
                updated_at,
                thread_name,
                cwd: primary.cwd,
                is_active_like: is_active_like(primary.modified_at),
                index_status: if overlay.is_some() {
                    "indexed".to_string()
                } else {
                    "missing".to_string()
                },
            };
            if matches_query(&summary, query.as_deref()) {
                sessions.push(summary);
            }
        }

        sessions.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.session_ref.cmp(&right.session_ref))
        });
        let next_cursor = if offset + limit < sessions.len() {
            Some((offset + limit).to_string())
        } else {
            None
        };
        let items = sessions.into_iter().skip(offset).take(limit).collect();

        Ok(SessionCatalogPage {
            items,
            next_cursor,
            diagnostics,
        })
    }

    pub fn list_indexed_sessions(
        &self,
        limit: Option<usize>,
        cursor: Option<&str>,
        query: Option<&str>,
        exact_session_id: Option<&str>,
    ) -> AppResult<IndexedSessionCatalogPage> {
        let limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT).clamp(1, MAX_PAGE_LIMIT);
        let cursor = IndexedSessionCatalogCursor::parse(cursor)?;
        let offset = cursor.offset;

        let exact_session_id = exact_session_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        let query = query
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_lowercase);

        let mut diagnostics = Vec::new();
        let mut index_overlay = None;
        let mut sessions =
            if let Some(state_sessions) = self.load_state_indexed_sessions(&mut diagnostics)? {
                state_sessions
            } else {
                let overlay = self.load_index_overlay(&mut diagnostics)?;
                let sessions = overlay
                    .iter()
                    .map(|(session_id, entry)| {
                        indexed_session_summary_from_index_entry(&session_id, &entry)
                    })
                    .collect::<Vec<_>>();
                index_overlay = Some(overlay);
                sessions
            };

        sessions.retain(|summary| matches_index_query(summary, query.as_deref()));

        sessions.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| left.session_id.cmp(&right.session_id))
        });

        let exact_fallback = if cursor.allows_exact_fallback() {
            exact_session_id
                .as_deref()
                .filter(|session_id| {
                    !sessions
                        .iter()
                        .any(|summary| summary.session_id == *session_id)
                })
                .map(|session_id| {
                    self.find_file_backed_indexed_session(
                        session_id,
                        &mut diagnostics,
                        &mut index_overlay,
                    )
                })
                .transpose()?
                .flatten()
                .and_then(|summary| {
                    matches_index_query(&summary, query.as_deref()).then_some(summary)
                })
        } else {
            None
        };

        let regular_limit = limit.saturating_sub(usize::from(exact_fallback.is_some()));
        let session_count = sessions.len();
        let regular_items = sessions
            .into_iter()
            .skip(offset)
            .take(regular_limit)
            .collect::<Vec<_>>();
        let consumed_regular = regular_items.len();
        let exact_fallback_injected = exact_fallback.is_some();
        let next_offset = offset + consumed_regular;
        let next_cursor = if next_offset < session_count {
            Some(
                IndexedSessionCatalogCursor {
                    offset: next_offset,
                    exact_fallback_consumed: cursor.exact_fallback_consumed
                        || exact_fallback_injected,
                }
                .encode(),
            )
        } else {
            None
        };
        let mut items =
            Vec::with_capacity(regular_items.len() + usize::from(exact_fallback.is_some()));
        if let Some(fallback) = exact_fallback {
            items.push(fallback);
        }
        items.extend(regular_items);

        Ok(IndexedSessionCatalogPage {
            items,
            next_cursor,
            diagnostics,
        })
    }

    pub fn find_indexed_session(
        &self,
        session_id: &str,
    ) -> AppResult<Option<IndexedSessionSummary>> {
        let session_id = session_id.trim();
        if session_id.is_empty() {
            return Ok(None);
        }

        if let Some(summary) = self.find_state_indexed_session(session_id)? {
            return Ok(Some(summary));
        }

        let mut diagnostics = Vec::new();
        let overlay = self.load_index_overlay(&mut diagnostics)?;
        if let Some(entry) = overlay.get(session_id) {
            return Ok(Some(indexed_session_summary_from_index_entry(
                session_id, entry,
            )));
        }

        let mut overlay = Some(overlay);
        self.find_file_backed_indexed_session(session_id, &mut diagnostics, &mut overlay)
    }

    pub fn find_session(&self, session_id: &str) -> AppResult<Option<SessionSummary>> {
        let mut cursor = None;
        loop {
            let page = self.list_sessions(Some(MAX_PAGE_LIMIT), cursor.as_deref(), None)?;
            if let Some(found) = page
                .items
                .into_iter()
                .find(|item| item.session_id == session_id)
            {
                return Ok(Some(found));
            }
            if page.next_cursor.is_none() {
                return Ok(None);
            }
            cursor = page.next_cursor;
        }
    }

    pub fn resolve_session_ref_by_id(&self, session_id: &str) -> AppResult<Option<String>> {
        let session_id = session_id.trim();
        if session_id.is_empty() {
            return Err(AppError::Validation {
                field: "session_id",
                reason: "must not be empty".to_string(),
            });
        }

        if let Ok(Some(session_ref)) = self.resolve_session_ref_by_id_from_state_db(session_id) {
            return Ok(Some(session_ref));
        }

        let mut best_match: Option<(SystemTime, String)> = None;
        find_session_ref_by_id(
            &self.home.sessions_dir,
            &self.home.sessions_dir,
            session_id,
            &mut best_match,
        )?;
        Ok(best_match.map(|(_, session_ref)| session_ref))
    }

    fn load_index_overlay(
        &self,
        diagnostics: &mut Vec<SessionDiagnostic>,
    ) -> AppResult<HashMap<String, SessionIndexEntry>> {
        if !self.home.session_index_path.exists() {
            return Ok(HashMap::new());
        }

        let file = fs::File::open(&self.home.session_index_path)?;
        let reader = BufReader::new(file);
        let mut latest: HashMap<String, SessionIndexEntry> = HashMap::new();
        for (line_idx, line) in reader.lines().enumerate() {
            let raw_line = line?;
            let trimmed = raw_line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let parsed = match serde_json::from_str::<SessionIndexEntryRaw>(trimmed) {
                Ok(value) => value,
                Err(err) => {
                    diagnostics.push(SessionDiagnostic {
                        kind: "invalid_index_line".to_string(),
                        message: format!(
                            "session_index.jsonl line {} ignored: {err}",
                            line_idx + 1
                        ),
                        session_id: None,
                        session_refs: Vec::new(),
                    });
                    continue;
                }
            };
            let session_id = parsed.id.trim().to_string();
            if session_id.is_empty() {
                diagnostics.push(SessionDiagnostic {
                    kind: "invalid_index_line".to_string(),
                    message: format!(
                        "session_index.jsonl line {} ignored: missing id",
                        line_idx + 1
                    ),
                    session_id: None,
                    session_refs: Vec::new(),
                });
                continue;
            }

            let entry = SessionIndexEntry {
                thread_name: parsed.thread_name.filter(|value| !value.trim().is_empty()),
                updated_at: parsed.updated_at,
                line_no: line_idx,
            };
            match latest.get(&session_id) {
                Some(existing)
                    if existing.updated_at > entry.updated_at
                        || (existing.updated_at == entry.updated_at
                            && existing.line_no > entry.line_no) => {}
                _ => {
                    latest.insert(session_id, entry);
                }
            }
        }
        Ok(latest)
    }

    fn discover_session_files(
        &self,
        diagnostics: &mut Vec<SessionDiagnostic>,
    ) -> AppResult<Vec<SessionFileEntry>> {
        let mut files = Vec::new();
        walk_session_files(
            &self.home.sessions_dir,
            &self.home.sessions_dir,
            &mut files,
            diagnostics,
        )?;
        Ok(files)
    }

    fn load_state_indexed_sessions(
        &self,
        diagnostics: &mut Vec<SessionDiagnostic>,
    ) -> AppResult<Option<Vec<IndexedSessionSummary>>> {
        let Some(state_db_path) = find_latest_state_db_path(&self.home.root)? else {
            return Ok(None);
        };

        let connection = match open_state_db(&state_db_path) {
            Ok(connection) => connection,
            Err(err) => {
                diagnostics.push(SessionDiagnostic {
                    kind: "sqlite_catalog_unavailable".to_string(),
                    message: format!(
                        "state sqlite catalog ignored ({}): {err}",
                        state_db_path.display()
                    ),
                    session_id: None,
                    session_refs: Vec::new(),
                });
                return Ok(None);
            }
        };

        let mut statement = match connection.prepare(
            "select id, created_at, updated_at, source, model_provider, cwd, title, sandbox_policy, approval_mode, tokens_used, has_user_event, archived, archived_at, git_sha, git_branch, git_origin_url, cli_version, first_user_message, agent_nickname, agent_role, memory_mode, model, reasoning_effort, agent_path from threads",
        ) {
            Ok(statement) => statement,
            Err(err) => {
                diagnostics.push(SessionDiagnostic {
                    kind: "sqlite_catalog_unavailable".to_string(),
                    message: format!(
                        "state sqlite catalog ignored ({}): {err}",
                        state_db_path.display()
                    ),
                    session_id: None,
                    session_refs: Vec::new(),
                });
                return Ok(None);
            }
        };

        let rows = match statement.query_map([], indexed_session_summary_from_state_row) {
            Ok(rows) => rows,
            Err(err) => {
                diagnostics.push(SessionDiagnostic {
                    kind: "sqlite_catalog_unavailable".to_string(),
                    message: format!(
                        "state sqlite catalog ignored ({}): {err}",
                        state_db_path.display()
                    ),
                    session_id: None,
                    session_refs: Vec::new(),
                });
                return Ok(None);
            }
        };

        let sessions = match rows.collect::<Result<Vec<_>, _>>() {
            Ok(sessions) => sessions,
            Err(err) => {
                diagnostics.push(SessionDiagnostic {
                    kind: "sqlite_catalog_unavailable".to_string(),
                    message: format!(
                        "state sqlite catalog ignored ({}): {err}",
                        state_db_path.display()
                    ),
                    session_id: None,
                    session_refs: Vec::new(),
                });
                return Ok(None);
            }
        };
        Ok(Some(sessions))
    }

    fn find_state_indexed_session(
        &self,
        session_id: &str,
    ) -> AppResult<Option<IndexedSessionSummary>> {
        let Some(state_db_path) = find_latest_state_db_path(&self.home.root)? else {
            return Ok(None);
        };

        let connection = match open_state_db(&state_db_path) {
            Ok(connection) => connection,
            Err(_) => return Ok(None),
        };

        let mut statement = match connection.prepare(
            "select id, created_at, updated_at, source, model_provider, cwd, title, sandbox_policy, approval_mode, tokens_used, has_user_event, archived, archived_at, git_sha, git_branch, git_origin_url, cli_version, first_user_message, agent_nickname, agent_role, memory_mode, model, reasoning_effort, agent_path from threads where id = ?1 limit 1",
        ) {
            Ok(statement) => statement,
            Err(_) => return Ok(None),
        };

        statement
            .query_row([session_id], indexed_session_summary_from_state_row)
            .optional()
            .map_err(|err| AppError::Runner(err.to_string()))
    }

    fn resolve_session_ref_by_id_from_state_db(
        &self,
        session_id: &str,
    ) -> AppResult<Option<String>> {
        let Some(state_db_path) = find_latest_state_db_path(&self.home.root)? else {
            return Ok(None);
        };
        let connection =
            open_state_db(&state_db_path).map_err(|err| AppError::Runner(err.to_string()))?;
        let rollout_path = connection
            .query_row(
                "select rollout_path from threads where id = ?1 limit 1",
                [session_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|err| AppError::Runner(err.to_string()))?;

        Ok(rollout_path
            .and_then(|value| rollout_path_to_session_ref(&self.home.sessions_dir, value.trim())))
    }

    fn find_file_backed_indexed_session(
        &self,
        session_id: &str,
        diagnostics: &mut Vec<SessionDiagnostic>,
        index_overlay: &mut Option<HashMap<String, SessionIndexEntry>>,
    ) -> AppResult<Option<IndexedSessionSummary>> {
        let Some(session_ref) = self.resolve_session_ref_by_id(session_id)? else {
            return Ok(None);
        };
        let path = self.home.resolve_session_ref(&session_ref)?;
        let validated_session_id = match validate_standalone_rollout_root(&path) {
            Ok(value) => value,
            Err(_) => return Ok(None),
        };
        if validated_session_id != session_id {
            return Ok(None);
        }

        if index_overlay.is_none() {
            *index_overlay = Some(self.load_index_overlay(diagnostics)?);
        }

        let preview = read_session_meta_preview(&path)?;
        let modified_at = fs::metadata(&path)?
            .modified()
            .unwrap_or(SystemTime::UNIX_EPOCH);
        let overlay_entry = index_overlay
            .as_ref()
            .and_then(|overlay| overlay.get(session_id));

        Ok(Some(indexed_session_summary_from_file(
            session_id,
            modified_at,
            preview.cwd,
            overlay_entry,
        )))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionReadContext {
    pub task_id: String,
    pub run_id: String,
    pub session_ref: String,
    pub session_id: String,
    pub default_parent_thread_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileIdentity {
    pub device: Option<u64>,
    pub inode: Option<u64>,
    pub size: u64,
    pub modified_unix_ms: Option<u128>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TailCursor {
    pub session_ref: String,
    pub offset: u64,
    pub file_identity: FileIdentity,
    #[serde(default)]
    pub pending_fragment: Vec<u8>,
    #[serde(default)]
    pub call_names: HashMap<String, String>,
    pub resolved_parent_thread_id: Option<String>,
    pub next_seq: u64,
    #[serde(default)]
    pub recent_dedup_keys: Vec<String>,
}

impl TailCursor {
    pub fn new(session_ref: impl Into<String>) -> Self {
        Self {
            session_ref: session_ref.into(),
            offset: 0,
            file_identity: FileIdentity::default(),
            pending_fragment: Vec::new(),
            call_names: HashMap::new(),
            resolved_parent_thread_id: None,
            next_seq: 1,
            recent_dedup_keys: Vec::new(),
        }
    }
}

impl Default for FileIdentity {
    fn default() -> Self {
        Self {
            device: None,
            inode: None,
            size: 0,
            modified_unix_ms: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionReadResult {
    pub events: Vec<EventRecord>,
    pub startup_metadata: Option<Map<String, Value>>,
    pub tail_cursor: TailCursor,
    #[serde(default)]
    pub tool_counts: HashMap<String, u64>,
    #[serde(default)]
    pub subagent_counts: HashMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TailResult {
    pub events: Vec<EventRecord>,
    pub next_cursor: TailCursor,
    pub reset: bool,
    #[serde(default)]
    pub raw_bytes: Vec<u8>,
    #[serde(default)]
    pub tool_counts: HashMap<String, u64>,
    #[serde(default)]
    pub subagent_counts: HashMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadedSession {
    pub session_ref: String,
    pub session_id: String,
    pub tree: EventTree,
    pub tail_cursor: TailCursor,
    #[serde(default)]
    pub metrics: Option<SessionMetrics>,
}

pub struct SessionReader;

impl SessionReader {
    pub fn load_all(path: &Path, context: &SessionReadContext) -> AppResult<SessionReadResult> {
        let mut file = fs::File::open(path)?;
        let identity = file_identity(&file.metadata()?);
        let mut chunk = Vec::new();
        file.read_to_end(&mut chunk)?;
        let mut cursor = TailCursor::new(context.session_ref.clone());
        let lines = split_session_chunk_lines(&mut cursor.pending_fragment, &chunk, true);
        let parsed = parse_session_lines(&lines, path, context, &mut cursor, false)?;
        cursor.file_identity = identity;
        cursor.offset = chunk.len() as u64;

        Ok(SessionReadResult {
            events: parsed.events,
            startup_metadata: parsed.startup_metadata,
            tail_cursor: cursor,
            tool_counts: parsed.tool_counts,
            subagent_counts: parsed.subagent_counts,
        })
    }

    pub fn tail(
        path: &Path,
        context: &SessionReadContext,
        cursor: &TailCursor,
    ) -> AppResult<TailResult> {
        if cursor.session_ref != context.session_ref {
            return Err(AppError::Validation {
                field: "tail_cursor.session_ref",
                reason: "must match requested session_ref".to_string(),
            });
        }

        let mut file = fs::File::open(path)?;
        let metadata = file.metadata()?;
        let identity = file_identity(&metadata);
        let reset =
            cursor.offset > metadata.len() || !same_identity(&cursor.file_identity, &identity);
        let start_offset = if reset { 0 } else { cursor.offset };

        file.seek(SeekFrom::Start(start_offset))?;
        let mut chunk = Vec::new();
        file.read_to_end(&mut chunk)?;
        let mut next_cursor = if reset {
            TailCursor::new(context.session_ref.clone())
        } else {
            cursor.clone()
        };
        let lines = split_session_chunk_lines(&mut next_cursor.pending_fragment, &chunk, false);
        let parsed = parse_session_lines(&lines, path, context, &mut next_cursor, reset)?;
        next_cursor.offset = metadata.len();
        next_cursor.file_identity = identity;

        Ok(TailResult {
            events: parsed.events,
            next_cursor,
            reset,
            raw_bytes: chunk,
            tool_counts: parsed.tool_counts,
            subagent_counts: parsed.subagent_counts,
        })
    }
}

pub struct SessionLoader {
    home: ResolvedCodexHome,
}

impl SessionLoader {
    pub fn new(home: ResolvedCodexHome) -> Self {
        Self { home }
    }

    pub fn load_session(&self, session_ref: &str, text_limit: usize) -> AppResult<LoadedSession> {
        let path = self.home.resolve_session_ref(session_ref)?;
        let session_id = validate_standalone_rollout_root(&path)?;
        let standalone = load_records_from_standalone_rollout(&path, &session_id)?;
        let context = SessionReadContext {
            task_id: "standalone-rollout".to_string(),
            run_id: "standalone-rollout".to_string(),
            session_ref: session_ref.to_string(),
            session_id: session_id.clone(),
            default_parent_thread_id: "standalone-root".to_string(),
        };
        let read_result = SessionReader::load_all(&path, &context)?;
        let tree = build_event_tree_with_standalone_startup_metadata(
            &path,
            &standalone.events,
            Some(standalone.startup_metadata),
            text_limit,
        );

        Ok(LoadedSession {
            session_ref: session_ref.to_string(),
            session_id,
            tree,
            tail_cursor: read_result.tail_cursor,
            metrics: None,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct SessionIndexEntryRaw {
    #[serde(alias = "session_id")]
    id: String,
    thread_name: Option<String>,
    updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionIndexEntry {
    thread_name: Option<String>,
    updated_at: String,
    line_no: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionFileEntry {
    session_id: String,
    session_ref: String,
    updated_at: String,
    modified_at: SystemTime,
    cwd: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SessionMetaPreview {
    session_id: Option<String>,
    cwd: Option<String>,
}

trait OptionalRow<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalRow<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct ParsedSessionChunk {
    events: Vec<EventRecord>,
    startup_metadata: Option<Map<String, Value>>,
    tool_counts: HashMap<String, u64>,
    subagent_counts: HashMap<String, u64>,
}

fn walk_session_files(
    base_root: &Path,
    current_dir: &Path,
    out: &mut Vec<SessionFileEntry>,
    diagnostics: &mut Vec<SessionDiagnostic>,
) -> AppResult<()> {
    let entries = match fs::read_dir(current_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
            diagnostics.push(SessionDiagnostic {
                kind: "unreadable_directory".to_string(),
                message: format!(
                    "directory ignored during discovery: {}",
                    current_dir.display()
                ),
                session_id: None,
                session_refs: Vec::new(),
            });
            return Ok(());
        }
        Err(err) => return Err(err.into()),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            walk_session_files(base_root, &path, out, diagnostics)?;
            continue;
        }
        if !file_type.is_file()
            || path.extension().and_then(|value| value.to_str()) != Some("jsonl")
        {
            continue;
        }

        let preview = read_session_meta_preview(&path)?;
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        let session_id = preview
            .session_id
            .or_else(|| parse_rollout_session_id(file_name).map(str::to_string));
        let Some(session_id) = session_id.filter(|value| !value.trim().is_empty()) else {
            diagnostics.push(SessionDiagnostic {
                kind: "invalid_session_file".to_string(),
                message: format!("session id could not be resolved from {}", path.display()),
                session_id: None,
                session_refs: Vec::new(),
            });
            continue;
        };

        let metadata = entry.metadata()?;
        let modified_at = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let updated_at = system_time_to_rfc3339(modified_at);
        let relative = path
            .strip_prefix(base_root)
            .expect("session path should stay under sessions root");
        let session_ref = relative
            .components()
            .filter_map(|component| match component {
                Component::Normal(value) => value.to_str(),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");

        out.push(SessionFileEntry {
            session_id,
            session_ref,
            updated_at,
            modified_at,
            cwd: preview.cwd,
        });
    }
    Ok(())
}

fn read_session_meta_preview(path: &Path) -> AppResult<SessionMetaPreview> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
            return Ok(SessionMetaPreview::default());
        }
        Err(err) => return Err(err.into()),
    };
    let mut reader = BufReader::new(file);
    let mut first_line = String::new();
    if reader.read_line(&mut first_line)? == 0 {
        return Ok(SessionMetaPreview::default());
    }

    let parsed = match serde_json::from_str::<Value>(first_line.trim_end_matches(['\n', '\r'])) {
        Ok(Value::Object(obj)) => obj,
        _ => return Ok(SessionMetaPreview::default()),
    };
    if parsed.get("type").and_then(Value::as_str) != Some("session_meta") {
        return Ok(SessionMetaPreview::default());
    }

    let payload = parsed.get("payload").and_then(Value::as_object);
    Ok(SessionMetaPreview {
        session_id: payload
            .and_then(|map| map.get("id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        cwd: payload
            .and_then(|map| map.get("cwd"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    })
}

fn find_latest_state_db_path(root: &Path) -> AppResult<Option<PathBuf>> {
    let mut best_match: Option<(u64, SystemTime, PathBuf)> = None;
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if !file_type.is_file() {
            continue;
        }

        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        let Some(generation) = parse_state_db_generation(file_name) else {
            continue;
        };
        let modified_at = entry
            .metadata()?
            .modified()
            .unwrap_or(SystemTime::UNIX_EPOCH);

        match &best_match {
            Some((best_generation, best_modified_at, best_path))
                if *best_generation > generation
                    || (*best_generation == generation
                        && (*best_modified_at > modified_at
                            || (*best_modified_at == modified_at && best_path >= &path))) => {}
            _ => {
                best_match = Some((generation, modified_at, path));
            }
        }
    }

    Ok(best_match.map(|(_, _, path)| path))
}

fn parse_state_db_generation(file_name: &str) -> Option<u64> {
    file_name
        .strip_prefix(STATE_DB_PREFIX)?
        .strip_suffix(STATE_DB_SUFFIX)?
        .parse::<u64>()
        .ok()
}

fn open_state_db(path: &Path) -> Result<Connection, rusqlite::Error> {
    Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
}

fn unix_seconds_to_rfc3339(value: i64) -> String {
    Utc.timestamp_opt(value, 0)
        .single()
        .map(|timestamp| timestamp.to_rfc3339())
        .unwrap_or_else(|| value.to_string())
}

fn select_thread_name_entry(
    title: Option<&str>,
    first_user_message: Option<&str>,
    agent_nickname: Option<&str>,
) -> (Option<String>, Option<String>) {
    [
        ("title", title),
        ("first_user_message", first_user_message),
        ("agent_nickname", agent_nickname),
    ]
    .into_iter()
    .find_map(|(source, value)| {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| (Some(value.to_string()), Some(source.to_string())))
    })
    .unwrap_or((None, None))
}

fn optional_text(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn sandbox_policy_kind(value: Option<&str>) -> Option<String> {
    let raw = optional_text(value)?;
    if let Ok(Value::Object(object)) = serde_json::from_str::<Value>(&raw) {
        if let Some(kind) = object.get("type").and_then(Value::as_str) {
            return optional_text(Some(kind));
        }
    }
    Some(raw)
}

fn indexed_session_summary_from_index_entry(
    session_id: &str,
    entry: &SessionIndexEntry,
) -> IndexedSessionSummary {
    IndexedSessionSummary {
        session_id: session_id.to_string(),
        updated_at: entry.updated_at.clone(),
        thread_name: entry.thread_name.clone(),
        thread_name_source: entry
            .thread_name
            .as_ref()
            .map(|_| "session_index".to_string()),
        cwd: None,
        agent_name: None,
        tokens_used: None,
        created_at: None,
        source: None,
        model_provider: None,
        sandbox_policy_kind: None,
        approval_mode: None,
        has_user_event: None,
        archived: None,
        archived_at: None,
        git_sha: None,
        git_branch: None,
        git_origin_url: None,
        cli_version: None,
        agent_role: None,
        memory_mode: None,
        model: None,
        reasoning_effort: None,
        agent_path: None,
    }
}

fn indexed_session_summary_from_file(
    session_id: &str,
    modified_at: SystemTime,
    cwd: Option<String>,
    overlay_entry: Option<&SessionIndexEntry>,
) -> IndexedSessionSummary {
    IndexedSessionSummary {
        session_id: session_id.to_string(),
        updated_at: system_time_to_rfc3339(modified_at),
        thread_name: overlay_entry.and_then(|entry| entry.thread_name.clone()),
        thread_name_source: overlay_entry.and_then(|entry| {
            entry
                .thread_name
                .as_ref()
                .map(|_| "session_index".to_string())
        }),
        cwd,
        agent_name: None,
        tokens_used: None,
        created_at: None,
        source: None,
        model_provider: None,
        sandbox_policy_kind: None,
        approval_mode: None,
        has_user_event: None,
        archived: None,
        archived_at: None,
        git_sha: None,
        git_branch: None,
        git_origin_url: None,
        cli_version: None,
        agent_role: None,
        memory_mode: None,
        model: None,
        reasoning_effort: None,
        agent_path: None,
    }
}

fn indexed_session_summary_from_state_row(
    row: &rusqlite::Row<'_>,
) -> Result<IndexedSessionSummary, rusqlite::Error> {
    let title: String = row.get(6)?;
    let first_user_message: String = row.get(17)?;
    let cwd: String = row.get(5)?;
    let source: String = row.get(3)?;
    let model_provider: String = row.get(4)?;
    let sandbox_policy: String = row.get(7)?;
    let approval_mode: String = row.get(8)?;
    let cli_version: String = row.get(16)?;
    let memory_mode: String = row.get(20)?;
    let agent_nickname: Option<String> = row.get(18)?;
    let agent_role: Option<String> = row.get(19)?;
    let model: Option<String> = row.get(21)?;
    let reasoning_effort: Option<String> = row.get(22)?;
    let agent_path: Option<String> = row.get(23)?;
    let git_sha: Option<String> = row.get(13)?;
    let git_branch: Option<String> = row.get(14)?;
    let git_origin_url: Option<String> = row.get(15)?;
    let tokens_used: i64 = row.get(9)?;
    let has_user_event: i64 = row.get(10)?;
    let archived: i64 = row.get(11)?;
    let archived_at: Option<i64> = row.get(12)?;
    let (thread_name, thread_name_source) = select_thread_name_entry(
        Some(title.as_str()),
        Some(first_user_message.as_str()),
        agent_nickname.as_deref(),
    );

    Ok(IndexedSessionSummary {
        session_id: row.get(0)?,
        created_at: Some(unix_seconds_to_rfc3339(row.get(1)?)),
        updated_at: unix_seconds_to_rfc3339(row.get(2)?),
        thread_name,
        thread_name_source,
        cwd: optional_text(Some(cwd.as_str())),
        agent_name: optional_text(agent_nickname.as_deref()),
        tokens_used: u64::try_from(tokens_used).ok(),
        source: optional_text(Some(source.as_str())),
        model_provider: optional_text(Some(model_provider.as_str())),
        sandbox_policy_kind: sandbox_policy_kind(Some(sandbox_policy.as_str())),
        approval_mode: optional_text(Some(approval_mode.as_str())),
        has_user_event: Some(has_user_event != 0),
        archived: Some(archived != 0),
        archived_at: archived_at.map(unix_seconds_to_rfc3339),
        git_sha: optional_text(git_sha.as_deref()),
        git_branch: optional_text(git_branch.as_deref()),
        git_origin_url: optional_text(git_origin_url.as_deref()),
        cli_version: optional_text(Some(cli_version.as_str())),
        agent_role: optional_text(agent_role.as_deref()),
        memory_mode: optional_text(Some(memory_mode.as_str())),
        model: optional_text(model.as_deref()),
        reasoning_effort: optional_text(reasoning_effort.as_deref()),
        agent_path: optional_text(agent_path.as_deref()),
    })
}

fn rollout_path_to_session_ref(sessions_dir: &Path, rollout_path: &str) -> Option<String> {
    if rollout_path.is_empty() {
        return None;
    }

    let normalized = normalize_path(Path::new(rollout_path));
    if !normalized.is_absolute() || !normalized.is_file() {
        return None;
    }

    let relative = normalized.strip_prefix(sessions_dir).ok()?;
    Some(
        relative
            .components()
            .filter_map(|component| match component {
                Component::Normal(value) => value.to_str(),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/"),
    )
}

fn find_session_ref_by_id(
    base_root: &Path,
    current_dir: &Path,
    session_id: &str,
    best_match: &mut Option<(SystemTime, String)>,
) -> AppResult<()> {
    let entries = match fs::read_dir(current_dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
            return Ok(());
        }
        Err(err) => return Err(err.into()),
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            find_session_ref_by_id(base_root, &path, session_id, best_match)?;
            continue;
        }
        if !file_type.is_file()
            || path.extension().and_then(|value| value.to_str()) != Some("jsonl")
        {
            continue;
        }

        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if parse_rollout_session_id(file_name) != Some(session_id) {
            continue;
        }

        let metadata = entry.metadata()?;
        let modified_at = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
        let relative = path
            .strip_prefix(base_root)
            .expect("session path should stay under sessions root");
        let session_ref = relative
            .components()
            .filter_map(|component| match component {
                Component::Normal(value) => value.to_str(),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/");

        match best_match {
            Some((current_modified_at, current_ref))
                if *current_modified_at > modified_at
                    || (*current_modified_at == modified_at && *current_ref <= session_ref) => {}
            _ => {
                *best_match = Some((modified_at, session_ref));
            }
        }
    }

    Ok(())
}

fn parse_session_lines(
    lines: &[Vec<u8>],
    path: &Path,
    context: &SessionReadContext,
    cursor: &mut TailCursor,
    reset: bool,
) -> AppResult<ParsedSessionChunk> {
    let mut parser = JsonOutputEventReader::new(
        RunEventContext {
            task_id: context.task_id.clone(),
            run_id: context.run_id.clone(),
        },
        None,
    );
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let mut current_parent_thread_id = cursor
        .resolved_parent_thread_id
        .clone()
        .unwrap_or_else(|| context.default_parent_thread_id.clone());
    let recent_keys = cursor.recent_dedup_keys.clone();
    let mut next_recent = VecDeque::from(cursor.recent_dedup_keys.clone());
    let mut events = Vec::new();
    let mut startup_metadata = None;

    for raw_line in lines {
        let stripped = trim_line_bytes(raw_line);
        if stripped.is_empty() {
            continue;
        }
        let seq = cursor.next_seq;
        cursor.next_seq += 1;
        let ts = utc_now_iso();
        let line_text = match std::str::from_utf8(stripped) {
            Ok(text) => text,
            Err(_) => {
                let event = subagent_unparsed_event(
                    context,
                    seq,
                    &ts,
                    "invalid_utf8",
                    String::from_utf8_lossy(stripped).into_owned(),
                );
                push_dedup_key(&mut next_recent, dedup_key_for_line(None, stripped));
                if !(reset && recent_keys.contains(&dedup_key_for_line(None, stripped))) {
                    events.push(event);
                }
                continue;
            }
        };

        let parsed_value = match serde_json::from_str::<Value>(line_text) {
            Ok(Value::Object(obj)) => obj,
            _ => {
                let event = subagent_unparsed_event(
                    context,
                    seq,
                    &ts,
                    "invalid_json",
                    line_text.to_string(),
                );
                let key = dedup_key_for_line(None, stripped);
                push_dedup_key(&mut next_recent, key.clone());
                if !(reset && recent_keys.contains(&key)) {
                    events.push(event);
                }
                continue;
            }
        };

        if startup_metadata.is_none()
            && parsed_value.get("type").and_then(Value::as_str) == Some("session_meta")
        {
            startup_metadata = parsed_value
                .get("payload")
                .and_then(Value::as_object)
                .cloned();
        }

        if let Some(meta) = imported_subagent_session_meta(&parsed_value, &context.session_id) {
            if let Some(parent_thread_id) = meta.parent_thread_id {
                current_parent_thread_id = parent_thread_id.clone();
                cursor.resolved_parent_thread_id = Some(parent_thread_id);
            }
        }

        let event = parser.parse_subagent_session_payload(
            seq,
            &parsed_value,
            path,
            &current_parent_thread_id,
            &context.session_id,
            &mut cursor.call_names,
            &mut tool_counts,
            &mut subagent_counts,
        );

        let key = dedup_key_for_line(Some(&parsed_value), stripped);
        push_dedup_key(&mut next_recent, key.clone());
        if reset && recent_keys.contains(&key) {
            continue;
        }
        if let Some(event) = event {
            events.push(event.to_record());
        }
    }

    cursor.recent_dedup_keys = next_recent.into_iter().collect();

    Ok(ParsedSessionChunk {
        events,
        startup_metadata,
        tool_counts,
        subagent_counts,
    })
}

fn subagent_unparsed_event(
    context: &SessionReadContext,
    seq: u64,
    ts: &str,
    raw_type: &str,
    text: String,
) -> EventRecord {
    EventRecord {
        schema_version: 1,
        ts: ts.to_string(),
        task_id: context.task_id.clone(),
        run_id: context.run_id.clone(),
        seq,
        event_type: "raw.unparsed".to_string(),
        raw_type: raw_type.to_string(),
        parse_status: "unparsed".to_string(),
        payload: Value::Object(Map::from_iter([
            ("actor_type".to_string(), Value::from("subagent")),
            (
                "thread_id".to_string(),
                Value::from(context.session_id.clone()),
            ),
            ("text".to_string(), Value::from(text)),
        ])),
    }
}

pub fn session_file_name_matches_thread_id(file_name: &str, thread_id: &str) -> bool {
    file_name.match_indices(thread_id).any(|(start, _)| {
        let before = file_name[..start].chars().next_back();
        let after = file_name[start + thread_id.len()..].chars().next();
        session_name_boundary(before) && session_name_boundary(after)
    })
}

pub fn split_session_chunk_lines(
    partial_bytes: &mut Vec<u8>,
    chunk: &[u8],
    final_pass: bool,
) -> Vec<Vec<u8>> {
    let mut combined = std::mem::take(partial_bytes);
    combined.extend_from_slice(chunk);

    let mut lines = Vec::new();
    let mut line_start = 0usize;
    for (idx, byte) in combined.iter().enumerate() {
        if *byte == b'\n' {
            lines.push(combined[line_start..idx].to_vec());
            line_start = idx + 1;
        }
    }

    if final_pass {
        if line_start < combined.len() {
            lines.push(combined[line_start..].to_vec());
        }
    } else {
        partial_bytes.extend_from_slice(&combined[line_start..]);
    }

    lines
}

pub fn trim_line_bytes(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r").unwrap_or(line)
}

fn same_identity(left: &FileIdentity, right: &FileIdentity) -> bool {
    if left.device.is_some()
        && left.inode.is_some()
        && right.device.is_some()
        && right.inode.is_some()
    {
        left.device == right.device && left.inode == right.inode
    } else {
        left == right
    }
}

fn file_identity(metadata: &fs::Metadata) -> FileIdentity {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        return FileIdentity {
            device: Some(metadata.dev()),
            inode: Some(metadata.ino()),
            size: metadata.len(),
            modified_unix_ms: metadata.modified().ok().and_then(system_time_to_unix_ms),
        };
    }

    #[cfg(not(unix))]
    {
        FileIdentity {
            device: None,
            inode: None,
            size: metadata.len(),
            modified_unix_ms: metadata.modified().ok().and_then(system_time_to_unix_ms),
        }
    }
}

fn system_time_to_unix_ms(time: SystemTime) -> Option<u128> {
    time.duration_since(SystemTime::UNIX_EPOCH)
        .ok()
        .map(|value| value.as_millis())
}

fn system_time_to_rfc3339(time: SystemTime) -> String {
    DateTime::<Utc>::from(time).to_rfc3339()
}

fn is_active_like(modified_at: SystemTime) -> bool {
    match SystemTime::now().duration_since(modified_at) {
        Ok(age) => age.as_secs() < 120,
        Err(_) => true,
    }
}

fn matches_query(summary: &SessionSummary, query: Option<&str>) -> bool {
    let Some(query) = query else {
        return true;
    };
    let query = query.trim();
    if query.is_empty() {
        return true;
    }

    [
        summary.session_id.as_str(),
        summary.session_ref.as_str(),
        summary.thread_name.as_deref().unwrap_or_default(),
        summary.cwd.as_deref().unwrap_or_default(),
    ]
    .iter()
    .any(|value| value.to_lowercase().contains(query))
}

fn matches_index_query(summary: &IndexedSessionSummary, query: Option<&str>) -> bool {
    let Some(query) = query else {
        return true;
    };
    let query = query.trim();
    if query.is_empty() {
        return true;
    }

    [
        summary.session_id.as_str(),
        summary.thread_name.as_deref().unwrap_or_default(),
        summary.cwd.as_deref().unwrap_or_default(),
        summary.agent_name.as_deref().unwrap_or_default(),
        summary.agent_role.as_deref().unwrap_or_default(),
        summary.model.as_deref().unwrap_or_default(),
        summary.reasoning_effort.as_deref().unwrap_or_default(),
        summary.git_branch.as_deref().unwrap_or_default(),
        summary.git_sha.as_deref().unwrap_or_default(),
    ]
    .iter()
    .any(|value| value.to_lowercase().contains(query))
}

fn session_name_boundary(ch: Option<char>) -> bool {
    ch.is_none_or(|value| !value.is_ascii_alphanumeric())
}

fn parse_rollout_session_id(file_name: &str) -> Option<&str> {
    const PREFIX: &str = "rollout-";
    const SUFFIX: &str = ".jsonl";
    let body = file_name.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?;
    if body.len() <= 20 {
        return None;
    }
    let (timestamp, rest) = body.split_at(19);
    if !is_rollout_timestamp_prefix(timestamp) || !rest.starts_with('-') {
        return None;
    }
    let session_id = &rest[1..];
    if session_id.trim().is_empty() {
        None
    } else {
        Some(session_id)
    }
}

fn is_rollout_timestamp_prefix(value: &str) -> bool {
    value.len() == 19
        && value.chars().enumerate().all(|(idx, ch)| match idx {
            4 | 7 => ch == '-',
            10 => ch == 'T',
            13 | 16 => ch == '-',
            _ => ch.is_ascii_digit(),
        })
}

fn dedup_key_for_line(parsed: Option<&Map<String, Value>>, raw_line: &[u8]) -> String {
    if let Some(parsed) = parsed {
        let timestamp = parsed
            .get("timestamp")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let kind = parsed
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !timestamp.is_empty() && !kind.is_empty() {
            return format!(
                "event:{timestamp}:{kind}:{}",
                hash8(&serde_json::to_string(parsed).unwrap_or_default())
            );
        }
    }
    format!("line:{}", hash8(&String::from_utf8_lossy(raw_line)))
}

fn push_dedup_key(history: &mut VecDeque<String>, key: String) {
    history.push_back(key);
    while history.len() > RECENT_DEDUP_WINDOW {
        history.pop_front();
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;
    use std::thread;
    use std::time::Duration;

    use tempfile::tempdir;

    use super::{
        IndexedSessionCatalogPage, ResolvedCodexHome, SessionCatalog, SessionLoader,
        SessionReadContext, SessionReader, INDEXED_SESSION_CURSOR_PREFIX,
    };
    use rusqlite::Connection;

    fn write_rollout_file(path: &Path, session_id: &str, lines: &[&str]) {
        let mut content = format!(
            "{{\"timestamp\":\"2026-04-07T10:00:00Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"{session_id}\",\"cwd\":\"/repo\"}}}}\n"
        );
        for line in lines {
            content.push_str(line);
            content.push('\n');
        }
        fs::write(path, content).expect("rollout file should be written");
    }

    struct StateThreadRow<'a> {
        session_id: &'a str,
        rollout_path: &'a Path,
        updated_at: i64,
        cwd: &'a str,
        title: &'a str,
        first_user_message: &'a str,
        agent_nickname: Option<&'a str>,
        tokens_used: i64,
    }

    fn write_state_threads_db(home: &Path, rows: &[StateThreadRow<'_>]) {
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
    fn catalog_merges_files_and_index_overlay() {
        let tmp = tempdir().expect("tmpdir should exist");
        let home = tmp.path().join(".codex");
        let sessions = home.join("sessions").join("2026").join("04").join("07");
        fs::create_dir_all(&sessions).expect("sessions dir should exist");

        write_rollout_file(
            &sessions.join("rollout-2026-04-07T10-00-00-session-a.jsonl"),
            "session-a",
            &[],
        );
        write_rollout_file(
            &sessions.join("rollout-2026-04-07T10-00-00-session-b.jsonl"),
            "session-b",
            &[],
        );
        fs::write(
            home.join("session_index.jsonl"),
            concat!(
                "{\"id\":\"session-a\",\"thread_name\":\"Alpha\",\"updated_at\":\"2026-04-07T10:01:00Z\"}\n",
                "{\"id\":\"session-a\",\"thread_name\":\"Alpha older\",\"updated_at\":\"2026-04-07T09:00:00Z\"}\n",
                "{bad json\n",
                "{\"id\":\"stale-only\",\"thread_name\":\"Stale\",\"updated_at\":\"2026-04-07T11:00:00Z\"}\n"
            ),
        )
        .expect("index should be written");

        let home = ResolvedCodexHome::initialize(Some(home)).expect("codex home should resolve");
        let catalog = SessionCatalog::new(home);
        let page = catalog
            .list_sessions(Some(10), None, None)
            .expect("catalog should load");

        assert_eq!(page.items.len(), 2);
        let indexed = page
            .items
            .iter()
            .find(|item| item.session_id == "session-a")
            .expect("indexed session must exist");
        assert_eq!(indexed.thread_name.as_deref(), Some("Alpha"));
        assert_eq!(indexed.index_status, "indexed");
        let missing = page
            .items
            .iter()
            .find(|item| item.session_id == "session-b")
            .expect("file-only session must exist");
        assert_eq!(missing.index_status, "missing");
        assert!(page
            .diagnostics
            .iter()
            .any(|item| item.kind == "invalid_index_line"));
        assert!(!page
            .items
            .iter()
            .any(|item| item.session_id == "stale-only"));
    }

    #[test]
    fn catalog_prefers_latest_file_for_duplicate_session_id() {
        let tmp = tempdir().expect("tmpdir should exist");
        let home = tmp.path().join(".codex");
        let sessions = home.join("sessions").join("2026").join("04").join("07");
        fs::create_dir_all(&sessions).expect("sessions dir should exist");

        let older = sessions.join("rollout-2026-04-07T10-00-00-session-a.jsonl");
        let newer = sessions.join("rollout-2026-04-07T10-00-01-session-a.jsonl");
        write_rollout_file(&older, "session-a", &[]);
        thread::sleep(Duration::from_millis(20));
        write_rollout_file(&newer, "session-a", &[]);

        let home = ResolvedCodexHome::initialize(Some(home)).expect("codex home should resolve");
        let catalog = SessionCatalog::new(home);
        let page = catalog
            .list_sessions(Some(10), None, None)
            .expect("catalog should load");

        assert_eq!(page.items.len(), 1);
        assert_eq!(
            page.items[0].session_ref,
            "2026/04/07/rollout-2026-04-07T10-00-01-session-a.jsonl"
        );
        assert!(page
            .diagnostics
            .iter()
            .any(|item| item.kind == "duplicate_session_files"));
    }

    #[test]
    fn indexed_catalog_reads_only_session_index() {
        let tmp = tempdir().expect("tmpdir should exist");
        let home = tmp.path().join(".codex");
        fs::create_dir_all(home.join("sessions")).expect("sessions dir should exist");

        fs::write(
            home.join("session_index.jsonl"),
            concat!(
                "{\"id\":\"session-a\",\"thread_name\":\"Alpha\",\"updated_at\":\"2026-04-07T10:01:00Z\"}\n",
                "{\"id\":\"stale-only\",\"thread_name\":\"Stale\",\"updated_at\":\"2026-04-07T11:00:00Z\"}\n",
                "{bad json\n"
            ),
        )
        .expect("index should be written");

        let home = ResolvedCodexHome::initialize(Some(home)).expect("codex home should resolve");
        let catalog = SessionCatalog::new(home);
        let page: IndexedSessionCatalogPage = catalog
            .list_indexed_sessions(Some(10), None, None, None)
            .expect("indexed catalog should load");

        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].session_id, "stale-only");
        assert_eq!(page.items[1].thread_name.as_deref(), Some("Alpha"));
        assert_eq!(page.items[0].cwd, None);
        assert_eq!(page.items[0].agent_name, None);
        assert_eq!(page.items[0].tokens_used, None);
        assert!(page
            .diagnostics
            .iter()
            .any(|item| item.kind == "invalid_index_line"));
    }

    #[test]
    fn indexed_catalog_prefers_sqlite_threads_when_state_db_exists() {
        let tmp = tempdir().expect("tmpdir should exist");
        let home = tmp.path().join(".codex");
        let sessions = home.join("sessions").join("2026").join("04").join("07");
        fs::create_dir_all(&sessions).expect("sessions dir should exist");

        let session_a = sessions.join("rollout-2026-04-07T10-00-00-session-a.jsonl");
        let session_b = sessions.join("rollout-2026-04-07T10-00-00-session-b.jsonl");
        write_rollout_file(&session_a, "session-a", &[]);
        write_rollout_file(&session_b, "session-b", &[]);
        fs::write(
            home.join("session_index.jsonl"),
            "{\"id\":\"session-index-only\",\"thread_name\":\"Index only\",\"updated_at\":\"2026-04-07T11:00:00Z\"}\n",
        )
        .expect("index should be written");
        write_state_threads_db(
            &home,
            &[
                StateThreadRow {
                    session_id: "session-a",
                    rollout_path: &session_a,
                    updated_at: 1_775_560_607,
                    cwd: "/repo/alpha",
                    title: "SQLite Alpha",
                    first_user_message: "ignored",
                    agent_nickname: None,
                    tokens_used: 42,
                },
                StateThreadRow {
                    session_id: "session-b",
                    rollout_path: &session_b,
                    updated_at: 1_775_560_701,
                    cwd: "/repo/beta",
                    title: "",
                    first_user_message: "",
                    agent_nickname: Some("Archimedes"),
                    tokens_used: 777,
                },
            ],
        );

        let home = ResolvedCodexHome::initialize(Some(home)).expect("codex home should resolve");
        let catalog = SessionCatalog::new(home);
        let page: IndexedSessionCatalogPage = catalog
            .list_indexed_sessions(Some(10), None, None, None)
            .expect("indexed catalog should load");

        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].session_id, "session-b");
        assert_eq!(page.items[0].thread_name.as_deref(), Some("Archimedes"));
        assert_eq!(page.items[0].cwd.as_deref(), Some("/repo/beta"));
        assert_eq!(page.items[0].agent_name.as_deref(), Some("Archimedes"));
        assert_eq!(page.items[0].tokens_used, Some(777));
        assert_eq!(page.items[0].source.as_deref(), Some("cli"));
        assert_eq!(page.items[0].model_provider.as_deref(), Some("openai"));
        assert_eq!(
            page.items[0].sandbox_policy_kind.as_deref(),
            Some("danger-full-access")
        );
        assert_eq!(page.items[0].approval_mode.as_deref(), Some("never"));
        assert_eq!(page.items[0].memory_mode.as_deref(), Some("enabled"));
        assert_eq!(page.items[0].cli_version.as_deref(), Some("0.118.0"));
        assert_eq!(page.items[0].archived, Some(false));
        assert_eq!(page.items[0].has_user_event, Some(false));
        assert_eq!(page.items[1].thread_name.as_deref(), Some("SQLite Alpha"));
        assert_eq!(page.items[1].cwd.as_deref(), Some("/repo/alpha"));
        assert_eq!(page.items[1].tokens_used, Some(42));
        assert!(!page
            .items
            .iter()
            .any(|item| item.session_id == "session-index-only"));
    }

    #[test]
    fn indexed_catalog_free_text_query_does_not_inject_file_backed_match() {
        let tmp = tempdir().expect("tmpdir should exist");
        let home = tmp.path().join(".codex");
        let sessions = home.join("sessions").join("2026").join("04").join("07");
        fs::create_dir_all(&sessions).expect("sessions dir should exist");

        let exact = sessions.join("rollout-2026-04-07T10-00-00-session-a.jsonl");
        let other_a = sessions.join("rollout-2026-04-07T10-00-00-other-a.jsonl");
        let other_b = sessions.join("rollout-2026-04-07T10-00-00-other-b.jsonl");
        write_rollout_file(&exact, "session-a", &[]);
        write_rollout_file(&other_a, "other-a", &[]);
        write_rollout_file(&other_b, "other-b", &[]);
        write_state_threads_db(
            &home,
            &[
                StateThreadRow {
                    session_id: "other-a",
                    rollout_path: &other_a,
                    updated_at: 1_775_560_607,
                    cwd: "/repo/alpha",
                    title: "contains session-a",
                    first_user_message: "",
                    agent_nickname: None,
                    tokens_used: 41,
                },
                StateThreadRow {
                    session_id: "other-b",
                    rollout_path: &other_b,
                    updated_at: 1_775_560_701,
                    cwd: "/repo/beta",
                    title: "contains session-a too",
                    first_user_message: "",
                    agent_nickname: None,
                    tokens_used: 42,
                },
            ],
        );

        let home = ResolvedCodexHome::initialize(Some(home)).expect("codex home should resolve");
        let catalog = SessionCatalog::new(home);
        let page = catalog
            .list_indexed_sessions(Some(10), None, Some("session-a"), None)
            .expect("indexed catalog should load");

        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].session_id, "other-b");
        assert_eq!(page.items[1].session_id, "other-a");
        assert_eq!(page.next_cursor, None);
    }

    #[test]
    fn indexed_catalog_injects_exact_file_backed_match_on_first_page_only() {
        let tmp = tempdir().expect("tmpdir should exist");
        let home = tmp.path().join(".codex");
        let sessions = home.join("sessions").join("2026").join("04").join("07");
        fs::create_dir_all(&sessions).expect("sessions dir should exist");

        let exact = sessions.join("rollout-2026-04-07T10-00-00-session-a.jsonl");
        let other_a = sessions.join("rollout-2026-04-07T10-00-00-other-a.jsonl");
        let other_b = sessions.join("rollout-2026-04-07T10-00-00-other-b.jsonl");
        write_rollout_file(&exact, "session-a", &[]);
        write_rollout_file(&other_a, "other-a", &[]);
        write_rollout_file(&other_b, "other-b", &[]);
        write_state_threads_db(
            &home,
            &[
                StateThreadRow {
                    session_id: "other-a",
                    rollout_path: &other_a,
                    updated_at: 1_775_560_607,
                    cwd: "/repo/alpha",
                    title: "contains session-a",
                    first_user_message: "",
                    agent_nickname: None,
                    tokens_used: 41,
                },
                StateThreadRow {
                    session_id: "other-b",
                    rollout_path: &other_b,
                    updated_at: 1_775_560_701,
                    cwd: "/repo/beta",
                    title: "contains session-a too",
                    first_user_message: "",
                    agent_nickname: None,
                    tokens_used: 42,
                },
            ],
        );

        let home = ResolvedCodexHome::initialize(Some(home)).expect("codex home should resolve");
        let catalog = SessionCatalog::new(home);
        let first_page = catalog
            .list_indexed_sessions(Some(2), None, Some("session-a"), Some("session-a"))
            .expect("indexed catalog should load");

        assert_eq!(first_page.items.len(), 2);
        assert_eq!(first_page.items[0].session_id, "session-a");
        assert_eq!(first_page.items[0].cwd.as_deref(), Some("/repo"));
        assert_eq!(first_page.items[0].tokens_used, None);
        assert_eq!(first_page.items[1].session_id, "other-b");
        let expected_second_cursor = format!("{INDEXED_SESSION_CURSOR_PREFIX}1:1");
        assert_eq!(
            first_page.next_cursor.as_deref(),
            Some(expected_second_cursor.as_str())
        );

        let second_cursor = first_page
            .next_cursor
            .clone()
            .expect("first page should expose opaque continuation");
        assert_eq!(second_cursor, expected_second_cursor);
        let second_page = catalog
            .list_indexed_sessions(
                Some(2),
                Some(&second_cursor),
                Some("session-a"),
                Some("session-a"),
            )
            .expect("second page should load");

        assert_eq!(second_page.items.len(), 1);
        assert_eq!(second_page.items[0].session_id, "other-a");
        assert_eq!(second_page.next_cursor, None);
    }

    #[test]
    fn indexed_catalog_exact_fallback_limit_one_encodes_page_state_in_cursor() {
        let tmp = tempdir().expect("tmpdir should exist");
        let home = tmp.path().join(".codex");
        let sessions = home.join("sessions").join("2026").join("04").join("07");
        fs::create_dir_all(&sessions).expect("sessions dir should exist");

        let exact = sessions.join("rollout-2026-04-07T10-00-00-session-a.jsonl");
        let other_a = sessions.join("rollout-2026-04-07T10-00-00-other-a.jsonl");
        let other_b = sessions.join("rollout-2026-04-07T10-00-00-other-b.jsonl");
        write_rollout_file(&exact, "session-a", &[]);
        write_rollout_file(&other_a, "other-a", &[]);
        write_rollout_file(&other_b, "other-b", &[]);
        write_state_threads_db(
            &home,
            &[
                StateThreadRow {
                    session_id: "other-a",
                    rollout_path: &other_a,
                    updated_at: 1_775_560_607,
                    cwd: "/repo/alpha",
                    title: "contains session-a",
                    first_user_message: "",
                    agent_nickname: None,
                    tokens_used: 41,
                },
                StateThreadRow {
                    session_id: "other-b",
                    rollout_path: &other_b,
                    updated_at: 1_775_560_701,
                    cwd: "/repo/beta",
                    title: "contains session-a too",
                    first_user_message: "",
                    agent_nickname: None,
                    tokens_used: 42,
                },
            ],
        );

        let home = ResolvedCodexHome::initialize(Some(home)).expect("codex home should resolve");
        let catalog = SessionCatalog::new(home);
        let first_page = catalog
            .list_indexed_sessions(Some(1), None, Some("session-a"), Some("session-a"))
            .expect("indexed catalog should load");
        let first_page_from_zero = catalog
            .list_indexed_sessions(Some(1), Some("0"), Some("session-a"), Some("session-a"))
            .expect("numeric zero cursor should load");
        let first_page_from_empty = catalog
            .list_indexed_sessions(Some(1), Some(""), Some("session-a"), Some("session-a"))
            .expect("empty cursor should load");

        assert_eq!(first_page.items.len(), 1);
        assert_eq!(first_page.items[0].session_id, "session-a");
        assert_eq!(first_page.items[0].cwd.as_deref(), Some("/repo"));
        assert_eq!(first_page, first_page_from_zero);
        assert_eq!(first_page, first_page_from_empty);

        let second_cursor = first_page
            .next_cursor
            .clone()
            .expect("fallback-only first page must expose next cursor");
        assert_eq!(second_cursor, format!("{INDEXED_SESSION_CURSOR_PREFIX}0:1"));

        let second_page = catalog
            .list_indexed_sessions(
                Some(1),
                Some(&second_cursor),
                Some("session-a"),
                Some("session-a"),
            )
            .expect("opaque cursor continuation should load");

        assert_eq!(second_page.items.len(), 1);
        assert_eq!(second_page.items[0].session_id, "other-b");
        let third_cursor = format!("{INDEXED_SESSION_CURSOR_PREFIX}1:1");
        assert_eq!(
            second_page.next_cursor.as_deref(),
            Some(third_cursor.as_str())
        );

        let third_page = catalog
            .list_indexed_sessions(
                Some(1),
                second_page.next_cursor.as_deref(),
                Some("session-a"),
                Some("session-a"),
            )
            .expect("legacy numeric continuation should load");

        assert_eq!(third_page.items.len(), 1);
        assert_eq!(third_page.items[0].session_id, "other-a");
        assert_eq!(third_page.next_cursor, None);

        let third_page_from_legacy_numeric = catalog
            .list_indexed_sessions(Some(1), Some("1"), Some("session-a"), Some("session-a"))
            .expect("legacy numeric continuation should still load");

        assert_eq!(third_page_from_legacy_numeric, third_page);
    }

    #[test]
    fn indexed_catalog_does_not_duplicate_exact_match_when_primary_contains_it() {
        let tmp = tempdir().expect("tmpdir should exist");
        let home = tmp.path().join(".codex");
        let sessions = home.join("sessions").join("2026").join("04").join("07");
        fs::create_dir_all(&sessions).expect("sessions dir should exist");

        let session_a = sessions.join("rollout-2026-04-07T10-00-00-session-a.jsonl");
        write_rollout_file(&session_a, "session-a", &[]);
        write_state_threads_db(
            &home,
            &[StateThreadRow {
                session_id: "session-a",
                rollout_path: &session_a,
                updated_at: 1_775_560_607,
                cwd: "/repo/alpha",
                title: "SQLite Alpha",
                first_user_message: "",
                agent_nickname: None,
                tokens_used: 42,
            }],
        );

        let home = ResolvedCodexHome::initialize(Some(home)).expect("codex home should resolve");
        let catalog = SessionCatalog::new(home);
        let page = catalog
            .list_indexed_sessions(Some(10), None, Some("session-a"), Some("session-a"))
            .expect("indexed catalog should load");

        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].session_id, "session-a");
        assert_eq!(page.items[0].tokens_used, Some(42));
        assert_eq!(page.items[0].cwd.as_deref(), Some("/repo/alpha"));
    }

    #[test]
    fn indexed_catalog_exact_fallback_ignores_meta_only_non_rollout_files() {
        let tmp = tempdir().expect("tmpdir should exist");
        let home = tmp.path().join(".codex");
        let sessions = home.join("sessions").join("2026").join("04").join("07");
        fs::create_dir_all(&sessions).expect("sessions dir should exist");

        write_rollout_file(&sessions.join("meta-only.jsonl"), "session-a", &[]);

        let home = ResolvedCodexHome::initialize(Some(home)).expect("codex home should resolve");
        let catalog = SessionCatalog::new(home);
        let page = catalog
            .list_indexed_sessions(Some(10), None, Some("session-a"), Some("session-a"))
            .expect("indexed catalog should load");

        assert!(page.items.is_empty());
        assert_eq!(
            catalog
                .find_indexed_session("session-a")
                .expect("lookup should succeed"),
            None
        );
    }

    #[test]
    fn find_indexed_session_synthesizes_file_backed_summary() {
        let tmp = tempdir().expect("tmpdir should exist");
        let home = tmp.path().join(".codex");
        let sessions = home.join("sessions").join("2026").join("04").join("07");
        fs::create_dir_all(&sessions).expect("sessions dir should exist");

        write_rollout_file(
            &sessions.join("rollout-2026-04-07T10-00-00-session-a.jsonl"),
            "session-a",
            &[],
        );

        let home = ResolvedCodexHome::initialize(Some(home)).expect("codex home should resolve");
        let catalog = SessionCatalog::new(home);
        let summary = catalog
            .find_indexed_session("session-a")
            .expect("lookup should succeed")
            .expect("summary should exist");

        assert_eq!(summary.session_id, "session-a");
        assert_eq!(summary.cwd.as_deref(), Some("/repo"));
        assert_eq!(summary.tokens_used, None);
    }

    #[test]
    fn resolve_session_ref_by_id_prefers_latest_rollout_file() {
        let tmp = tempdir().expect("tmpdir should exist");
        let home = tmp.path().join(".codex");
        let sessions = home.join("sessions").join("2026").join("04").join("07");
        fs::create_dir_all(&sessions).expect("sessions dir should exist");

        let older = sessions.join("rollout-2026-04-07T10-00-00-session-a.jsonl");
        let newer = sessions.join("rollout-2026-04-07T10-00-01-session-a.jsonl");
        write_rollout_file(&older, "session-a", &[]);
        thread::sleep(Duration::from_millis(20));
        write_rollout_file(&newer, "session-a", &[]);

        let home = ResolvedCodexHome::initialize(Some(home)).expect("codex home should resolve");
        let catalog = SessionCatalog::new(home);
        let session_ref = catalog
            .resolve_session_ref_by_id("session-a")
            .expect("resolution should succeed")
            .expect("session ref should exist");

        assert_eq!(
            session_ref,
            "2026/04/07/rollout-2026-04-07T10-00-01-session-a.jsonl"
        );
    }

    #[test]
    fn resolve_session_ref_by_id_prefers_sqlite_rollout_path() {
        let tmp = tempdir().expect("tmpdir should exist");
        let home = tmp.path().join(".codex");
        let sessions = home.join("sessions").join("2026").join("04").join("07");
        fs::create_dir_all(&sessions).expect("sessions dir should exist");

        let older = sessions.join("rollout-2026-04-07T10-00-00-session-a.jsonl");
        let newer = sessions.join("rollout-2026-04-07T10-00-01-session-a.jsonl");
        write_rollout_file(&older, "session-a", &[]);
        thread::sleep(Duration::from_millis(20));
        write_rollout_file(&newer, "session-a", &[]);
        write_state_threads_db(
            &home,
            &[StateThreadRow {
                session_id: "session-a",
                rollout_path: &older,
                updated_at: 1_775_560_607,
                cwd: "/repo/alpha",
                title: "SQLite Alpha",
                first_user_message: "SQLite Alpha",
                agent_nickname: None,
                tokens_used: 42,
            }],
        );

        let home = ResolvedCodexHome::initialize(Some(home)).expect("codex home should resolve");
        let catalog = SessionCatalog::new(home);
        let session_ref = catalog
            .resolve_session_ref_by_id("session-a")
            .expect("resolution should succeed")
            .expect("session ref should exist");

        assert_eq!(
            session_ref,
            "2026/04/07/rollout-2026-04-07T10-00-00-session-a.jsonl"
        );
    }

    #[test]
    fn reader_loads_and_tails_append_without_duplicates() {
        let tmp = tempdir().expect("tmpdir should exist");
        let path = tmp
            .path()
            .join("rollout-2026-04-07T10-00-00-session-a.jsonl");
        write_rollout_file(
            &path,
            "session-a",
            &[
                r#"{"timestamp":"2026-04-07T10:00:01Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"text":"hello hello hello hello hello hello"}]}}"#,
            ],
        );

        let context = SessionReadContext {
            task_id: "task".to_string(),
            run_id: "run".to_string(),
            session_ref: "2026/04/07/rollout-2026-04-07T10-00-00-session-a.jsonl".to_string(),
            session_id: "session-a".to_string(),
            default_parent_thread_id: "root".to_string(),
        };
        let loaded = SessionReader::load_all(&path, &context).expect("session should load");
        assert_eq!(loaded.events.len(), 2);

        fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("session should be appendable");
        fs::write(
            &path,
            format!(
                "{}{}",
                fs::read_to_string(&path).expect("session content should be readable"),
                "{\"timestamp\":\"2026-04-07T10:00:02Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"text\":\"tail\"}]}}\n"
            ),
        )
        .expect("tail event should be appended");

        let tailed =
            SessionReader::tail(&path, &context, &loaded.tail_cursor).expect("tail should succeed");
        assert!(!tailed.reset);
        assert_eq!(tailed.events.len(), 1);
        assert_eq!(tailed.events[0].payload["text"], "tail");
    }

    #[test]
    fn reader_waits_for_completed_newline_before_emitting_partial_line() {
        let tmp = tempdir().expect("tmpdir should exist");
        let path = tmp
            .path()
            .join("rollout-2026-04-07T10-00-00-session-a.jsonl");
        write_rollout_file(&path, "session-a", &[]);

        let context = SessionReadContext {
            task_id: "task".to_string(),
            run_id: "run".to_string(),
            session_ref: "2026/04/07/rollout-2026-04-07T10-00-00-session-a.jsonl".to_string(),
            session_id: "session-a".to_string(),
            default_parent_thread_id: "root".to_string(),
        };
        let loaded = SessionReader::load_all(&path, &context).expect("session should load");
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("session should be appendable");
        use std::io::Write as _;
        file.write_all(b"{\"timestamp\":\"2026-04-07T10:00:02Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"text\":\"\xD0")
            .expect("partial prefix should be written");
        file.flush().expect("partial prefix should flush");

        let partial =
            SessionReader::tail(&path, &context, &loaded.tail_cursor).expect("tail should succeed");
        assert!(partial.events.is_empty());
        assert!(!partial.next_cursor.pending_fragment.is_empty());

        file.write_all(b"\x9F\"}]}}\n")
            .expect("partial suffix should be written");
        file.flush().expect("partial suffix should flush");

        let complete = SessionReader::tail(&path, &context, &partial.next_cursor)
            .expect("tail should succeed");
        assert_eq!(complete.events.len(), 1);
        assert_eq!(complete.events[0].payload["text"], "П");
    }

    #[test]
    fn reader_resets_on_truncate_and_identity_change() {
        let tmp = tempdir().expect("tmpdir should exist");
        let path = tmp
            .path()
            .join("rollout-2026-04-07T10-00-00-session-a.jsonl");
        let long_text = "hello ".repeat(256);
        fs::write(
            &path,
            format!(
                "{{\"timestamp\":\"2026-04-07T10:00:00Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"session-a\",\"cwd\":\"/repo\"}}}}\n\
                 {{\"timestamp\":\"2026-04-07T10:00:01Z\",\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{{\"text\":\"{long_text}\"}}]}}}}\n"
            ),
        )
        .expect("initial rollout should be written");

        let context = SessionReadContext {
            task_id: "task".to_string(),
            run_id: "run".to_string(),
            session_ref: "2026/04/07/rollout-2026-04-07T10-00-00-session-a.jsonl".to_string(),
            session_id: "session-a".to_string(),
            default_parent_thread_id: "root".to_string(),
        };
        let loaded = SessionReader::load_all(&path, &context).expect("session should load");

        fs::write(
            &path,
            concat!(
                "{\"timestamp\":\"2026-04-07T10:10:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"session-a\",\"cwd\":\"/repo\"}}\n",
                "{\"timestamp\":\"2026-04-07T10:10:01Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"text\":\"after truncate\"}]}}\n"
            ),
        )
        .expect("session should be rewritten");

        let truncated = SessionReader::tail(&path, &context, &loaded.tail_cursor)
            .expect("truncate tail should succeed");
        assert!(truncated.reset);
        assert!(truncated
            .events
            .iter()
            .any(|event| event.payload["text"] == "after truncate"));

        let rotated_path = path.with_file_name("rotated-old.jsonl");
        fs::rename(&path, &rotated_path).expect("old file should be renamed");
        write_rollout_file(
            &path,
            "session-a",
            &[
                r#"{"timestamp":"2026-04-07T10:20:01Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"text":"after rotate"}]}}"#,
            ],
        );

        let rotated = SessionReader::tail(&path, &context, &truncated.next_cursor)
            .expect("rotate tail should succeed");
        assert!(rotated.reset);
        assert!(rotated
            .events
            .iter()
            .any(|event| event.payload["text"] == "after rotate"));
    }

    #[test]
    fn loader_rejects_traversal_session_ref() {
        let tmp = tempdir().expect("tmpdir should exist");
        let home = tmp.path().join(".codex");
        let sessions = home.join("sessions").join("2026").join("04").join("07");
        fs::create_dir_all(&sessions).expect("sessions dir should exist");
        write_rollout_file(
            &sessions.join("rollout-2026-04-07T10-00-00-session-a.jsonl"),
            "session-a",
            &[],
        );

        let home = ResolvedCodexHome::initialize(Some(home)).expect("codex home should resolve");
        let loader = SessionLoader::new(home);
        assert!(loader.load_session("../secret.txt", 120).is_err());
    }
}
