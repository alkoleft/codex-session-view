use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};
use crate::events::operation_stream::{
    classify_event as classify_operation_event,
    normalize_scope_value as normalize_operation_scope_value, operation_id as operation_stream_id,
    operation_scope as operation_stream_scope, project_operation_stream, EventClassification,
    OperationKey as OperationStreamKey, OperationKind as OperationStreamKind,
    OperationProjection as OperationStreamProjection, OperationSnapshot as OperationStreamSnapshot,
};
use crate::events::projector::{
    categorize_event, load_event_records, summarize_event_full, AgentSnapshot, EventProjector,
    EventSummaryCategory,
};
use crate::events::readers::{JsonOutputEventReader, RunEventContext};
use crate::events::record::EventRecord;
use crate::events::replay::ReplayedRunStream;
use crate::events::types::{
    AGENT_ABORTED, AGENT_COMPLETED, AGENT_FAILED, AGENT_META, AGENT_REASONING, AGENT_SESSION,
    AGENT_SESSION_FOREIGN, AGENT_STARTED, COLLAB_CLOSE_AGENT, COLLAB_RESUME_AGENT,
    COLLAB_SEND_INPUT, COLLAB_SPAWN_AGENT, COLLAB_WAIT, CONTEXT_COMPACTED,
    CONTEXT_COMPACTED_DUPLICATE, INFO_TOKENS, MCP_CALL, MCP_RESULT, MESSAGE_COMMENTARY,
    MESSAGE_PLAN, MESSAGE_USER, PATCH_APPLY, PATCH_APPLY_DUPLICATE, RUNTIME_CONTEXT, SHELL_CALL,
    SHELL_RESULT, STDERR_LINE, STDIN_WRITE, TASK_COMPLETED, TASK_STARTED, THREAD_STARTED,
    TODO_UPDATE, TOOL_CALL, TOOL_RESULT, USER_INPUT_REQUEST, WEB_OPEN, WEB_SEARCH,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanStepEntry {
    pub step: String,
    pub status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnAgentEntry {
    pub prompt: Option<String>,
    pub requested_agent_type: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub receiver_thread_id: Option<String>,
    pub receiver_nickname: Option<String>,
    pub receiver_role: Option<String>,
    pub receiver_status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserInputOptionEntry {
    pub label: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserInputQuestionEntry {
    pub header: Option<String>,
    pub id: Option<String>,
    pub question: Option<String>,
    pub options: Vec<UserInputOptionEntry>,
    pub answers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserInputAnswerEntry {
    pub id: String,
    pub answers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserInputRequestEntry {
    pub questions: Vec<UserInputQuestionEntry>,
    pub extra_answers: Vec<UserInputAnswerEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchApplyChangeEntry {
    pub path: String,
    pub change_type: Option<String>,
    pub unified_diff: Option<String>,
    pub move_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellParsedCommandEntry {
    pub kind: Option<String>,
    pub command: Option<String>,
    pub query: Option<String>,
    pub name: Option<String>,
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEntry {
    pub event_id: String,
    pub parent_event_id: Option<String>,
    pub seq: u64,
    pub ts: String,
    pub actor_type: Option<String>,
    pub thread_id: Option<String>,
    pub subagent_nickname: Option<String>,
    pub event_type: String,
    pub raw_type: String,
    pub parse_status: String,
    pub duplicate_of: Option<String>,
    pub summary: String,
    pub category: EventSummaryCategory,
    pub meta_type: Option<String>,
    pub turn_id: Option<String>,
    pub model_context_window: Option<String>,
    pub collaboration_mode_kind: Option<String>,
    pub last_agent_message: Option<String>,
    pub message_role: Option<String>,
    pub message_direction: Option<String>,
    pub message_text: Option<String>,
    pub plan_message_text: Option<String>,
    pub plan_explanation: Option<String>,
    pub plan_steps: Vec<PlanStepEntry>,
    pub spawn_agent: Option<SpawnAgentEntry>,
    pub user_input_request: Option<UserInputRequestEntry>,
    pub runtime_context_pairs: Vec<(String, String)>,
    pub patch_apply_status: Option<String>,
    pub patch_apply_input: Option<String>,
    pub patch_apply_changes: Vec<PatchApplyChangeEntry>,
    pub tool_name: Option<String>,
    pub receiver_thread_ids: Vec<String>,
    pub operation_id: Option<String>,
    #[serde(default)]
    pub operation_kind: Option<String>,
    #[serde(default)]
    pub operation_root_event_id: Option<String>,
    #[serde(default)]
    pub operation_revision: Option<u64>,
    #[serde(default)]
    pub operation_started_seq: Option<u64>,
    #[serde(default)]
    pub operation_terminal_seq: Option<u64>,
    #[serde(default)]
    pub operation_last_seq: Option<u64>,
    #[serde(default)]
    pub operation_is_preferred_terminal: bool,
    pub phase: Option<String>,
    pub aggregated_output: Option<String>,
    pub output_value: Option<Value>,
    pub shell_command: Option<String>,
    pub shell_exit_code: Option<i32>,
    #[serde(default)]
    pub shell_workdir: Option<String>,
    #[serde(default)]
    pub shell_cwd: Option<String>,
    #[serde(default)]
    pub shell_yield_time_ms: Option<u64>,
    #[serde(default)]
    pub shell_max_output_tokens: Option<u64>,
    #[serde(default)]
    pub shell_login: Option<bool>,
    #[serde(default)]
    pub shell_tty: Option<bool>,
    #[serde(default)]
    pub shell_binary: Option<String>,
    #[serde(default)]
    pub shell_process_id: Option<String>,
    #[serde(default)]
    pub shell_source: Option<String>,
    #[serde(default)]
    pub shell_duration_ns: Option<u64>,
    #[serde(default)]
    pub shell_original_token_count: Option<u64>,
    #[serde(default)]
    pub shell_formatted_output: Option<String>,
    #[serde(default)]
    pub shell_parsed_commands: Vec<ShellParsedCommandEntry>,
    pub summary_pairs: Vec<(String, String)>,
    pub input_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub reasoning_output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventNode {
    pub event: EventEntry,
    pub children: Vec<TimelineItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadNode {
    pub thread_id: String,
    pub parent_event_id: Option<String>,
    pub is_root: bool,
    pub status: Option<String>,
    pub role: Option<String>,
    pub nickname: Option<String>,
    pub cwd: Option<String>,
    pub event_count: usize,
    pub child_thread_count: usize,
    pub items: Vec<TimelineItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimelineItem {
    Event(EventNode),
    Thread(ThreadNode),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventTree {
    pub source_path: PathBuf,
    pub task_id: String,
    pub run_id: String,
    pub event_count: usize,
    pub thread_count: usize,
    pub root_thread_id: String,
    pub roots: Vec<ThreadNode>,
    pub orphan_events: Vec<EventEntry>,
    pub standalone_startup_metadata: Option<Map<String, Value>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StandaloneRolloutLoad {
    pub events: Vec<EventRecord>,
    pub startup_metadata: Map<String, Value>,
}

#[derive(Debug, Clone)]
struct FlatThreadNode {
    thread_id: String,
    is_root: bool,
    status: Option<String>,
    role: Option<String>,
    nickname: Option<String>,
    cwd: Option<String>,
    events: Vec<EventEntry>,
    children: Vec<FlatThreadNode>,
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn load_records_any(path: &Path) -> AppResult<Vec<EventRecord>> {
    let content = fs::read_to_string(path)?;
    let trimmed = content.trim_start();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    if trimmed.starts_with('[') {
        if let Ok(events) = serde_json::from_str::<Vec<EventRecord>>(&content) {
            return Ok(events);
        }
        if let Some(normalized) = strip_trailing_array_comma(&content) {
            if let Ok(events) = serde_json::from_str::<Vec<EventRecord>>(&normalized) {
                return Ok(events);
            }
        }
        return Err(AppError::Runner(format!(
            "не удалось разобрать JSON-массив событий из {}",
            path.display()
        )));
    }
    load_event_records(path).map_err(AppError::from)
}

pub fn load_records_from_run_input(input_path: &Path) -> AppResult<(PathBuf, Vec<EventRecord>)> {
    let run_dir = resolve_run_dir(input_path).ok_or_else(|| {
        AppError::Runner(format!(
            "ожидалась run-директория или её raw-артефакт, но путь не распознан: {}",
            input_path.display()
        ))
    })?;
    let replayed = ReplayedRunStream::replay_run_dir(&run_dir)?;
    Ok((run_dir, replayed.events))
}

pub fn is_run_input(path: &Path) -> bool {
    resolve_run_dir(path).is_some()
}

pub fn is_rollout_jsonl_family(path: &Path) -> bool {
    path.file_name()
        .and_then(|value| value.to_str())
        .map(|file_name| file_name.starts_with("rollout-") && file_name.ends_with(".jsonl"))
        .unwrap_or(false)
}

pub fn validate_standalone_rollout_root(path: &Path) -> AppResult<String> {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| {
            AppError::Runner(format!(
                "standalone rollout: некорректное имя файла {}",
                path.display()
            ))
        })?;
    let session_id = extract_standalone_rollout_session_id(file_name, path)?;

    let file = fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);
    let mut first_line = String::new();
    if reader.read_line(&mut first_line)? == 0 {
        return Err(AppError::Runner(format!(
            "standalone rollout: файл пустой, ожидалась первая строка session_meta: {}",
            path.display()
        )));
    }
    let first_line = first_line.trim_end_matches(['\r', '\n']);
    let parsed: Value = serde_json::from_str(first_line).map_err(|err| {
        AppError::Runner(format!(
            "standalone rollout: первая строка должна быть JSON session_meta ({}): {err}",
            path.display()
        ))
    })?;
    if parsed.get("type").and_then(Value::as_str) != Some("session_meta") {
        return Err(AppError::Runner(format!(
            "standalone rollout: первая строка должна иметь type=session_meta ({})",
            path.display()
        )));
    }

    let meta_id = parsed
        .get("payload")
        .and_then(Value::as_object)
        .and_then(|payload| payload.get("id"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            AppError::Runner(format!(
                "standalone rollout: session_meta.payload.id отсутствует или не строка ({})",
                path.display()
            ))
        })?;
    if meta_id != session_id {
        return Err(AppError::Runner(format!(
            "standalone rollout: session_meta.payload.id ({meta_id}) не совпадает с session-id из имени файла ({session_id}) в {}",
            path.display()
        )));
    }

    Ok(session_id)
}

fn extract_standalone_rollout_session_id(file_name: &str, path: &Path) -> AppResult<String> {
    match parse_standalone_rollout_session_id_from_file_name(file_name) {
        Some(session_id) => Ok(session_id.to_string()),
        None if !file_name.starts_with("rollout-") || !file_name.ends_with(".jsonl") => {
            Err(AppError::Runner(format!(
                "standalone rollout: ожидался файл rollout-*.jsonl, получено {}",
                path.display()
            )))
        }
        None => Err(AppError::Runner(format!(
            "standalone rollout: ожидался формат rollout-<timestamp>-<session-id>.jsonl, получено {}",
            path.display()
        ))),
    }
}

fn parse_standalone_rollout_session_id_from_file_name(file_name: &str) -> Option<&str> {
    const PREFIX: &str = "rollout-";
    const SUFFIX: &str = ".jsonl";
    const TIMESTAMP_LEN: usize = 19;

    let remainder = file_name.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?;
    if remainder.len() <= TIMESTAMP_LEN {
        return None;
    }

    let (timestamp, suffix) = remainder.split_at(TIMESTAMP_LEN);
    if !is_rollout_timestamp_prefix(timestamp) || !suffix.starts_with('-') {
        return None;
    }

    let session_id = suffix[1..].trim();
    if session_id.is_empty() {
        return None;
    }

    Some(session_id)
}

fn is_rollout_timestamp_prefix(value: &str) -> bool {
    value.len() == 19
        && value.bytes().enumerate().all(|(idx, byte)| match idx {
            4 | 7 | 13 | 16 => byte == b'-',
            10 => byte == b'T',
            _ => byte.is_ascii_digit(),
        })
}

pub fn build_event_tree(path: &Path, events: &[EventRecord], _text_limit: usize) -> EventTree {
    build_event_tree_with_standalone_startup_metadata(path, events, None, _text_limit)
}

pub fn build_event_tree_with_standalone_startup_metadata(
    path: &Path,
    events: &[EventRecord],
    standalone_startup_metadata: Option<Map<String, Value>>,
    _text_limit: usize,
) -> EventTree {
    let task_id = events
        .first()
        .map(|event| event.task_id.as_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("-")
        .to_string();
    let run_id = events
        .first()
        .and_then(|event| normalize_operation_scope_value(event.run_id.as_str()))
        .unwrap_or_else(|| "-".to_string());

    let mut projector = EventProjector::new(events.len().max(1), 8);
    projector.apply_events(events.iter());
    let operation_projection = project_operation_stream(events);
    let operation_snapshots = index_operation_snapshots(&operation_projection);
    let normalized_agents = normalize_agent_snapshots(&projector.snapshot.agents);

    let root_thread_id = projector
        .snapshot
        .root_thread_id
        .as_deref()
        .and_then(|value| normalize_thread_identifier(Some(value)))
        .or_else(|| events.iter().find_map(|event| event_thread_id(event, None)))
        .unwrap_or_else(|| "root".to_string());

    let mut events_by_thread: BTreeMap<String, Vec<EventEntry>> = BTreeMap::new();
    let mut orphan_events = Vec::new();
    for event in events {
        let entry = format_event_entry(
            event,
            &normalized_agents,
            root_thread_id.as_str(),
            &operation_snapshots,
        );
        if let Some(thread_id) = event_thread_id(event, Some(root_thread_id.as_str())) {
            events_by_thread.entry(thread_id).or_default().push(entry);
        } else {
            orphan_events.push(entry);
        }
    }
    for events in events_by_thread.values_mut() {
        *events = merge_plan_message_duplicates(std::mem::take(events));
    }
    orphan_events = merge_plan_message_duplicates(orphan_events);

    let mut all_thread_ids = BTreeSet::new();
    all_thread_ids.insert(root_thread_id.clone());
    all_thread_ids.extend(normalized_agents.keys().cloned());
    all_thread_ids.extend(events_by_thread.keys().cloned());

    let first_seq_by_thread = first_seq_by_thread(&all_thread_ids, &events_by_thread);
    let (mut root_threads, mut children_by_parent) = build_thread_index(
        &all_thread_ids,
        &normalized_agents,
        &first_seq_by_thread,
        &root_thread_id,
    );

    sort_threads(
        &mut root_threads,
        &first_seq_by_thread,
        Some(root_thread_id.as_str()),
    );
    for children in children_by_parent.values_mut() {
        sort_threads(children, &first_seq_by_thread, None);
    }

    let roots = root_threads
        .into_iter()
        .map(|thread_id| {
            let flat = build_flat_thread_node(
                &thread_id,
                &root_thread_id,
                &normalized_agents,
                &events_by_thread,
                &children_by_parent,
            );
            materialize_thread(flat, None)
        })
        .collect();

    EventTree {
        source_path: path.to_path_buf(),
        task_id,
        run_id,
        event_count: events.len(),
        thread_count: all_thread_ids.len(),
        root_thread_id,
        roots,
        orphan_events,
        standalone_startup_metadata,
    }
}

pub fn load_records_from_standalone_rollout(
    path: &Path,
    session_id: &str,
) -> AppResult<StandaloneRolloutLoad> {
    let root_canonical = fs::canonicalize(path)?;
    let mut visited = BTreeSet::new();
    let mut events = Vec::new();
    let startup_metadata = load_records_from_standalone_rollout_recursive(
        path,
        &root_canonical,
        session_id,
        &mut visited,
        &mut events,
    )?;
    for (index, event) in events.iter_mut().enumerate() {
        event.seq = (index + 1) as u64;
    }
    Ok(StandaloneRolloutLoad {
        events,
        startup_metadata,
    })
}

fn load_records_from_single_standalone_rollout(
    path: &Path,
    session_id: &str,
) -> AppResult<StandaloneRolloutLoad> {
    let file = fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);

    let synthetic_context = RunEventContext {
        task_id: "standalone-rollout".to_string(),
        run_id: "standalone-rollout".to_string(),
    };
    let mut parser = JsonOutputEventReader::new(synthetic_context, None);
    let mut seq = 0u64;
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let mut current_parent_thread_id = "standalone-root".to_string();
    let mut startup_metadata: Option<Map<String, Value>> = None;
    let mut events = Vec::new();

    for (line_idx, line) in reader.lines().enumerate() {
        let raw_line = line?;
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let parsed = serde_json::from_str::<Value>(trimmed).map_err(|err| {
            AppError::Runner(format!(
                "standalone rollout: строка {} должна быть JSON-object ({}): {err}",
                line_idx + 1,
                path.display()
            ))
        })?;
        let parsed = parsed.as_object().ok_or_else(|| {
            AppError::Runner(format!(
                "standalone rollout: строка {} должна быть JSON-object ({})",
                line_idx + 1,
                path.display()
            ))
        })?;

        if line_idx == 0 {
            if parsed.get("type").and_then(Value::as_str) != Some("session_meta") {
                return Err(AppError::Runner(format!(
                    "standalone rollout: первая строка должна иметь type=session_meta ({})",
                    path.display()
                )));
            }
            let startup = parsed
                .get("payload")
                .and_then(Value::as_object)
                .ok_or_else(|| {
                    AppError::Runner(format!(
                        "standalone rollout: первая строка должна содержать payload-object ({})",
                        path.display()
                    ))
                })?;
            let startup_session_id = startup
                .get("id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    AppError::Runner(format!(
                        "standalone rollout: session_meta.payload.id отсутствует или не строка ({})",
                        path.display()
                    ))
                })?;
            if startup_session_id != session_id {
                return Err(AppError::Runner(format!(
                    "standalone rollout: session_meta.payload.id ({startup_session_id}) не совпадает с ожидаемым session-id ({session_id}) в {}",
                    path.display()
                )));
            }
            startup_metadata = Some(startup.clone());
            let canonical_parent_thread_id = startup
                .get("source")
                .and_then(Value::as_object)
                .and_then(|source| source.get("subagent"))
                .and_then(Value::as_object)
                .and_then(|subagent| subagent.get("thread_spawn"))
                .and_then(Value::as_object)
                .and_then(|thread_spawn| thread_spawn.get("parent_thread_id"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            let flat_parent_thread_id = startup
                .get("parent_thread_id")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty());
            if let Some(parent_thread_id) = canonical_parent_thread_id.or(flat_parent_thread_id) {
                current_parent_thread_id = parent_thread_id.to_string();
            }
        }

        seq += 1;
        if let Some(event) = parser.parse_subagent_session_payload(
            seq,
            parsed,
            path,
            &current_parent_thread_id,
            session_id,
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        ) {
            events.push(event.to_record());
        }
    }

    let startup_metadata = startup_metadata.ok_or_else(|| {
        AppError::Runner(format!(
            "standalone rollout: отсутствует startup metadata в {}",
            path.display()
        ))
    })?;
    Ok(StandaloneRolloutLoad {
        events,
        startup_metadata,
    })
}

fn load_records_from_standalone_rollout_recursive(
    path: &Path,
    canonical_path: &Path,
    session_id: &str,
    visited: &mut BTreeSet<PathBuf>,
    out_events: &mut Vec<EventRecord>,
) -> AppResult<Map<String, Value>> {
    if !visited.insert(canonical_path.to_path_buf()) {
        return Ok(Map::new());
    }

    let loaded = load_records_from_single_standalone_rollout(path, session_id)?;
    let startup_metadata = loaded.startup_metadata.clone();
    let parent_events = loaded.events;
    out_events.extend(parent_events.iter().cloned());

    let child_ids = collect_receiver_thread_ids_from_events(&parent_events);
    for child_id in child_ids {
        let Some((child_path, child_canonical)) = resolve_child_rollout_path(path, &child_id)?
        else {
            continue;
        };
        let _ = load_records_from_standalone_rollout_recursive(
            &child_path,
            &child_canonical,
            &child_id,
            visited,
            out_events,
        )?;
    }

    Ok(startup_metadata)
}

fn collect_receiver_thread_ids_from_events(events: &[EventRecord]) -> Vec<String> {
    let mut ids = BTreeSet::new();
    for event in events {
        if let Some(payload) = event.payload.as_object() {
            collect_receiver_thread_ids_from_payload(payload, &mut ids);
        }
    }
    ids.into_iter().collect()
}

fn collect_receiver_thread_ids_from_payload(
    payload: &serde_json::Map<String, Value>,
    out: &mut BTreeSet<String>,
) {
    collect_receiver_thread_ids_from_value(&Value::Object(payload.clone()), out);
}

fn collect_receiver_thread_ids_from_value(value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            if let Some(Value::Array(ids)) = map.get("receiver_thread_ids") {
                for thread_id in ids
                    .iter()
                    .filter_map(Value::as_str)
                    .filter_map(|thread_id| normalize_thread_identifier(Some(thread_id)))
                {
                    out.insert(thread_id);
                }
            }
            for nested in map.values() {
                collect_receiver_thread_ids_from_value(nested, out);
            }
        }
        Value::Array(items) => {
            for nested in items {
                collect_receiver_thread_ids_from_value(nested, out);
            }
        }
        Value::String(text) => {
            if let Ok(parsed) = serde_json::from_str::<Value>(text) {
                collect_receiver_thread_ids_from_value(&parsed, out);
            }
        }
        _ => {}
    }
}

fn resolve_child_rollout_path(
    parent_path: &Path,
    child_session_id: &str,
) -> AppResult<Option<(PathBuf, PathBuf)>> {
    let parent_dir = parent_path.parent().ok_or_else(|| {
        AppError::Runner(format!(
            "standalone rollout: у файла нет родительской директории {}",
            parent_path.display()
        ))
    })?;
    let suffix = format!("-{child_session_id}.jsonl");
    let mut matches = Vec::new();
    for entry in fs::read_dir(parent_dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_file() {
            continue;
        }
        let file_name = entry.file_name();
        let Some(basename) = file_name.to_str() else {
            continue;
        };
        if basename.starts_with("rollout-") && basename.ends_with(&suffix) {
            let path = entry.path();
            let canonical = fs::canonicalize(&path)?;
            matches.push((basename.to_string(), path, canonical));
        }
    }
    matches.sort_by(|left, right| left.0.cmp(&right.0));

    if matches.len() > 1 {
        return Err(AppError::Runner(format!(
            "standalone rollout: ambiguous child match for {child_session_id} in {}",
            parent_dir.display()
        )));
    }
    Ok(matches
        .into_iter()
        .next()
        .map(|(_, path, canonical)| (path, canonical)))
}

#[allow(dead_code)]
pub fn format_event_line(entry: &EventEntry) -> String {
    format!(
        "#{:04} {} raw={} parse={} | {}",
        entry.seq, entry.event_type, entry.raw_type, entry.parse_status, entry.summary
    )
}

pub fn timeline_item_first_seq(item: &TimelineItem) -> u64 {
    match item {
        TimelineItem::Event(node) => node.event.seq,
        TimelineItem::Thread(thread) => thread_first_seq(thread),
    }
}

pub fn thread_first_seq(thread: &ThreadNode) -> u64 {
    thread
        .items
        .iter()
        .map(timeline_item_first_seq)
        .min()
        .unwrap_or(u64::MAX)
}

#[cfg_attr(not(test), allow(dead_code))]
fn strip_trailing_array_comma(content: &str) -> Option<String> {
    let trimmed_end = content.trim_end();
    if !trimmed_end.ends_with(']') {
        return None;
    }
    let bracket_pos = trimmed_end.rfind(']')?;
    let before = &trimmed_end[..bracket_pos];
    let before_trimmed = before.trim_end();
    if !before_trimmed.ends_with(',') {
        return None;
    }
    let comma_pos = before_trimmed.len().checked_sub(1)?;
    let mut normalized = String::with_capacity(trimmed_end.len());
    normalized.push_str(&before_trimmed[..comma_pos]);
    normalized.push_str(&trimmed_end[comma_pos + 1..]);
    Some(normalized)
}

fn resolve_run_dir(input_path: &Path) -> Option<PathBuf> {
    if is_run_dir(input_path) {
        return Some(input_path.to_path_buf());
    }

    let parent = input_path.parent()?;
    let file_name = input_path.file_name().and_then(|value| value.to_str())?;

    if matches!(
        file_name,
        "stdout.jsonl" | "stderr.log" | "events.jsonl" | "events.json" | "summary.json"
    ) && is_run_dir(parent)
    {
        return Some(parent.to_path_buf());
    }

    let container_name = parent.file_name().and_then(|value| value.to_str())?;
    if matches!(
        container_name,
        "subagents" | "raw_unparsed" | "problem_examples"
    ) {
        let run_dir = parent.parent()?;
        if is_run_dir(run_dir) {
            return Some(run_dir.to_path_buf());
        }
    }

    None
}

fn is_run_dir(path: &Path) -> bool {
    path.is_dir() && path.join("stdout.jsonl").is_file()
}

fn build_flat_thread_node(
    thread_id: &str,
    root_thread_id: &str,
    agents: &HashMap<String, AgentSnapshot>,
    events_by_thread: &BTreeMap<String, Vec<EventEntry>>,
    children_by_parent: &HashMap<String, Vec<String>>,
) -> FlatThreadNode {
    let agent = agents.get(thread_id);
    let children = children_by_parent
        .get(thread_id)
        .into_iter()
        .flatten()
        .map(|child_thread_id| {
            build_flat_thread_node(
                child_thread_id,
                root_thread_id,
                agents,
                events_by_thread,
                children_by_parent,
            )
        })
        .collect();

    FlatThreadNode {
        thread_id: thread_id.to_string(),
        is_root: thread_id == root_thread_id,
        status: agent
            .map(|agent| agent.status.clone())
            .filter(|value| !value.is_empty()),
        role: agent
            .and_then(|agent| agent.role.clone())
            .filter(|value| !value.is_empty()),
        nickname: agent
            .and_then(|agent| agent.nickname.clone())
            .filter(|value| !value.is_empty()),
        cwd: agent
            .and_then(|agent| agent.cwd.clone())
            .filter(|value| !value.is_empty()),
        events: events_by_thread.get(thread_id).cloned().unwrap_or_default(),
        children,
    }
}

fn materialize_thread(flat: FlatThreadNode, parent_event_id: Option<String>) -> ThreadNode {
    let mut events = flat.events;
    assign_operation_parent_ids(&mut events);
    assign_follow_up_parent_ids(&mut events);

    let mut child_threads: Vec<ThreadNode> = flat
        .children
        .into_iter()
        .map(|child| {
            let child_event_anchor = anchor_event_id_for_child(&events, &child);
            materialize_thread(child, child_event_anchor)
        })
        .collect();
    child_threads.sort_by_key(thread_first_seq);

    let mut child_items_by_parent: BTreeMap<Option<String>, Vec<TimelineItem>> = BTreeMap::new();
    for child_thread in child_threads {
        child_items_by_parent
            .entry(child_thread.parent_event_id.clone())
            .or_default()
            .push(TimelineItem::Thread(child_thread));
    }

    let mut items = build_timeline_items(&events, &child_items_by_parent, None);
    items.sort_by_key(timeline_item_first_seq);

    ThreadNode {
        thread_id: flat.thread_id,
        parent_event_id,
        is_root: flat.is_root,
        status: flat.status,
        role: flat.role,
        nickname: flat.nickname,
        cwd: flat.cwd,
        event_count: events.len(),
        child_thread_count: child_items_by_parent.values().map(Vec::len).sum::<usize>(),
        items,
    }
}

fn assign_operation_parent_ids(events: &mut [EventEntry]) {
    for event in events.iter_mut() {
        if let Some(parent_event_id) = event
            .operation_root_event_id
            .clone()
            .filter(|root_event_id| root_event_id != &event.event_id)
        {
            event.parent_event_id = Some(parent_event_id);
        }
    }
}

fn build_timeline_items(
    events: &[EventEntry],
    child_items_by_parent: &BTreeMap<Option<String>, Vec<TimelineItem>>,
    parent_event_id: Option<&str>,
) -> Vec<TimelineItem> {
    let mut items = Vec::new();

    for event in events
        .iter()
        .filter(|event| event.parent_event_id.as_deref() == parent_event_id)
    {
        let mut children =
            build_timeline_items(events, child_items_by_parent, Some(event.event_id.as_str()));
        if let Some(extra_children) = child_items_by_parent.get(&Some(event.event_id.clone())) {
            children.extend(extra_children.clone());
        }
        children.sort_by_key(timeline_item_first_seq);
        items.push(TimelineItem::Event(EventNode {
            event: event.clone(),
            children,
        }));
    }

    if parent_event_id.is_none() {
        if let Some(root_children) = child_items_by_parent.get(&None) {
            items.extend(root_children.clone());
        }
    }

    items
}

fn assign_follow_up_parent_ids(events: &mut [EventEntry]) {
    for index in 0..events.len() {
        if !matches!(
            events[index].event_type.as_str(),
            TOOL_RESULT | SHELL_RESULT
        ) {
            continue;
        }
        let parent_event_id = events[index].event_id.clone();
        for next_index in (index + 1)..events.len() {
            if is_barrier_event(&events[next_index]) {
                break;
            }
            if is_follow_up_event(&events[next_index]) {
                events[next_index].parent_event_id = Some(parent_event_id.clone());
            }
        }
    }
}

fn merge_plan_message_duplicates(events: Vec<EventEntry>) -> Vec<EventEntry> {
    let mut merged = Vec::with_capacity(events.len());
    let mut index = 0usize;

    while index < events.len() {
        let current = &events[index];
        if is_event_msg_plan_message(current) {
            if let Some(next) = events.get(index + 1) {
                if is_response_plan_message(next)
                    && plan_message_text_matches(
                        current.plan_message_text.as_deref(),
                        next.plan_message_text.as_deref(),
                    )
                {
                    merged.push(merge_plan_message_pair(current, next));
                    index += 2;
                    continue;
                }
            }
            merged.push(convert_to_plan_message_entry(current, None));
            index += 1;
            continue;
        }

        if is_response_plan_message(current) {
            merged.push(convert_to_plan_message_entry(current, None));
            index += 1;
            continue;
        }

        merged.push(current.clone());
        index += 1;
    }

    merged
}

fn is_event_msg_plan_message(event: &EventEntry) -> bool {
    event.event_type == TODO_UPDATE
        && event.raw_type == "event_msg"
        && event.plan_message_text.is_some()
        && event.duplicate_of.as_deref() == Some("response_item.message")
}

fn is_response_plan_message(event: &EventEntry) -> bool {
    event.raw_type == "response_item"
        && event.event_type.starts_with("message.")
        && event.plan_message_text.is_some()
}

fn plan_message_text_matches(left: Option<&str>, right: Option<&str>) -> bool {
    match (left, right) {
        (Some(left), Some(right)) => {
            let left = left.trim();
            let right = right.trim();
            !left.is_empty() && left == right
        }
        _ => false,
    }
}

fn merge_plan_message_pair(event_msg: &EventEntry, response_item: &EventEntry) -> EventEntry {
    convert_to_plan_message_entry(event_msg, Some(response_item))
}

fn convert_to_plan_message_entry(
    source: &EventEntry,
    response_item: Option<&EventEntry>,
) -> EventEntry {
    let mut entry = source.clone();
    let text = source
        .plan_message_text
        .as_ref()
        .or_else(|| response_item.and_then(|event| event.plan_message_text.as_ref()))
        .cloned();
    entry.event_type = MESSAGE_PLAN.to_string();
    entry.summary = text.clone().unwrap_or_default();
    entry.category = EventSummaryCategory::Assistant;
    entry.message_text = text;
    entry.message_role = response_item
        .and_then(|event| event.message_role.clone())
        .or_else(|| entry.message_role.clone())
        .or_else(|| Some("assistant".to_string()));
    entry.message_direction = response_item
        .and_then(|event| event.message_direction.clone())
        .or_else(|| entry.message_direction.clone())
        .or_else(|| Some("output_text".to_string()));
    entry.phase = response_item
        .and_then(|event| event.phase.clone())
        .or_else(|| entry.phase.clone());
    entry.plan_message_text = entry.message_text.clone();
    entry.plan_explanation = None;
    entry.plan_steps.clear();
    entry.tool_name = None;
    entry.operation_id = None;
    entry.duplicate_of = None;
    entry
}

fn anchor_event_id_for_child(events: &[EventEntry], child: &FlatThreadNode) -> Option<String> {
    let child_first_seq = flat_thread_first_seq(child);
    events
        .iter()
        .rev()
        .find(|event| {
            event.seq < child_first_seq
                && is_spawn_agent_event(event)
                && event.phase.as_deref() == Some("completed")
                && event
                    .receiver_thread_ids
                    .iter()
                    .any(|thread_id| thread_id == &child.thread_id)
        })
        .map(root_operation_event_id)
        .or_else(|| {
            events
                .iter()
                .rev()
                .find(|event| {
                    event.seq < child_first_seq
                        && is_spawn_agent_event(event)
                        && event
                            .receiver_thread_ids
                            .iter()
                            .any(|thread_id| thread_id == &child.thread_id)
                })
                .map(root_operation_event_id)
        })
        .or_else(|| {
            events
                .iter()
                .rev()
                .find(|event| {
                    event.seq < child_first_seq
                        && matches!(event.event_type.as_str(), TOOL_RESULT | SHELL_RESULT)
                })
                .map(|event| event.event_id.clone())
        })
        .or_else(|| {
            events
                .iter()
                .rev()
                .find(|event| event.seq < child_first_seq)
                .map(|event| event.event_id.clone())
        })
}

fn root_operation_event_id(event: &EventEntry) -> String {
    event
        .parent_event_id
        .clone()
        .unwrap_or_else(|| event.event_id.clone())
}

fn is_barrier_event(event: &EventEntry) -> bool {
    matches!(
        event.event_type.as_str(),
        TOOL_CALL
            | TOOL_RESULT
            | SHELL_CALL
            | SHELL_RESULT
            | MCP_CALL
            | MCP_RESULT
            | STDIN_WRITE
            | WEB_SEARCH
            | WEB_OPEN
            | TODO_UPDATE
            | USER_INPUT_REQUEST
            | PATCH_APPLY
            | COLLAB_SPAWN_AGENT
            | COLLAB_SEND_INPUT
            | COLLAB_WAIT
            | COLLAB_CLOSE_AGENT
            | COLLAB_RESUME_AGENT
            | AGENT_STARTED
            | AGENT_COMPLETED
            | AGENT_FAILED
            | AGENT_ABORTED
            | TASK_STARTED
            | TASK_COMPLETED
            | RUNTIME_CONTEXT
            | CONTEXT_COMPACTED
            | CONTEXT_COMPACTED_DUPLICATE
            | MESSAGE_USER
            | MESSAGE_COMMENTARY
            | THREAD_STARTED
            | AGENT_SESSION
            | AGENT_SESSION_FOREIGN
            | PATCH_APPLY_DUPLICATE
    )
}

fn is_follow_up_event(event: &EventEntry) -> bool {
    if matches!(event.event_type.as_str(), AGENT_REASONING | STDERR_LINE) {
        return true;
    }
    if event.event_type.starts_with("message.")
        && event.event_type != MESSAGE_USER
        && event.phase.as_deref() == Some("commentary")
    {
        return true;
    }

    matches!(event.event_type.as_str(), AGENT_META)
        && matches!(
            event.meta_type.as_deref(),
            Some("message" | "task_complete" | "user_message")
        )
}

fn is_spawn_agent_event(event: &EventEntry) -> bool {
    event.event_type == COLLAB_SPAWN_AGENT || event.tool_name.as_deref() == Some("spawn_agent")
}

fn flat_thread_first_seq(thread: &FlatThreadNode) -> u64 {
    let own_first = thread
        .events
        .first()
        .map(|event| event.seq)
        .unwrap_or(u64::MAX);
    let child_first = thread
        .children
        .iter()
        .map(flat_thread_first_seq)
        .min()
        .unwrap_or(u64::MAX);
    own_first.min(child_first)
}

fn index_operation_snapshots(
    projection: &OperationStreamProjection,
) -> HashMap<OperationStreamKey, OperationStreamSnapshot> {
    projection
        .snapshots
        .iter()
        .cloned()
        .map(|snapshot| (snapshot.key.clone(), snapshot))
        .collect()
}

fn resolve_operation_snapshot<'a>(
    event: &EventRecord,
    operation_snapshots: &'a HashMap<OperationStreamKey, OperationStreamSnapshot>,
) -> Option<&'a OperationStreamSnapshot> {
    let operation_kind = operation_kind_from_event(event)?;
    let operation_id = operation_stream_id(event.payload.as_object())?;
    let scope = operation_stream_scope(event);
    let key = OperationStreamKey {
        kind: operation_kind,
        scope,
        operation_id,
    };
    operation_snapshots.get(&key)
}

fn operation_kind_from_event(event: &EventRecord) -> Option<OperationStreamKind> {
    match classify_operation_event(event) {
        EventClassification::Lifecycle { kind, .. } => Some(kind),
        _ => None,
    }
}

fn operation_root_event_id(run_id: &str, snapshot: &OperationStreamSnapshot) -> String {
    let root_seq = snapshot.started_seq.unwrap_or(snapshot.last_seq);
    format!("{}:{root_seq}", canonical_run_id(run_id))
}

fn format_event_entry(
    event: &EventRecord,
    agents: &HashMap<String, AgentSnapshot>,
    root_thread_id: &str,
    operation_snapshots: &HashMap<OperationStreamKey, OperationStreamSnapshot>,
) -> EventEntry {
    let payload = event.payload.as_object();
    let actor_type = actor_type(event).map(str::to_string);
    let thread_id = event_thread_id(event, Some(root_thread_id));
    let subagent_nickname = if actor_type.as_deref() == Some("subagent") {
        thread_id.as_ref().and_then(|thread_id| {
            agents
                .get(thread_id)
                .and_then(|agent| agent.nickname.clone())
        })
    } else {
        None
    };
    let (plan_explanation, plan_steps) = extract_plan_update_content(&event.event_type, payload);
    let spawn_agent = extract_spawn_agent_entry(&event.event_type, payload);
    let user_input_request = extract_user_input_request_entry(&event.event_type, payload);
    let message_text = payload
        .and_then(|obj| obj.get("text"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let plan_message_text =
        extract_plan_message_text(&event.event_type, payload, message_text.as_deref());
    let operation_snapshot = resolve_operation_snapshot(event, operation_snapshots);
    let operation_root_event_id = operation_snapshot
        .as_ref()
        .map(|snapshot| operation_root_event_id(&event.run_id, snapshot));
    EventEntry {
        event_id: format!("{}:{}", canonical_run_id(&event.run_id), event.seq),
        parent_event_id: None,
        seq: event.seq,
        ts: event.ts.clone(),
        actor_type,
        thread_id,
        subagent_nickname,
        event_type: event.event_type.clone(),
        raw_type: event.raw_type.clone(),
        parse_status: event.parse_status.clone(),
        duplicate_of: payload
            .and_then(|obj| obj.get("duplicate_of"))
            .and_then(Value::as_str)
            .map(str::to_string),
        summary: summarize_event_full(event),
        category: categorize_event(event),
        meta_type: payload
            .and_then(|obj| obj.get("meta_type"))
            .and_then(Value::as_str)
            .map(str::to_string),
        turn_id: extract_payload_scalar_string(payload, "turn_id"),
        model_context_window: extract_payload_scalar_string(payload, "model_context_window"),
        collaboration_mode_kind: extract_payload_scalar_string(payload, "collaboration_mode_kind"),
        last_agent_message: payload
            .and_then(|obj| obj.get("last_agent_message"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        message_role: payload
            .and_then(|obj| obj.get("role"))
            .and_then(Value::as_str)
            .map(str::to_string),
        message_direction: payload
            .and_then(|obj| obj.get("direction"))
            .and_then(Value::as_str)
            .map(str::to_string),
        message_text,
        plan_message_text,
        plan_explanation,
        plan_steps,
        spawn_agent,
        user_input_request,
        runtime_context_pairs: extract_runtime_context_pairs(&event.event_type, payload),
        patch_apply_status: extract_patch_apply_status(&event.event_type, payload),
        patch_apply_input: extract_patch_apply_input(&event.event_type, payload),
        patch_apply_changes: extract_patch_apply_changes(&event.event_type, payload),
        tool_name: payload
            .and_then(|obj| obj.get("tool_name").or_else(|| obj.get("tool")))
            .and_then(Value::as_str)
            .map(str::to_string),
        receiver_thread_ids: {
            let mut ids = BTreeSet::new();
            if let Some(payload) = payload {
                collect_receiver_thread_ids_from_payload(payload, &mut ids);
            }
            ids.into_iter().collect()
        },
        operation_id: operation_stream_id(payload),
        operation_kind: operation_snapshot
            .as_ref()
            .map(|snapshot| snapshot.key.kind.as_str().to_string()),
        operation_root_event_id,
        operation_revision: operation_snapshot
            .as_ref()
            .map(|snapshot| snapshot.revision),
        operation_started_seq: operation_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.started_seq),
        operation_terminal_seq: operation_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.terminal_seq),
        operation_last_seq: operation_snapshot
            .as_ref()
            .map(|snapshot| snapshot.last_seq),
        operation_is_preferred_terminal: operation_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.terminal_seq)
            == Some(event.seq),
        phase: payload
            .and_then(|obj| obj.get("phase"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| match event.raw_type.as_str() {
                "item.started" => Some("started".to_string()),
                "item.updated" => Some("updated".to_string()),
                "item.completed" => Some("completed".to_string()),
                _ => None,
            }),
        aggregated_output: extract_command_aggregated_output(&event.event_type, payload),
        output_value: payload.and_then(|obj| obj.get("output")).cloned(),
        shell_command: extract_shell_command(&event.event_type, payload),
        shell_exit_code: extract_shell_exit_code(&event.event_type, payload),
        shell_workdir: extract_shell_workdir(&event.event_type, payload),
        shell_cwd: extract_shell_cwd(&event.event_type, payload),
        shell_yield_time_ms: extract_shell_yield_time_ms(&event.event_type, payload),
        shell_max_output_tokens: extract_shell_max_output_tokens(&event.event_type, payload),
        shell_login: extract_shell_login(&event.event_type, payload),
        shell_tty: extract_shell_tty(&event.event_type, payload),
        shell_binary: extract_shell_binary(&event.event_type, payload),
        shell_process_id: extract_shell_process_id(&event.event_type, payload),
        shell_source: extract_shell_source(&event.event_type, payload),
        shell_duration_ns: extract_shell_duration_ns(&event.event_type, payload),
        shell_original_token_count: extract_shell_original_token_count(&event.event_type, payload),
        shell_formatted_output: extract_shell_formatted_output(&event.event_type, payload),
        shell_parsed_commands: extract_shell_parsed_commands(&event.event_type, payload),
        summary_pairs: extract_summary_pairs(&event.event_type, payload),
        input_tokens: extract_token_value(&event.event_type, payload, "input_tokens"),
        cached_input_tokens: extract_token_value(&event.event_type, payload, "cached_input_tokens"),
        output_tokens: extract_token_value(&event.event_type, payload, "output_tokens"),
        reasoning_output_tokens: extract_token_value(
            &event.event_type,
            payload,
            "reasoning_output_tokens",
        ),
        total_tokens: extract_token_value(&event.event_type, payload, "total_tokens"),
    }
}

fn extract_payload_scalar_string(
    payload: Option<&serde_json::Map<String, Value>>,
    key: &str,
) -> Option<String> {
    let value = payload.and_then(|obj| obj.get(key))?;
    match value {
        Value::Null => None,
        Value::String(text) => {
            let text = text.trim();
            (!text.is_empty()).then(|| text.to_string())
        }
        Value::Bool(_) | Value::Number(_) => Some(value.to_string()),
        _ => {
            let rendered = render_payload_value(value);
            let rendered = rendered.trim();
            (!rendered.is_empty()).then(|| rendered.to_string())
        }
    }
}

fn extract_summary_pairs(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> Vec<(String, String)> {
    if event_type != INFO_TOKENS {
        return Vec::new();
    }

    let mut pairs = Vec::new();
    for (key, label) in [
        ("input_tokens", "input"),
        ("cached_input_tokens", "cached input"),
        ("output_tokens", "output"),
        ("reasoning_output_tokens", "reasoning output"),
        ("total_tokens", "total"),
    ] {
        let Some(value) = payload.and_then(|obj| obj.get(key)) else {
            continue;
        };
        let rendered = value
            .as_u64()
            .map(format_summary_number)
            .unwrap_or_else(|| value.to_string());
        pairs.push((label.to_string(), rendered));
    }
    pairs
}

fn extract_plan_update_content(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> (Option<String>, Vec<PlanStepEntry>) {
    if event_type != TODO_UPDATE {
        return (None, Vec::new());
    }

    let Some(payload) = payload else {
        return (None, Vec::new());
    };
    let container = payload.get("input").or_else(|| payload.get("output"));
    match container {
        Some(Value::Object(object)) => extract_plan_update_content_from_object(object),
        Some(Value::String(text)) => serde_json::from_str::<Value>(text)
            .ok()
            .and_then(|value| {
                value
                    .as_object()
                    .map(extract_plan_update_content_from_object)
            })
            .unwrap_or_default(),
        _ => (None, Vec::new()),
    }
}

fn extract_plan_update_content_from_object(
    object: &serde_json::Map<String, Value>,
) -> (Option<String>, Vec<PlanStepEntry>) {
    let explanation = object
        .get("explanation")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let plan_steps = object
        .get("plan")
        .and_then(Value::as_array)
        .map(|plan| plan.iter().filter_map(extract_plan_step).collect())
        .unwrap_or_default();
    (explanation, plan_steps)
}

fn extract_plan_step(value: &Value) -> Option<PlanStepEntry> {
    match value {
        Value::Object(object) => {
            let step = object
                .get("step")
                .or_else(|| object.get("text"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .or_else(|| {
                    let rendered = render_payload_value(value);
                    let trimmed = rendered.trim();
                    (!trimmed.is_empty()).then(|| trimmed.to_string())
                })?;
            let status = object
                .get("status")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .or_else(|| {
                    object
                        .get("completed")
                        .and_then(Value::as_bool)
                        .map(|completed| {
                            if completed {
                                "completed".to_string()
                            } else {
                                "pending".to_string()
                            }
                        })
                });
            Some(PlanStepEntry { step, status })
        }
        Value::String(text) => {
            let trimmed = text.trim();
            (!trimmed.is_empty()).then(|| PlanStepEntry {
                step: trimmed.to_string(),
                status: None,
            })
        }
        _ => {
            let rendered = render_payload_value(value);
            let trimmed = rendered.trim();
            (!trimmed.is_empty()).then(|| PlanStepEntry {
                step: trimmed.to_string(),
                status: None,
            })
        }
    }
}

fn extract_plan_message_text(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
    message_text: Option<&str>,
) -> Option<String> {
    match event_type {
        event_type if event_type.starts_with("message.") => message_text
            .and_then(strip_proposed_plan_wrapper)
            .map(str::to_string),
        TODO_UPDATE => payload
            .and_then(|obj| obj.get("output"))
            .and_then(Value::as_object)
            .filter(|output| {
                output.get("item_type").and_then(Value::as_str) == Some("Plan")
                    || output.get("text").and_then(Value::as_str).is_some()
            })
            .and_then(|output| output.get("text"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        _ => None,
    }
}

fn strip_proposed_plan_wrapper(text: &str) -> Option<&str> {
    let trimmed = text.trim();
    let start_tag = "<proposed_plan>";
    let end_tag = "</proposed_plan>";
    let inner = trimmed
        .strip_prefix(start_tag)?
        .strip_suffix(end_tag)?
        .trim();
    (!inner.is_empty()).then_some(inner)
}

fn extract_spawn_agent_entry(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> Option<SpawnAgentEntry> {
    if event_type != COLLAB_SPAWN_AGENT {
        return None;
    }

    let payload = payload?;
    let input = payload.get("input").and_then(Value::as_object);
    let output = payload.get("output").and_then(Value::as_object);
    let prompt = payload
        .get("prompt")
        .and_then(Value::as_str)
        .or_else(|| {
            input
                .and_then(|obj| obj.get("message"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            input
                .and_then(|obj| obj.get("prompt"))
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let requested_agent_type = payload
        .get("requested_agent_type")
        .and_then(Value::as_str)
        .or_else(|| {
            input
                .and_then(|obj| obj.get("agent_type"))
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let mut receiver_thread_id = payload
        .get("new_thread_id")
        .and_then(Value::as_str)
        .or_else(|| first_receiver_thread_id(payload))
        .or_else(|| {
            output
                .and_then(|obj| obj.get("agent_id"))
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let agent_state = if let Some(thread_id) = receiver_thread_id.as_deref() {
        spawn_agent_state(payload, Some(thread_id))
    } else {
        let state = spawn_agent_state(payload, None);
        if let Some((thread_id, _)) = state {
            receiver_thread_id = normalize_thread_identifier(Some(thread_id));
        }
        state
    };
    let state_obj = agent_state.map(|(_, state)| state);
    let receiver_nickname = payload
        .get("new_agent_nickname")
        .and_then(Value::as_str)
        .or_else(|| {
            output
                .and_then(|obj| obj.get("nickname"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            output
                .and_then(|obj| obj.get("agent_nickname"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            state_obj
                .and_then(|obj| obj.get("agent_nickname"))
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let receiver_role = payload
        .get("new_agent_role")
        .and_then(Value::as_str)
        .or_else(|| {
            state_obj
                .and_then(|obj| obj.get("agent_role"))
                .and_then(Value::as_str)
        })
        .or_else(|| requested_agent_type.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let model = payload
        .get("model")
        .and_then(Value::as_str)
        .or_else(|| {
            input
                .and_then(|obj| obj.get("model"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            state_obj
                .and_then(|obj| obj.get("model"))
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let reasoning_effort = payload
        .get("reasoning_effort")
        .and_then(Value::as_str)
        .or_else(|| {
            input
                .and_then(|obj| obj.get("reasoning_effort"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            state_obj
                .and_then(|obj| obj.get("reasoning_effort"))
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let receiver_status = payload
        .get("status")
        .and_then(collab_status_value)
        .or_else(|| {
            state_obj
                .and_then(|obj| obj.get("status"))
                .and_then(collab_status_value)
        });

    if prompt.is_none()
        && requested_agent_type.is_none()
        && model.is_none()
        && reasoning_effort.is_none()
        && receiver_thread_id.is_none()
        && receiver_nickname.is_none()
        && receiver_role.is_none()
        && receiver_status.is_none()
    {
        return None;
    }

    Some(SpawnAgentEntry {
        prompt,
        requested_agent_type,
        model,
        reasoning_effort,
        receiver_thread_id,
        receiver_nickname,
        receiver_role,
        receiver_status,
    })
}

fn extract_user_input_request_entry(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> Option<UserInputRequestEntry> {
    if event_type != USER_INPUT_REQUEST {
        return None;
    }

    let payload = payload?;
    let input = payload.get("input").and_then(Value::as_object);
    let output = payload.get("output").and_then(Value::as_object);
    let answers_by_id = extract_user_input_answers_by_id(output);
    let mut matched_answer_ids = BTreeSet::new();
    let questions = input
        .and_then(|obj| obj.get("questions"))
        .and_then(Value::as_array)
        .map(|questions| {
            questions
                .iter()
                .filter_map(|question| {
                    extract_user_input_question(question, &answers_by_id, &mut matched_answer_ids)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let extra_answers = answers_by_id
        .into_iter()
        .filter(|(id, answers)| !matched_answer_ids.contains(id) && !answers.is_empty())
        .map(|(id, answers)| UserInputAnswerEntry { id, answers })
        .collect::<Vec<_>>();

    if questions.is_empty() && extra_answers.is_empty() {
        None
    } else {
        Some(UserInputRequestEntry {
            questions,
            extra_answers,
        })
    }
}

fn extract_user_input_question(
    value: &Value,
    answers_by_id: &BTreeMap<String, Vec<String>>,
    matched_answer_ids: &mut BTreeSet<String>,
) -> Option<UserInputQuestionEntry> {
    let object = value.as_object()?;
    let header = object
        .get("header")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let id = object
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let question = object
        .get("question")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let options = object
        .get("options")
        .and_then(Value::as_array)
        .map(|options| {
            options
                .iter()
                .filter_map(extract_user_input_option)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let answers = id
        .as_ref()
        .and_then(|id| answers_by_id.get(id))
        .cloned()
        .unwrap_or_default();
    if let Some(id) = id.as_ref() {
        matched_answer_ids.insert(id.clone());
    }

    if header.is_none()
        && id.is_none()
        && question.is_none()
        && options.is_empty()
        && answers.is_empty()
    {
        None
    } else {
        Some(UserInputQuestionEntry {
            header,
            id,
            question,
            options,
            answers,
        })
    }
}

fn extract_user_input_option(value: &Value) -> Option<UserInputOptionEntry> {
    match value {
        Value::Object(object) => {
            let label = object
                .get("label")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)?;
            let description = object
                .get("description")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            Some(UserInputOptionEntry { label, description })
        }
        Value::String(text) => {
            let label = text.trim();
            (!label.is_empty()).then(|| UserInputOptionEntry {
                label: label.to_string(),
                description: None,
            })
        }
        _ => None,
    }
}

fn extract_user_input_answers_by_id(
    output: Option<&serde_json::Map<String, Value>>,
) -> BTreeMap<String, Vec<String>> {
    let mut answers_by_id = BTreeMap::new();
    let Some(output) = output else {
        return answers_by_id;
    };
    let Some(answers) = output.get("answers").and_then(Value::as_object) else {
        return answers_by_id;
    };
    for (id, raw_answers) in answers {
        let values = extract_user_input_answer_values(raw_answers);
        if !values.is_empty() {
            answers_by_id.insert(id.clone(), values);
        }
    }
    answers_by_id
}

fn extract_user_input_answer_values(value: &Value) -> Vec<String> {
    match value {
        Value::Object(object) => object
            .get("answers")
            .map(extract_user_input_answer_values)
            .unwrap_or_default(),
        Value::Array(values) => values
            .iter()
            .filter_map(|value| match value {
                Value::String(text) => {
                    let text = text.trim();
                    (!text.is_empty()).then(|| text.to_string())
                }
                _ => {
                    let rendered = render_payload_value(value);
                    let rendered = rendered.trim();
                    (!rendered.is_empty()).then(|| rendered.to_string())
                }
            })
            .collect(),
        Value::String(text) => {
            let text = text.trim();
            (!text.is_empty())
                .then(|| vec![text.to_string()])
                .unwrap_or_default()
        }
        _ => {
            let rendered = render_payload_value(value);
            let rendered = rendered.trim();
            (!rendered.is_empty())
                .then(|| vec![rendered.to_string()])
                .unwrap_or_default()
        }
    }
}

fn first_receiver_thread_id(payload: &serde_json::Map<String, Value>) -> Option<&str> {
    payload
        .get("receiver_thread_ids")
        .and_then(Value::as_array)
        .and_then(|ids| ids.first())
        .and_then(Value::as_str)
}

fn spawn_agent_state<'a>(
    payload: &'a serde_json::Map<String, Value>,
    thread_id: Option<&'a str>,
) -> Option<(&'a str, &'a serde_json::Map<String, Value>)> {
    let states = payload.get("agents_states").and_then(Value::as_object)?;
    if let Some(thread_id) = thread_id {
        if let Some(state) = states
            .get(thread_id)
            .and_then(Value::as_object)
            .map(|state| (thread_id, state))
        {
            return Some(state);
        }

        let normalized_thread_id = normalize_thread_identifier(Some(thread_id))?;
        return states.iter().find_map(|(state_thread_id, value)| {
            (normalize_thread_identifier(Some(state_thread_id.as_str())).as_deref()
                == Some(normalized_thread_id.as_str()))
            .then(|| {
                value
                    .as_object()
                    .map(|state| (state_thread_id.as_str(), state))
            })
            .flatten()
        });
    }
    states
        .iter()
        .find_map(|(thread_id, value)| value.as_object().map(|state| (thread_id.as_str(), state)))
}

fn collab_status_value(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }
        Value::Bool(true) => Some("completed".to_string()),
        Value::Bool(false) => Some("failed".to_string()),
        Value::Object(object) if object.len() == 1 => object.keys().next().cloned(),
        _ => None,
    }
}

fn extract_runtime_context_pairs(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> Vec<(String, String)> {
    if event_type != RUNTIME_CONTEXT {
        return Vec::new();
    }

    let mut pairs = Vec::new();
    let mut trailing_pairs = Vec::new();
    let Some(payload) = payload else {
        return pairs;
    };

    for (key, value) in payload {
        if matches!(
            key.as_str(),
            "actor_type" | "thread_id" | "parent_thread_id" | "session_path" | "turn_id"
        ) {
            continue;
        }
        let pair = (key.clone(), render_payload_value(value));
        if is_runtime_context_trailing_key(key) {
            trailing_pairs.push(pair);
        } else {
            pairs.push(pair);
        }
    }
    pairs.extend(trailing_pairs);
    pairs
}

fn extract_patch_apply_status(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> Option<String> {
    if !matches!(event_type, PATCH_APPLY | PATCH_APPLY_DUPLICATE) {
        return None;
    }

    payload
        .and_then(|obj| obj.get("status"))
        .map(render_payload_value)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn extract_patch_apply_input(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> Option<String> {
    if !matches!(event_type, PATCH_APPLY | PATCH_APPLY_DUPLICATE) {
        return None;
    }

    payload
        .and_then(|obj| obj.get("input"))
        .map(render_payload_value)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn extract_patch_apply_changes(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> Vec<PatchApplyChangeEntry> {
    if !matches!(event_type, PATCH_APPLY | PATCH_APPLY_DUPLICATE) {
        return Vec::new();
    }

    let Some(changes) = payload.and_then(|obj| obj.get("changes")) else {
        return Vec::new();
    };

    let mut entries = match changes {
        Value::Object(object) => object
            .iter()
            .map(|(path, value)| PatchApplyChangeEntry {
                path: path.clone(),
                change_type: change_type_from_patch_value(value),
                unified_diff: patch_unified_diff_from_value(value),
                move_path: patch_move_path_from_value(value),
            })
            .collect::<Vec<_>>(),
        Value::Array(array) => array
            .iter()
            .filter_map(|value| {
                let object = value.as_object()?;
                let path = object
                    .get("path")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())?
                    .to_string();
                let change_type = object
                    .get("type")
                    .or_else(|| object.get("kind"))
                    .map(render_payload_value)
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                let unified_diff = object
                    .get("unified_diff")
                    .map(render_payload_value)
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                let move_path = object
                    .get("move_path")
                    .map(render_payload_value)
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty());
                Some(PatchApplyChangeEntry {
                    path,
                    change_type,
                    unified_diff,
                    move_path,
                })
            })
            .collect::<Vec<_>>(),
        _ => Vec::new(),
    };

    entries.sort_by(|left, right| left.path.cmp(&right.path));
    entries
}

fn change_type_from_patch_value(value: &Value) -> Option<String> {
    match value {
        Value::Object(object) => object
            .get("type")
            .or_else(|| object.get("kind"))
            .map(render_payload_value)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        Value::String(_) | Value::Bool(_) | Value::Number(_) => {
            let rendered = render_payload_value(value);
            let trimmed = rendered.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }
        _ => None,
    }
}

fn patch_unified_diff_from_value(value: &Value) -> Option<String> {
    value
        .as_object()
        .and_then(|object| object.get("unified_diff"))
        .map(render_payload_value)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn patch_move_path_from_value(value: &Value) -> Option<String> {
    value.as_object().and_then(|object| {
        object
            .get("move_path")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

fn is_runtime_context_trailing_key(key: &str) -> bool {
    key == "collaboration_mode" || key.ends_with("_instructions")
}

fn render_payload_value(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Bool(_) | Value::Number(_) | Value::Null => value.to_string(),
        _ => serde_json::to_string(value).unwrap_or_else(|_| "<invalid-json>".to_string()),
    }
}

fn extract_token_value(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
    key: &str,
) -> Option<u64> {
    if event_type != INFO_TOKENS {
        return None;
    }

    payload.and_then(|obj| obj.get(key)).and_then(Value::as_u64)
}

fn format_summary_number(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            out.push(' ');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

fn extract_command_aggregated_output(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> Option<String> {
    if !is_command_shell_result(event_type, payload) {
        return None;
    }
    let rendered = payload
        .and_then(|obj| obj.get("output"))
        .map(render_event_value)
        .unwrap_or_default();
    let trimmed = rendered.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn is_command_shell_event(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> bool {
    matches!(event_type, SHELL_CALL | SHELL_RESULT)
        && matches!(
            payload
                .and_then(|obj| obj.get("tool_name"))
                .and_then(Value::as_str),
            Some("command_execution" | "exec_command")
        )
}

fn extract_shell_input<'a>(
    event_type: &str,
    payload: Option<&'a serde_json::Map<String, Value>>,
) -> Option<&'a serde_json::Map<String, Value>> {
    if !is_command_shell_event(event_type, payload) {
        return None;
    }

    payload
        .and_then(|obj| obj.get("input"))
        .and_then(Value::as_object)
}

fn extract_shell_command(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> Option<String> {
    let input = extract_shell_input(event_type, payload)?;
    for key in ["command", "cmd"] {
        let Some(value) = input.get(key) else {
            continue;
        };
        if let Some(command) = render_shell_command_value(value) {
            return Some(command);
        }
    }
    None
}

fn render_shell_command_value(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }
        Value::Array(parts) => {
            let rendered_parts = parts
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>();
            if rendered_parts.is_empty() {
                return None;
            }

            if rendered_parts.len() >= 3
                && matches!(rendered_parts[1], "-c" | "-lc")
                && matches!(rendered_parts[0], "bash" | "/bin/bash" | "sh" | "/bin/sh")
            {
                return Some(rendered_parts[2..].join(" "));
            }

            Some(rendered_parts.join(" "))
        }
        _ => None,
    }
}

fn extract_shell_exit_code(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> Option<i32> {
    if !is_command_shell_result(event_type, payload) {
        return None;
    }

    payload
        .and_then(|obj| obj.get("exit_code"))
        .and_then(Value::as_i64)
        .and_then(|code| {
            if (i32::MIN as i64..=i32::MAX as i64).contains(&code) {
                Some(code as i32)
            } else {
                None
            }
        })
}

fn extract_shell_workdir(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> Option<String> {
    extract_shell_input(event_type, payload)
        .and_then(|input| extract_map_scalar_string(input, "workdir"))
}

fn extract_shell_cwd(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> Option<String> {
    if event_type != SHELL_RESULT || !is_command_shell_event(event_type, payload) {
        return None;
    }

    extract_payload_scalar_string(payload, "cwd")
}

fn extract_shell_yield_time_ms(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> Option<u64> {
    extract_shell_input(event_type, payload)
        .and_then(|input| extract_map_u64(input, "yield_time_ms"))
}

fn extract_shell_max_output_tokens(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> Option<u64> {
    extract_shell_input(event_type, payload)
        .and_then(|input| extract_map_u64(input, "max_output_tokens"))
}

fn extract_shell_login(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> Option<bool> {
    extract_shell_input(event_type, payload).and_then(|input| extract_map_bool(input, "login"))
}

fn extract_shell_tty(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> Option<bool> {
    extract_shell_input(event_type, payload).and_then(|input| extract_map_bool(input, "tty"))
}

fn extract_shell_binary(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> Option<String> {
    extract_shell_input(event_type, payload)
        .and_then(|input| extract_map_scalar_string(input, "shell"))
}

fn extract_shell_process_id(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> Option<String> {
    if event_type != SHELL_RESULT || !is_command_shell_event(event_type, payload) {
        return None;
    }

    extract_payload_scalar_string(payload, "process_id")
}

fn extract_shell_source(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> Option<String> {
    if event_type != SHELL_RESULT || !is_command_shell_event(event_type, payload) {
        return None;
    }

    extract_payload_scalar_string(payload, "source")
}

fn extract_shell_duration_ns(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> Option<u64> {
    if event_type != SHELL_RESULT || !is_command_shell_event(event_type, payload) {
        return None;
    }

    let duration = payload.and_then(|obj| obj.get("duration"))?;
    let object = duration.as_object()?;
    let secs = object.get("secs").and_then(extract_value_u64).unwrap_or(0);
    let nanos = object.get("nanos").and_then(extract_value_u64).unwrap_or(0);
    secs.checked_mul(1_000_000_000)?.checked_add(nanos)
}

fn extract_shell_original_token_count(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> Option<u64> {
    if event_type != SHELL_RESULT || !is_command_shell_event(event_type, payload) {
        return None;
    }

    payload
        .and_then(|obj| obj.get("original_token_count"))
        .and_then(extract_value_u64)
}

fn extract_shell_formatted_output(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> Option<String> {
    if event_type != SHELL_RESULT || !is_command_shell_event(event_type, payload) {
        return None;
    }

    extract_payload_scalar_string(payload, "formatted_output")
}

fn extract_shell_parsed_commands(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> Vec<ShellParsedCommandEntry> {
    if event_type != SHELL_RESULT || !is_command_shell_event(event_type, payload) {
        return Vec::new();
    }

    payload
        .and_then(|obj| obj.get("parsed_cmd"))
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let object = entry.as_object()?;
                    let parsed = ShellParsedCommandEntry {
                        kind: extract_map_scalar_string(object, "type"),
                        command: extract_map_scalar_string(object, "cmd"),
                        query: extract_map_scalar_string(object, "query"),
                        name: extract_map_scalar_string(object, "name"),
                        path: extract_map_scalar_string(object, "path"),
                    };
                    if parsed.kind.is_none()
                        && parsed.command.is_none()
                        && parsed.query.is_none()
                        && parsed.name.is_none()
                        && parsed.path.is_none()
                    {
                        None
                    } else {
                        Some(parsed)
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

fn is_command_shell_result(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> bool {
    event_type == SHELL_RESULT && is_command_shell_event(event_type, payload)
}

fn render_event_value(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        Value::Object(object) => {
            for key in ["aggregated_output", "output", "message", "stderr"] {
                if let Some(nested) = object.get(key) {
                    let rendered = render_event_value(nested);
                    if !rendered.trim().is_empty() {
                        return rendered;
                    }
                }
            }
            serde_json::to_string_pretty(value).unwrap_or_default()
        }
        Value::Array(_) => serde_json::to_string_pretty(value).unwrap_or_default(),
        other => other.to_string(),
    }
}

fn extract_map_scalar_string(map: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    extract_payload_scalar_string(Some(map), key)
}

fn extract_map_u64(map: &serde_json::Map<String, Value>, key: &str) -> Option<u64> {
    map.get(key).and_then(extract_value_u64)
}

fn extract_map_bool(map: &serde_json::Map<String, Value>, key: &str) -> Option<bool> {
    map.get(key).and_then(extract_value_bool)
}

fn extract_value_u64(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64(),
        Value::String(text) => text.trim().parse::<u64>().ok(),
        _ => None,
    }
}

fn extract_value_bool(value: &Value) -> Option<bool> {
    match value {
        Value::Bool(value) => Some(*value),
        Value::String(text) => match text.trim().to_ascii_lowercase().as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

fn event_thread_id(event: &EventRecord, root_thread_id: Option<&str>) -> Option<String> {
    let payload = event.payload.as_object();
    payload
        .and_then(|obj| obj.get("thread_id"))
        .and_then(Value::as_str)
        .and_then(|value| normalize_thread_identifier(Some(value)))
        .or_else(|| {
            payload
                .and_then(|obj| obj.get("sender_thread_id"))
                .and_then(Value::as_str)
                .and_then(|value| normalize_thread_identifier(Some(value)))
        })
        .or_else(|| match actor_type(event).as_deref() {
            Some("agent") => {
                root_thread_id.and_then(|value| normalize_thread_identifier(Some(value)))
            }
            _ => None,
        })
}

fn actor_type(event: &EventRecord) -> Option<&str> {
    event
        .payload
        .as_object()
        .and_then(|obj| obj.get("actor_type"))
        .and_then(Value::as_str)
}

fn first_seq_by_thread(
    thread_ids: &BTreeSet<String>,
    events_by_thread: &BTreeMap<String, Vec<EventEntry>>,
) -> HashMap<String, u64> {
    thread_ids
        .iter()
        .map(|thread_id| {
            let first_seq = events_by_thread
                .get(thread_id)
                .and_then(|events| events.first())
                .map(|event| event.seq)
                .unwrap_or(u64::MAX);
            (thread_id.clone(), first_seq)
        })
        .collect()
}

fn build_thread_index(
    all_thread_ids: &BTreeSet<String>,
    agents: &HashMap<String, AgentSnapshot>,
    first_seq_by_thread: &HashMap<String, u64>,
    root_thread_id: &str,
) -> (Vec<String>, HashMap<String, Vec<String>>) {
    let mut root_threads = Vec::new();
    let mut children_by_parent: HashMap<String, Vec<String>> = HashMap::new();

    for thread_id in all_thread_ids {
        let parent_thread_id = agents
            .get(thread_id)
            .and_then(|agent| agent.parent_thread_id.clone())
            .filter(|parent| all_thread_ids.contains(parent));

        if let Some(parent_thread_id) = parent_thread_id {
            children_by_parent
                .entry(parent_thread_id)
                .or_default()
                .push(thread_id.clone());
        } else {
            root_threads.push(thread_id.clone());
        }
    }

    sort_threads(&mut root_threads, first_seq_by_thread, Some(root_thread_id));
    for children in children_by_parent.values_mut() {
        sort_threads(children, first_seq_by_thread, None);
    }

    (root_threads, children_by_parent)
}

fn canonical_run_id(run_id: &str) -> String {
    normalize_operation_scope_value(run_id).unwrap_or_else(|| run_id.to_string())
}

fn normalize_thread_identifier(value: Option<&str>) -> Option<String> {
    value.and_then(normalize_operation_scope_value)
}

fn normalize_agent_snapshots(
    agents: &HashMap<String, AgentSnapshot>,
) -> HashMap<String, AgentSnapshot> {
    let mut normalized = HashMap::new();
    let mut entries: Vec<_> = agents.iter().collect();
    entries.sort_by(|(left, _), (right, _)| left.cmp(right));

    for (raw_thread_id, agent) in entries {
        let Some(thread_id) = normalize_thread_identifier(Some(agent.thread_id.as_str()))
            .or_else(|| normalize_thread_identifier(Some(raw_thread_id.as_str())))
        else {
            continue;
        };

        let mut candidate = agent.clone();
        candidate.thread_id = thread_id.clone();
        candidate.parent_thread_id =
            normalize_thread_identifier(candidate.parent_thread_id.as_deref());

        if let Some(existing) = normalized.get_mut(&thread_id) {
            merge_agent_snapshot(existing, candidate);
        } else {
            normalized.insert(thread_id, candidate);
        }
    }

    normalized
}

fn merge_agent_snapshot(existing: &mut AgentSnapshot, candidate: AgentSnapshot) {
    if existing.parent_thread_id.is_none() {
        existing.parent_thread_id = candidate.parent_thread_id.clone();
    }
    if existing.status.is_empty() && !candidate.status.is_empty() {
        existing.status = candidate.status.clone();
    }
    if existing.nickname.is_none() {
        existing.nickname = candidate.nickname.clone();
    }
    if existing.role.is_none() {
        existing.role = candidate.role.clone();
    }
    if existing.cwd.is_none() {
        existing.cwd = candidate.cwd.clone();
    }
    if existing.color.is_none() {
        existing.color = candidate.color.clone();
    }
    if existing.pending_response_category.is_none() {
        existing.pending_response_category = candidate.pending_response_category;
    }
    if existing.recent_lines.len() < candidate.recent_lines.len() {
        existing.recent_lines = candidate.recent_lines.clone();
    }
}

fn sort_threads(
    threads: &mut [String],
    first_seq_by_thread: &HashMap<String, u64>,
    root_thread_id: Option<&str>,
) {
    threads.sort_by(|left, right| {
        let left_is_root = root_thread_id == Some(left.as_str());
        let right_is_root = root_thread_id == Some(right.as_str());
        left_is_root
            .cmp(&right_is_root)
            .reverse()
            .then_with(|| {
                first_seq_by_thread
                    .get(left)
                    .copied()
                    .unwrap_or(u64::MAX)
                    .cmp(&first_seq_by_thread.get(right).copied().unwrap_or(u64::MAX))
            })
            .then_with(|| left.cmp(right))
    });
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use serde_json::json;
    use tempfile::tempdir;

    use super::{
        build_event_tree, is_rollout_jsonl_family, load_records_any, load_records_from_run_input,
        load_records_from_standalone_rollout, validate_standalone_rollout_root, TimelineItem,
    };
    use crate::events::record::EventRecord;

    fn make_event(event_type: &str, payload: serde_json::Value, seq: u64) -> EventRecord {
        EventRecord {
            schema_version: 1,
            ts: "2026-04-06T08:47:59Z".to_string(),
            task_id: "smoke-run".to_string(),
            run_id: "run-1".to_string(),
            seq,
            event_type: event_type.to_string(),
            raw_type: event_type.to_string(),
            parse_status: "parsed".to_string(),
            payload,
        }
    }

    fn rollout_root_name(session_id: &str) -> String {
        format!("rollout-2026-04-06T22-54-37-{session_id}.jsonl")
    }

    fn write_rollout_file(
        tmp: &tempfile::TempDir,
        basename: &str,
        session_id: &str,
        body: &str,
    ) -> PathBuf {
        let path = tmp.path().join(basename);
        let content = format!(
            "{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{session_id}\",\"parent_thread_id\":\"root-thread\"}}}}\n{body}"
        );
        fs::write(&path, content).expect("rollout jsonl should be written");
        path
    }

    #[test]
    fn load_records_any_supports_json_array_files() {
        let tmp = tempdir().expect("temp dir should be created");
        let path = tmp.path().join("events.json");
        let events = vec![make_event(
            "thread.started",
            json!({"thread_id":"root-thread"}),
            1,
        )];
        fs::write(
            &path,
            serde_json::to_string(&events).expect("events should serialize"),
        )
        .expect("events.json should be written");

        let loaded = load_records_any(&path).expect("events should load");

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].event_type, "thread.started");
        assert_eq!(loaded[0].payload["thread_id"], "root-thread");
    }

    #[test]
    fn load_records_any_supports_json_array_with_trailing_comma() {
        let tmp = tempdir().expect("temp dir should be created");
        let path = tmp.path().join("events.json");
        fs::write(
            &path,
            "[\n{\"schema_version\":1,\"ts\":\"2026-04-06T08:47:59Z\",\"task_id\":\"smoke-run\",\"run_id\":\"run-1\",\"seq\":1,\"event_type\":\"thread.started\",\"raw_type\":\"thread.started\",\"parse_status\":\"parsed\",\"payload\":{\"thread_id\":\"root-thread\"}},\n]\n",
        )
        .expect("events.json should be written");

        let loaded = load_records_any(&path).expect("events should load");

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].payload["thread_id"], "root-thread");
    }

    #[test]
    fn validate_standalone_rollout_root_extracts_hyphenated_session_id_after_timestamp_prefix() {
        let tmp = tempdir().expect("temp dir should be created");
        let session_id = "019d645c-816c-7761-a34e-9db1ca764618";
        let path = tmp.path().join(rollout_root_name(session_id));
        fs::write(
            &path,
            format!("{{\"type\":\"session_meta\",\"payload\":{{\"id\":\"{session_id}\"}}}}\n"),
        )
        .expect("rollout jsonl should be written");

        let validated =
            validate_standalone_rollout_root(&path).expect("standalone rollout root should pass");

        assert_eq!(validated, session_id);
    }

    #[test]
    fn is_rollout_jsonl_family_recognizes_rollout_prefixed_jsonl_paths() {
        assert!(is_rollout_jsonl_family(Path::new(
            "/tmp/rollout-2026-04-06T22-54-37-sub1.jsonl"
        )));
        assert!(is_rollout_jsonl_family(Path::new(
            "/tmp/rollout-shadow.jsonl"
        )));
        assert!(!is_rollout_jsonl_family(Path::new(
            "/tmp/events-shadow.jsonl"
        )));
    }

    #[test]
    fn load_records_from_standalone_rollout_parses_session_jsonl_and_startup_metadata() {
        let tmp = tempdir().expect("temp dir should be created");
        let path = tmp.path().join(rollout_root_name("sub1"));
        fs::write(
            &path,
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"sub1\",\"parent_thread_id\":\"root-thread\",\"approval_policy\":\"never\",\"sandbox_policy\":\"danger-full-access\"}}\n",
        )
        .expect("rollout jsonl should be written");

        let loaded =
            load_records_from_standalone_rollout(&path, "sub1").expect("standalone should load");

        assert_eq!(loaded.events.len(), 1);
        assert_eq!(loaded.events[0].event_type, "agent.session");
        assert_eq!(loaded.events[0].task_id, "standalone-rollout");
        assert_eq!(loaded.events[0].run_id, "standalone-rollout");
        assert_eq!(loaded.startup_metadata["id"], "sub1");
        assert_eq!(loaded.startup_metadata["approval_policy"], "never");
        assert_eq!(
            loaded.startup_metadata["sandbox_policy"],
            "danger-full-access"
        );
    }

    #[test]
    fn load_records_from_standalone_rollout_uses_nested_parent_thread_id_from_session_meta() {
        let tmp = tempdir().expect("temp dir should be created");
        let path = tmp.path().join(rollout_root_name("sub1"));
        fs::write(
            &path,
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"sub1\",\"source\":{\"subagent\":{\"thread_spawn\":{\"parent_thread_id\":\"root-thread\"}}}}}\n",
        )
        .expect("rollout jsonl should be written");

        let loaded =
            load_records_from_standalone_rollout(&path, "sub1").expect("standalone should load");

        assert_eq!(loaded.events.len(), 1);
        assert_eq!(loaded.events[0].event_type, "agent.session");
        assert_eq!(
            loaded.events[0].payload["parent_thread_id"].as_str(),
            Some("root-thread")
        );
    }

    #[test]
    fn load_records_from_standalone_rollout_loads_only_linked_child_sessions() {
        let tmp = tempdir().expect("temp dir should be created");
        let root = write_rollout_file(
            &tmp,
            "rollout-main-root.jsonl",
            "root",
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"collab_tool_call\",\"id\":\"ct-1\",\"tool\":\"spawn_agent\",\"status\":\"completed\",\"sender_thread_id\":\"root\",\"receiver_thread_ids\":[\"sub-1\"],\"prompt\":\"go\",\"agents_states\":{\"sub-1\":{\"status\":\"ok\"}}}}\n",
        );
        write_rollout_file(&tmp, "rollout-main-sub-1.jsonl", "sub-1", "");
        write_rollout_file(&tmp, "rollout-main-sub-2.jsonl", "sub-2", "");

        let loaded =
            load_records_from_standalone_rollout(&root, "root").expect("standalone should load");
        let sessions: Vec<String> = loaded
            .events
            .iter()
            .filter(|event| event.event_type == "agent.session")
            .filter_map(|event| event.payload["thread_id"].as_str().map(str::to_string))
            .collect();

        assert!(sessions.contains(&"root".to_string()));
        assert!(sessions.contains(&"sub-1".to_string()));
        assert!(!sessions.contains(&"sub-2".to_string()));
    }

    #[test]
    fn load_records_from_standalone_rollout_deduplicates_repeated_child_ids() {
        let tmp = tempdir().expect("temp dir should be created");
        let root = write_rollout_file(
            &tmp,
            "rollout-main-root.jsonl",
            "root",
            concat!(
                "{\"type\":\"item.completed\",\"item\":{\"type\":\"collab_tool_call\",\"id\":\"ct-1\",\"tool\":\"spawn_agent\",\"status\":\"completed\",\"sender_thread_id\":\"root\",\"receiver_thread_ids\":[\"sub-1\",\"sub-1\"],\"prompt\":\"go\",\"agents_states\":{\"sub-1\":{\"status\":\"ok\"}}}}\n",
                "{\"type\":\"item.completed\",\"item\":{\"type\":\"collab_tool_call\",\"id\":\"ct-2\",\"tool\":\"spawn_agent\",\"status\":\"completed\",\"sender_thread_id\":\"root\",\"receiver_thread_ids\":[\"sub-1\"],\"prompt\":\"go2\",\"agents_states\":{\"sub-1\":{\"status\":\"ok\"}}}}\n"
            ),
        );
        write_rollout_file(&tmp, "rollout-main-sub-1.jsonl", "sub-1", "");

        let loaded =
            load_records_from_standalone_rollout(&root, "root").expect("standalone should load");
        let sub_1_sessions = loaded
            .events
            .iter()
            .filter(|event| event.event_type == "agent.session")
            .filter(|event| event.payload["thread_id"].as_str() == Some("sub-1"))
            .count();

        assert_eq!(sub_1_sessions, 1);
    }

    #[test]
    fn load_records_from_standalone_rollout_skips_missing_child_match() {
        let tmp = tempdir().expect("temp dir should be created");
        let root = write_rollout_file(
            &tmp,
            "rollout-main-root.jsonl",
            "root",
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"collab_tool_call\",\"id\":\"ct-1\",\"tool\":\"spawn_agent\",\"status\":\"completed\",\"sender_thread_id\":\"root\",\"receiver_thread_ids\":[\"sub-missing\"],\"prompt\":\"go\",\"agents_states\":{\"sub-missing\":{\"status\":\"ok\"}}}}\n",
        );

        let loaded =
            load_records_from_standalone_rollout(&root, "root").expect("standalone should load");

        assert_eq!(
            loaded
                .events
                .iter()
                .filter(|event| event.event_type == "agent.session")
                .count(),
            1
        );
    }

    #[test]
    fn load_records_from_standalone_rollout_fails_on_ambiguous_child_match() {
        let tmp = tempdir().expect("temp dir should be created");
        let root = write_rollout_file(
            &tmp,
            "rollout-main-root.jsonl",
            "root",
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"collab_tool_call\",\"id\":\"ct-1\",\"tool\":\"spawn_agent\",\"status\":\"completed\",\"sender_thread_id\":\"root\",\"receiver_thread_ids\":[\"sub-1\"],\"prompt\":\"go\",\"agents_states\":{\"sub-1\":{\"status\":\"ok\"}}}}\n",
        );
        write_rollout_file(&tmp, "rollout-a-sub-1.jsonl", "sub-1", "");
        write_rollout_file(&tmp, "rollout-b-sub-1.jsonl", "sub-1", "");

        let err = load_records_from_standalone_rollout(&root, "root")
            .expect_err("ambiguous child lookup should fail");
        assert!(err.to_string().contains("ambiguous child match"));
    }

    #[test]
    fn load_records_from_standalone_rollout_fails_when_child_identity_mismatches_file_suffix() {
        let tmp = tempdir().expect("temp dir should be created");
        let root = write_rollout_file(
            &tmp,
            "rollout-main-root.jsonl",
            "root",
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"collab_tool_call\",\"id\":\"ct-1\",\"tool\":\"spawn_agent\",\"status\":\"completed\",\"sender_thread_id\":\"root\",\"receiver_thread_ids\":[\"sub-1\"],\"prompt\":\"go\",\"agents_states\":{\"sub-1\":{\"status\":\"ok\"}}}}\n",
        );
        write_rollout_file(&tmp, "rollout-main-sub-1.jsonl", "foreign-sub", "");

        let err = load_records_from_standalone_rollout(&root, "root")
            .expect_err("child identity mismatch should fail");

        assert!(err
            .to_string()
            .contains("не совпадает с ожидаемым session-id"));
        assert!(err.to_string().contains("rollout-main-sub-1.jsonl"));
    }

    #[test]
    fn load_records_from_standalone_rollout_handles_cycle_without_duplicate_loads() {
        let tmp = tempdir().expect("temp dir should be created");
        let root = write_rollout_file(
            &tmp,
            "rollout-main-root.jsonl",
            "root",
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"collab_tool_call\",\"id\":\"ct-1\",\"tool\":\"spawn_agent\",\"status\":\"completed\",\"sender_thread_id\":\"root\",\"receiver_thread_ids\":[\"sub-1\"],\"prompt\":\"go\",\"agents_states\":{\"sub-1\":{\"status\":\"ok\"}}}}\n",
        );
        write_rollout_file(
            &tmp,
            "rollout-main-sub-1.jsonl",
            "sub-1",
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"collab_tool_call\",\"id\":\"ct-2\",\"tool\":\"spawn_agent\",\"status\":\"completed\",\"sender_thread_id\":\"sub-1\",\"receiver_thread_ids\":[\"root\"],\"prompt\":\"loop\",\"agents_states\":{\"root\":{\"status\":\"ok\"}}}}\n",
        );

        let loaded =
            load_records_from_standalone_rollout(&root, "root").expect("standalone should load");
        let sessions: Vec<String> = loaded
            .events
            .iter()
            .filter(|event| event.event_type == "agent.session")
            .filter_map(|event| event.payload["thread_id"].as_str().map(str::to_string))
            .collect();

        assert_eq!(
            sessions
                .iter()
                .filter(|thread_id| thread_id.as_str() == "root")
                .count(),
            1
        );
        assert_eq!(
            sessions
                .iter()
                .filter(|thread_id| thread_id.as_str() == "sub-1")
                .count(),
            1
        );
    }

    #[test]
    fn load_records_from_standalone_rollout_traverses_children_deterministically() {
        let tmp = tempdir().expect("temp dir should be created");
        let root = write_rollout_file(
            &tmp,
            "rollout-main-root.jsonl",
            "root",
            "{\"type\":\"item.completed\",\"item\":{\"type\":\"collab_tool_call\",\"id\":\"ct-1\",\"tool\":\"spawn_agent\",\"status\":\"completed\",\"sender_thread_id\":\"root\",\"receiver_thread_ids\":[\"sub-2\",\"sub-1\"],\"prompt\":\"go\",\"agents_states\":{\"sub-1\":{\"status\":\"ok\"},\"sub-2\":{\"status\":\"ok\"}}}}\n",
        );
        write_rollout_file(&tmp, "rollout-z-sub-2.jsonl", "sub-2", "");
        write_rollout_file(&tmp, "rollout-a-sub-1.jsonl", "sub-1", "");

        let loaded =
            load_records_from_standalone_rollout(&root, "root").expect("standalone should load");
        let sessions: Vec<String> = loaded
            .events
            .iter()
            .filter(|event| event.event_type == "agent.session")
            .filter_map(|event| event.payload["thread_id"].as_str().map(str::to_string))
            .filter(|thread_id| thread_id != "root")
            .collect();

        assert_eq!(sessions, vec!["sub-1".to_string(), "sub-2".to_string()]);
    }

    #[test]
    fn build_event_tree_attaches_child_thread_without_reparenting_agent_message() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "collab.spawn_agent",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"spawn_agent",
                    "phase":"completed",
                    "status":"completed",
                    "receiver_thread_ids":["sub-1"],
                    "agents_states":{"sub-1":{"status":"pending_init"}}
                }),
                2,
            ),
            make_event(
                "agent.session",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "agent_role":"default",
                    "agent_nickname":"Hegel"
                }),
                3,
            ),
            make_event(
                "message.agent",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "text":"root continues"
                }),
                10,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);

        let root = &tree.roots[0];
        assert_eq!(root.thread_id, "root-thread");
        assert_eq!(root.items.len(), 3);

        let spawn = match &root.items[1] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected root event"),
        };
        assert_eq!(spawn.event.seq, 2);
        assert_eq!(spawn.children.len(), 1);

        match &spawn.children[0] {
            TimelineItem::Thread(thread) => {
                assert_eq!(thread.thread_id, "sub-1");
                assert_eq!(thread.parent_event_id.as_deref(), Some("run-1:2"));
            }
            TimelineItem::Event(_) => panic!("expected child thread first"),
        }

        match &root.items[2] {
            TimelineItem::Event(node) => {
                assert_eq!(node.event.seq, 10);
                assert_eq!(node.event.parent_event_id.as_deref(), None);
            }
            TimelineItem::Thread(_) => panic!("expected root message as top-level event"),
        }
    }

    #[test]
    fn build_event_tree_keeps_nested_subagent_under_immediate_parent() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "collab.spawn_agent",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"spawn_agent",
                    "phase":"completed",
                    "status":"completed",
                    "sender_thread_id":"root-thread",
                    "receiver_thread_ids":["sub-1"],
                    "agents_states":{"sub-1":{"status":"pending_init"}}
                }),
                2,
            ),
            make_event(
                "agent.session",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "agent_role":"worker"
                }),
                3,
            ),
            make_event(
                "collab.spawn_agent",
                json!({
                    "actor_type":"agent",
                    "thread_id":"sub-1",
                    "tool_name":"spawn_agent",
                    "phase":"completed",
                    "status":"completed",
                    "sender_thread_id":"sub-1",
                    "receiver_thread_ids":["sub-2"],
                    "agents_states":{"sub-2":{"status":"pending_init"}}
                }),
                4,
            ),
            make_event(
                "agent.session",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-2",
                    "parent_thread_id":"sub-1",
                    "agent_role":"reviewer"
                }),
                5,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);

        let root = &tree.roots[0];
        let sub_1 = match &root.items[1] {
            TimelineItem::Event(node) => match &node.children[0] {
                TimelineItem::Thread(thread) => thread,
                TimelineItem::Event(_) => panic!("expected child thread under root spawn"),
            },
            TimelineItem::Thread(_) => panic!("expected root spawn event"),
        };
        assert_eq!(sub_1.thread_id, "sub-1");

        let nested_spawn = match &sub_1.items[1] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected nested spawn event"),
        };
        let sub_2 = match &nested_spawn.children[0] {
            TimelineItem::Thread(thread) => thread,
            TimelineItem::Event(_) => panic!("expected nested child thread"),
        };
        assert_eq!(sub_2.thread_id, "sub-2");
        assert_eq!(sub_2.parent_event_id.as_deref(), Some("run-1:4"));
    }

    #[test]
    fn build_event_tree_nests_web_search_completed_under_started() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "web.search",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"web_search",
                    "tool_use_id":"web-1",
                    "phase":"started"
                }),
                2,
            ),
            make_event(
                "web.search",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"web_search",
                    "tool_use_id":"web-1",
                    "phase":"completed"
                }),
                3,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let root = &tree.roots[0];
        let web_search_started = match &root.items[1] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected web_search start event"),
        };
        let web_search_completed = match &web_search_started.children[0] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected web_search completion child"),
        };

        assert_eq!(web_search_started.event.event_type, "web.search");
        assert_eq!(web_search_completed.event.event_type, "web.search");
        assert_eq!(
            web_search_started.event.operation_kind.as_deref(),
            Some("web.search")
        );
        assert_eq!(web_search_started.event.operation_started_seq, Some(2));
        assert_eq!(web_search_started.event.operation_terminal_seq, Some(3));
        assert_eq!(
            web_search_completed
                .event
                .operation_root_event_id
                .as_deref(),
            Some("run-1:2")
        );
        assert!(web_search_completed.event.operation_is_preferred_terminal);
        assert_eq!(
            web_search_completed.event.parent_event_id.as_deref(),
            Some("run-1:2")
        );
    }

    #[test]
    fn build_event_tree_normalizes_scope_identifiers_for_operation_parenting() {
        let mut completed = make_event(
            "web.search",
            json!({
                "actor_type":"agent",
                "thread_id":" root-thread ",
                "tool_name":"web_search",
                "tool_use_id":"web-1",
                "phase":"completed"
            }),
            3,
        );
        completed.run_id = " run-1 ".to_string();

        let events = vec![
            make_event("thread.started", json!({"thread_id":" root-thread "}), 1),
            make_event(
                "web.search",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"web_search",
                    "tool_use_id":"web-1",
                    "phase":"started"
                }),
                2,
            ),
            completed,
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        assert_eq!(tree.run_id, "run-1");
        assert_eq!(tree.root_thread_id, "root-thread");
        assert_eq!(tree.thread_count, 1);
        assert!(tree.orphan_events.is_empty());

        let root = &tree.roots[0];
        assert_eq!(root.thread_id, "root-thread");
        let web_search_started = match &root.items[1] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected web_search start event"),
        };
        let web_search_completed = match &web_search_started.children[0] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected web_search completion child"),
        };

        assert_eq!(web_search_completed.event.event_id, "run-1:3");
        assert_eq!(
            web_search_completed
                .event
                .operation_root_event_id
                .as_deref(),
            Some("run-1:2")
        );
        assert_eq!(
            web_search_completed.event.parent_event_id.as_deref(),
            Some("run-1:2")
        );
        assert!(web_search_completed.event.operation_is_preferred_terminal);
    }

    #[test]
    fn build_event_tree_nests_web_open_completed_under_started() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "web.open",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"web_search",
                    "tool_use_id":"web-1",
                    "phase":"started",
                    "input":{"action":{"type":"open_page","url":"https://example.com"}}
                }),
                2,
            ),
            make_event(
                "web.open",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"web_search",
                    "tool_use_id":"web-1",
                    "phase":"completed",
                    "output":{"action":{"type":"open_page","url":"https://example.com"}}
                }),
                3,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let root = &tree.roots[0];
        let web_open_started = match &root.items[1] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected web_open start event"),
        };
        let web_open_completed = match &web_open_started.children[0] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected web_open completion child"),
        };

        assert_eq!(web_open_started.event.event_type, "web.open");
        assert_eq!(web_open_completed.event.event_type, "web.open");
        assert_eq!(
            web_open_completed.event.parent_event_id.as_deref(),
            Some("run-1:2")
        );
    }

    #[test]
    fn build_event_tree_nests_stdin_write_completed_under_started() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "stdin.write",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"write_stdin",
                    "tool_use_id":"stdin-1",
                    "phase":"started",
                    "input":{"session_id":42,"chars":"ls\n"}
                }),
                2,
            ),
            make_event(
                "stdin.write",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"write_stdin",
                    "tool_use_id":"stdin-1",
                    "phase":"completed",
                    "output":{"stdout":"ok"}
                }),
                3,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let root = &tree.roots[0];
        let started = match &root.items[1] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected stdin.write start event"),
        };
        let completed = match &started.children[0] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected stdin.write completion child"),
        };

        assert_eq!(started.event.event_type, "stdin.write");
        assert_eq!(completed.event.event_type, "stdin.write");
        assert_eq!(completed.event.parent_event_id.as_deref(), Some("run-1:2"));
    }

    #[test]
    fn build_event_tree_nests_patch_apply_completed_under_started() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "patch.apply",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "tool_name":"apply_patch",
                    "tool_use_id":"patch-1",
                    "phase":"started",
                    "input":"*** Begin Patch\n*** Update File: src/events/readers.rs\n*** End Patch\n"
                }),
                2,
            ),
            make_event(
                "patch.apply",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "tool_name":"apply_patch",
                    "tool_use_id":"patch-1",
                    "phase":"completed",
                    "status":"completed",
                    "success":true,
                    "changes":{"src/events/readers.rs":{"type":"update"}}
                }),
                3,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let root = &tree.roots[0];
        let root_started = match &root.items[0] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected root thread.started event"),
        };
        let sub_thread = match &root_started.children[0] {
            TimelineItem::Thread(thread) => thread,
            TimelineItem::Event(_) => panic!("expected child thread"),
        };
        let patch_started = match &sub_thread.items[0] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected patch apply start event"),
        };
        let patch_completed = match &patch_started.children[0] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected patch apply completion child"),
        };

        assert_eq!(patch_started.event.event_type, "patch.apply");
        assert_eq!(patch_completed.event.event_type, "patch.apply");
        assert_eq!(
            patch_started.event.operation_kind.as_deref(),
            Some("patch.apply")
        );
        assert_eq!(patch_started.event.operation_started_seq, Some(2));
        assert_eq!(patch_completed.event.operation_terminal_seq, Some(3));
        assert!(patch_completed.event.operation_is_preferred_terminal);
        assert_eq!(
            patch_completed.event.parent_event_id.as_deref(),
            Some("run-1:2")
        );
    }

    #[test]
    fn build_event_tree_prefers_event_msg_shell_terminal_over_response_item_duplicate() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "shell.call",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"command_execution",
                    "tool_use_id":"cmd-1",
                    "input":{"cmd":"git status --short"}
                }),
                2,
            ),
            make_event(
                "shell.result",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"command_execution",
                    "tool_use_id":"cmd-1",
                    "duplicate_of":"response_item.function_call_output",
                    "input":{"command":["/bin/bash","-lc","git status --short"]},
                    "output":"",
                    "exit_code":0
                }),
                3,
            ),
            make_event(
                "shell.result",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"command_execution",
                    "tool_use_id":"cmd-1",
                    "output":"Command: /bin/bash -lc 'git status --short'\nOutput:\n",
                    "phase":"completed"
                }),
                4,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let root = &tree.roots[0];
        let shell_call = match &root.items[1] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected shell call event"),
        };
        let canonical_result = match &shell_call.children[0] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected shell result child"),
        };
        let duplicate_result = match &shell_call.children[1] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected duplicate shell result child"),
        };

        assert_eq!(
            shell_call.event.shell_command.as_deref(),
            Some("git status --short")
        );
        assert_eq!(
            canonical_result.event.shell_command.as_deref(),
            Some("git status --short")
        );
        assert!(canonical_result.event.operation_is_preferred_terminal);
        assert_eq!(canonical_result.event.operation_terminal_seq, Some(3));
        assert!(!duplicate_result.event.operation_is_preferred_terminal);
        assert_eq!(duplicate_result.event.operation_terminal_seq, Some(3));
    }

    #[test]
    fn build_event_tree_extracts_shell_command_from_command_array() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "shell.result",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"command_execution",
                    "tool_use_id":"cmd-array",
                    "input":{"command":["/bin/bash","-lc","git branch --show-current"]},
                    "output":"main\n",
                    "exit_code":0
                }),
                2,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let root = &tree.roots[0];
        let shell_result = match &root.items[1] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected shell result event"),
        };

        assert_eq!(
            shell_result.event.shell_command.as_deref(),
            Some("git branch --show-current")
        );
    }

    #[test]
    fn build_event_tree_extracts_shell_execution_metadata() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "shell.call",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"root-thread",
                    "tool_name":"exec_command",
                    "tool_use_id":"cmd-meta",
                    "input":{
                        "cmd":"sed -n '1,20p' README.md",
                        "workdir":"/repo",
                        "yield_time_ms":1000,
                        "max_output_tokens":2000,
                        "login":true,
                        "tty":true,
                        "shell":"/bin/zsh"
                    }
                }),
                2,
            ),
            make_event(
                "shell.result",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"root-thread",
                    "tool_name":"exec_command",
                    "tool_use_id":"cmd-meta",
                    "input":{"command":["/bin/bash","-lc","sed -n '1,20p' README.md"]},
                    "output":"# README\n",
                    "exit_code":0,
                    "cwd":"/repo",
                    "process_id":"4242",
                    "source":"unified_exec_startup",
                    "duration":{"secs":1,"nanos":250000000},
                    "original_token_count":4096,
                    "formatted_output":"Command: ...",
                    "parsed_cmd":[
                        {
                            "type":"read",
                            "cmd":"sed -n '1,20p' README.md",
                            "name":"README.md",
                            "path":"README.md"
                        },
                        {
                            "type":"search",
                            "cmd":"rg 'README'",
                            "query":"README"
                        }
                    ]
                }),
                3,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let root = &tree.roots[0];
        let shell_call = match &root.items[1] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected shell call event"),
        };
        let shell_result = match &shell_call.children[0] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected shell result event"),
        };

        assert_eq!(shell_call.event.shell_workdir.as_deref(), Some("/repo"));
        assert_eq!(shell_call.event.shell_yield_time_ms, Some(1000));
        assert_eq!(shell_call.event.shell_max_output_tokens, Some(2000));
        assert_eq!(shell_call.event.shell_login, Some(true));
        assert_eq!(shell_call.event.shell_tty, Some(true));
        assert_eq!(shell_call.event.shell_binary.as_deref(), Some("/bin/zsh"));
        assert_eq!(shell_result.event.shell_cwd.as_deref(), Some("/repo"));
        assert_eq!(shell_result.event.shell_process_id.as_deref(), Some("4242"));
        assert_eq!(
            shell_result.event.shell_source.as_deref(),
            Some("unified_exec_startup")
        );
        assert_eq!(shell_result.event.shell_duration_ns, Some(1_250_000_000));
        assert_eq!(shell_result.event.shell_original_token_count, Some(4096));
        assert_eq!(
            shell_result.event.shell_formatted_output.as_deref(),
            Some("Command: ...")
        );
        assert_eq!(shell_result.event.shell_parsed_commands.len(), 2);
        assert_eq!(
            shell_result.event.shell_parsed_commands[0].kind.as_deref(),
            Some("read")
        );
        assert_eq!(
            shell_result.event.shell_parsed_commands[0]
                .command
                .as_deref(),
            Some("sed -n '1,20p' README.md")
        );
        assert_eq!(
            shell_result.event.shell_parsed_commands[0].name.as_deref(),
            Some("README.md")
        );
        assert_eq!(
            shell_result.event.shell_parsed_commands[0].path.as_deref(),
            Some("README.md")
        );
        assert_eq!(
            shell_result.event.shell_parsed_commands[1].kind.as_deref(),
            Some("search")
        );
        assert_eq!(
            shell_result.event.shell_parsed_commands[1].query.as_deref(),
            Some("README")
        );
    }

    #[test]
    fn build_event_tree_nests_file_change_completed_under_started() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "file.change",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "item_id":"fc-1",
                    "phase":"started",
                    "changes":[{"kind":"update","path":"/tmp/demo.py"}]
                }),
                2,
            ),
            make_event(
                "file.change",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "item_id":"fc-1",
                    "phase":"completed",
                    "changes":[{"kind":"update","path":"/tmp/demo.py"}]
                }),
                3,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let root = &tree.roots[0];
        let file_change_started = match &root.items[1] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected file change start"),
        };
        let file_change_completed = match &file_change_started.children[0] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected file change completion child"),
        };

        assert_eq!(file_change_completed.event.event_type, "file.change");
        assert_eq!(
            file_change_completed.event.parent_event_id.as_deref(),
            Some("run-1:2")
        );
    }

    #[test]
    fn build_event_tree_nests_plan_update_completed_under_started() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "todo.update",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "tool_name":"update_plan",
                    "tool_use_id":"plan-1",
                    "phase":"started",
                    "input":{"plan":[{"step":"Inspect","status":"completed"}]}
                }),
                2,
            ),
            make_event(
                "todo.update",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "tool_name":"update_plan",
                    "tool_use_id":"plan-1",
                    "phase":"completed",
                    "output":{"plan":[{"step":"Inspect","status":"completed"}]}
                }),
                3,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let root = &tree.roots[0];
        let root_started = match &root.items[0] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected root thread.started event"),
        };
        let sub_thread = match &root_started.children[0] {
            TimelineItem::Thread(thread) => thread,
            TimelineItem::Event(_) => panic!("expected child thread"),
        };
        let plan_update_started = match &sub_thread.items[0] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected plan update start event"),
        };
        let plan_update_completed = match &plan_update_started.children[0] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected plan update completion child"),
        };

        assert_eq!(plan_update_started.event.event_type, "todo.update");
        assert_eq!(plan_update_completed.event.event_type, "todo.update");
        assert_eq!(
            plan_update_completed.event.parent_event_id.as_deref(),
            Some("run-1:2")
        );
    }

    #[test]
    fn build_event_tree_nests_user_input_request_completed_under_started() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "user.input.request",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "tool_name":"request_user_input",
                    "tool_use_id":"rui-1",
                    "phase":"started",
                    "input":{"questions":[{"id":"child_lookup","question":"How?"}]}
                }),
                2,
            ),
            make_event(
                "user.input.request",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "tool_name":"request_user_input",
                    "tool_use_id":"rui-1",
                    "phase":"completed",
                    "output":{"answers":{"child_lookup":{"answers":["same dir"]}}}
                }),
                3,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let root = &tree.roots[0];
        let root_started = match &root.items[0] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected root thread.started event"),
        };
        let sub_thread = match &root_started.children[0] {
            TimelineItem::Thread(thread) => thread,
            TimelineItem::Event(_) => panic!("expected child thread"),
        };
        let request_started = match &sub_thread.items[0] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected request_user_input start event"),
        };
        let request_completed = match &request_started.children[0] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected request_user_input completion child"),
        };

        assert_eq!(request_started.event.event_type, "user.input.request");
        assert_eq!(request_completed.event.event_type, "user.input.request");
        assert_eq!(
            request_completed.event.parent_event_id.as_deref(),
            Some("run-1:2")
        );
    }

    #[test]
    fn build_event_tree_nests_close_agent_completed_under_started() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "agent.session",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "agent_role":"reviewer",
                    "agent_nickname":"Ada"
                }),
                2,
            ),
            make_event(
                "collab.close_agent",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "tool_name":"close_agent",
                    "tool_use_id":"close-1",
                    "phase":"started",
                    "input":{"target":"peer-1"},
                    "receiver_thread_ids":["peer-1"]
                }),
                3,
            ),
            make_event(
                "collab.close_agent",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "tool_name":"close_agent",
                    "tool_use_id":"close-1",
                    "phase":"completed",
                    "status":"completed",
                    "receiver_thread_ids":["peer-1"],
                    "output":{"previous_status":{"completed":"closed"}}
                }),
                4,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let root = &tree.roots[0];
        let root_started = match &root.items[0] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected root thread.started event"),
        };
        let sub_thread = match &root_started.children[0] {
            TimelineItem::Thread(thread) => thread,
            TimelineItem::Event(_) => panic!("expected child thread"),
        };
        let close_started = match &sub_thread.items[1] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected close_agent start event"),
        };
        let close_completed = match &close_started.children[0] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected close_agent completion child"),
        };

        assert_eq!(close_started.event.event_type, "collab.close_agent");
        assert_eq!(close_completed.event.event_type, "collab.close_agent");
        assert_eq!(
            close_completed.event.parent_event_id.as_deref(),
            Some("run-1:3")
        );
    }

    #[test]
    fn build_event_tree_nests_mcp_result_under_call() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "mcp.call",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_use_id":"mcp-1",
                    "phase":"started",
                    "arguments":{"q":"a"},
                    "server":"docs",
                    "tool":"fetch_docs"
                }),
                2,
            ),
            make_event(
                "mcp.result",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_use_id":"mcp-1",
                    "phase":"completed",
                    "arguments":{"q":"a"},
                    "result":{"ok":true},
                    "server":"docs",
                    "tool":"fetch_docs"
                }),
                3,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let root = &tree.roots[0];
        let mcp_call = match &root.items[1] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected mcp call event"),
        };
        let mcp_result = match &mcp_call.children[0] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected mcp result child"),
        };

        assert_eq!(mcp_call.event.event_type, "mcp.call");
        assert_eq!(mcp_result.event.event_type, "mcp.result");
        assert_eq!(mcp_result.event.parent_event_id.as_deref(), Some("run-1:2"));
    }

    #[test]
    fn build_event_tree_anchors_child_thread_to_spawn_agent_operation_root() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "collab.spawn_agent",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"spawn_agent",
                    "tool_use_id":"ct-1",
                    "phase":"started",
                    "status":"in_progress"
                }),
                2,
            ),
            make_event(
                "shell.call",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"command_execution",
                    "tool_use_id":"cmd-1"
                }),
                3,
            ),
            make_event(
                "collab.spawn_agent",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"spawn_agent",
                    "tool_use_id":"ct-1",
                    "phase":"completed",
                    "status":"completed",
                    "receiver_thread_ids":["sub-1"],
                    "agents_states":{"sub-1":{"status":"pending_init"}}
                }),
                4,
            ),
            make_event(
                "agent.session",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread"
                }),
                5,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let root = &tree.roots[0];
        let spawn_call = match &root.items[1] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected spawn call"),
        };
        let spawn_result = match &spawn_call.children[0] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected spawn result child"),
        };
        let child_thread = match &spawn_call.children[1] {
            TimelineItem::Thread(thread) => thread,
            TimelineItem::Event(_) => panic!("expected child thread under spawn call"),
        };

        assert_eq!(spawn_call.event.event_type, "collab.spawn_agent");
        assert_eq!(
            spawn_result.event.parent_event_id.as_deref(),
            Some("run-1:2")
        );
        assert_eq!(child_thread.thread_id, "sub-1");
        assert_eq!(child_thread.parent_event_id.as_deref(), Some("run-1:2"));
    }

    #[test]
    fn build_event_tree_extracts_receiver_ids_from_nested_input_for_child_anchor() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "collab.spawn_agent",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"spawn_agent",
                    "tool_use_id":"ct-1",
                    "phase":"completed",
                    "status":"completed",
                    "input":{"receiver_thread_ids":["sub-1"]},
                    "agents_states":{"sub-1":{"status":"pending_init"}}
                }),
                2,
            ),
            make_event(
                "agent.session",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread"
                }),
                3,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let root = &tree.roots[0];
        let spawn_call = match &root.items[1] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected spawn event"),
        };
        let child_thread = match &spawn_call.children[0] {
            TimelineItem::Thread(thread) => thread,
            TimelineItem::Event(_) => panic!("expected child thread under spawn event"),
        };

        assert_eq!(child_thread.thread_id, "sub-1");
        assert_eq!(child_thread.parent_event_id.as_deref(), Some("run-1:2"));
    }

    #[test]
    fn build_event_tree_normalizes_receiver_thread_ids_for_child_anchor() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "collab.spawn_agent",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"spawn_agent",
                    "tool_use_id":"ct-1",
                    "phase":"completed",
                    "status":"completed",
                    "receiver_thread_ids":[" sub-1 "],
                    "agents_states":{" sub-1 ":{"status":"pending_init"}}
                }),
                2,
            ),
            make_event(
                "agent.session",
                json!({
                    "actor_type":"subagent",
                    "thread_id":" sub-1 ",
                    "parent_thread_id":" root-thread "
                }),
                3,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let root = &tree.roots[0];
        let spawn_call = match &root.items[1] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected spawn event"),
        };
        let child_thread = match &spawn_call.children[0] {
            TimelineItem::Thread(thread) => thread,
            TimelineItem::Event(_) => panic!("expected child thread under spawn event"),
        };

        assert_eq!(child_thread.thread_id, "sub-1");
        assert_eq!(child_thread.parent_event_id.as_deref(), Some("run-1:2"));
    }

    #[test]
    fn load_records_from_run_input_uses_local_fixture() {
        let temp = tempfile::tempdir().expect("tmpdir should be created");
        let task_dir = temp.path().join("task-demo");
        let run_dir = task_dir.join("runs").join("20260323T000000Z--run-1");
        std::fs::create_dir_all(&run_dir).expect("run dir should be created");
        std::fs::write(
            task_dir.join("task.json"),
            r#"{"task_id":"task-demo"}"#,
        )
        .expect("task json should be written");
        std::fs::write(
            run_dir.join("stdout.jsonl"),
            concat!(
                r#"{"type":"thread.started","thread_id":"root-thread"}"#,
                "\n",
                r#"{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}"#,
                "\n"
            ),
        )
        .expect("stdout should be written");
        std::fs::write(run_dir.join("stderr.log"), "").expect("stderr should be written");
        let run_dir = PathBuf::from(run_dir);
        let input_path = run_dir.clone();

        let (source_path, events) =
            load_records_from_run_input(&input_path).expect("fixture run should replay");

        assert_eq!(source_path, run_dir);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].event_type, "thread.started");
        assert_eq!(events[1].event_type, "message.agent");
    }

    #[test]
    fn load_records_from_run_input_rejects_non_run_paths() {
        let err = load_records_from_run_input(Path::new("/tmp/not-a-run/events.jsonl"))
            .expect_err("non-run path should fail");
        assert!(err.to_string().contains("путь не распознан"));
    }
}
