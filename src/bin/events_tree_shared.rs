use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::BufRead;
use std::path::{Path, PathBuf};

use codex_worker_rs::error::{AppError, AppResult};
use codex_worker_rs::events::projector::{
    categorize_event, load_event_records, summarize_event_full, EventProjector,
    EventSummaryCategory,
};
use codex_worker_rs::events::readers::{JsonOutputEventReader, RunEventContext};
use codex_worker_rs::events::replay::ReplayedRunStream;
use codex_worker_rs::events::types::{
    AGENT_ABORTED, AGENT_COMPLETED, AGENT_FAILED, AGENT_META, AGENT_REASONING, AGENT_SESSION,
    AGENT_SESSION_FOREIGN, AGENT_STARTED, COLLAB_CLOSE_AGENT, COLLAB_RESUME_AGENT,
    COLLAB_SEND_INPUT, COLLAB_SPAWN_AGENT, COLLAB_WAIT, CONTEXT_COMPACTED,
    CONTEXT_COMPACTED_DUPLICATE, FILE_CHANGE, INFO_TOKENS, MCP_CALL, MCP_RESULT,
    MESSAGE_COMMENTARY, MESSAGE_USER, PATCH_APPLY, PATCH_APPLY_DUPLICATE, PLAN_UPDATE,
    RUNTIME_CONTEXT, SHELL_CALL, SHELL_RESULT, STDERR_LINE, STDIN_WRITE, TASK_COMPLETED,
    TASK_STARTED, THREAD_STARTED, TODO_UPDATE, TOOL_CALL, TOOL_RESULT, WEB_OPEN, WEB_SEARCH,
};
use codex_worker_rs::models::EventRecord;
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
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
    pub summary: String,
    pub category: EventSummaryCategory,
    pub meta_type: Option<String>,
    pub tool_name: Option<String>,
    pub receiver_thread_ids: Vec<String>,
    pub operation_id: Option<String>,
    pub phase: Option<String>,
    pub aggregated_output: Option<String>,
    pub shell_command: Option<String>,
    pub shell_exit_code: Option<i32>,
    pub summary_pairs: Vec<(String, String)>,
    pub input_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub reasoning_output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventNode {
    pub event: EventEntry,
    pub children: Vec<TimelineItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimelineItem {
    Event(EventNode),
    Thread(ThreadNode),
}

#[derive(Debug, Clone, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq)]
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
        .map(|event| event.run_id.as_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("-")
        .to_string();

    let mut projector = EventProjector::new(events.len().max(1), 8);
    projector.apply_events(events.iter());

    let root_thread_id = projector
        .snapshot
        .root_thread_id
        .clone()
        .or_else(|| events.iter().find_map(|event| event_thread_id(event, None)))
        .unwrap_or_else(|| "root".to_string());

    let mut events_by_thread: BTreeMap<String, Vec<EventEntry>> = BTreeMap::new();
    let mut orphan_events = Vec::new();
    for event in events {
        let entry = format_event_entry(event, &projector, root_thread_id.as_str());
        if let Some(thread_id) = event_thread_id(event, Some(root_thread_id.as_str())) {
            events_by_thread.entry(thread_id).or_default().push(entry);
        } else {
            orphan_events.push(entry);
        }
    }

    let mut all_thread_ids = BTreeSet::new();
    all_thread_ids.insert(root_thread_id.clone());
    all_thread_ids.extend(projector.snapshot.agents.keys().cloned());
    all_thread_ids.extend(events_by_thread.keys().cloned());

    let first_seq_by_thread = first_seq_by_thread(&all_thread_ids, &events_by_thread);
    let (mut root_threads, mut children_by_parent) = build_thread_index(
        &all_thread_ids,
        &projector,
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
                &projector,
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
                    .map(str::trim)
                    .filter(|id| !id.is_empty())
                {
                    out.insert(thread_id.to_string());
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
    projector: &EventProjector,
    events_by_thread: &BTreeMap<String, Vec<EventEntry>>,
    children_by_parent: &HashMap<String, Vec<String>>,
) -> FlatThreadNode {
    let agent = projector.snapshot.agents.get(thread_id);
    let children = children_by_parent
        .get(thread_id)
        .into_iter()
        .flatten()
        .map(|child_thread_id| {
            build_flat_thread_node(
                child_thread_id,
                root_thread_id,
                projector,
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
    let mut started_by_operation: HashMap<(String, String), String> = HashMap::new();

    for event in events.iter_mut() {
        let Some(operation_kind) = operation_kind(event) else {
            continue;
        };
        let Some(operation_id) = event.operation_id.clone() else {
            continue;
        };
        let key = (operation_kind.to_string(), operation_id);
        let phase = event.phase.as_deref().unwrap_or_default();

        if is_operation_start(event) {
            started_by_operation.insert(key, event.event_id.clone());
            continue;
        }

        if matches!(phase, "completed" | "updated")
            || matches!(
                event.event_type.as_str(),
                TOOL_RESULT
                    | SHELL_RESULT
                    | MCP_RESULT
                    | STDIN_WRITE
                    | FILE_CHANGE
                    | TODO_UPDATE
                    | WEB_SEARCH
                    | WEB_OPEN
                    | COLLAB_SPAWN_AGENT
                    | COLLAB_WAIT
            )
        {
            if let Some(parent_event_id) = started_by_operation.get(&key) {
                event.parent_event_id = Some(parent_event_id.clone());
            }
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
            | PLAN_UPDATE
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
    match event.event_type.as_str() {
        AGENT_REASONING | MESSAGE_COMMENTARY | STDERR_LINE => true,
        AGENT_META => matches!(
            event.meta_type.as_deref(),
            Some("message" | "task_complete" | "user_message")
        ),
        _ => false,
    }
}

fn operation_kind(event: &EventEntry) -> Option<&'static str> {
    match event.event_type.as_str() {
        TOOL_CALL | TOOL_RESULT => Some("tool"),
        SHELL_CALL | SHELL_RESULT => Some("shell"),
        MCP_CALL | MCP_RESULT => Some("mcp"),
        STDIN_WRITE => Some("stdin.write"),
        WEB_SEARCH => Some("web.search"),
        WEB_OPEN => Some("web.open"),
        PLAN_UPDATE => Some("plan.update"),
        PATCH_APPLY => Some("patch.apply"),
        COLLAB_SPAWN_AGENT => Some("collab.spawn_agent"),
        COLLAB_SEND_INPUT => Some("collab.send_input"),
        COLLAB_WAIT => Some("collab.wait"),
        COLLAB_CLOSE_AGENT => Some("collab.close_agent"),
        COLLAB_RESUME_AGENT => Some("collab.resume_agent"),
        FILE_CHANGE => Some("file.change"),
        TODO_UPDATE => Some("todo.update"),
        _ => None,
    }
}

fn is_operation_start(event: &EventEntry) -> bool {
    match event.event_type.as_str() {
        TOOL_CALL | SHELL_CALL | MCP_CALL => true,
        STDIN_WRITE | WEB_SEARCH | WEB_OPEN | PLAN_UPDATE | PATCH_APPLY | COLLAB_SPAWN_AGENT
        | COLLAB_SEND_INPUT | COLLAB_WAIT | COLLAB_CLOSE_AGENT | COLLAB_RESUME_AGENT => {
            event.phase.as_deref() == Some("started")
        }
        FILE_CHANGE | TODO_UPDATE => event.phase.as_deref() == Some("started"),
        _ => false,
    }
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

fn format_event_entry(
    event: &EventRecord,
    projector: &EventProjector,
    root_thread_id: &str,
) -> EventEntry {
    let payload = event.payload.as_object();
    let actor_type = actor_type(event).map(str::to_string);
    let thread_id = event_thread_id(event, Some(root_thread_id));
    let subagent_nickname = if actor_type.as_deref() == Some("subagent") {
        thread_id.as_ref().and_then(|thread_id| {
            projector
                .snapshot
                .agents
                .get(thread_id)
                .and_then(|agent| agent.nickname.clone())
        })
    } else {
        None
    };
    EventEntry {
        event_id: format!("{}:{}", event.run_id, event.seq),
        parent_event_id: None,
        seq: event.seq,
        ts: event.ts.clone(),
        actor_type,
        thread_id,
        subagent_nickname,
        event_type: event.event_type.clone(),
        raw_type: event.raw_type.clone(),
        parse_status: event.parse_status.clone(),
        summary: summarize_event_full(event),
        category: categorize_event(event),
        meta_type: payload
            .and_then(|obj| obj.get("meta_type"))
            .and_then(Value::as_str)
            .map(str::to_string),
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
        operation_id: payload
            .and_then(|obj| obj.get("tool_use_id"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .or_else(|| {
                payload
                    .and_then(|obj| obj.get("item_id"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }),
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
        shell_command: extract_shell_command(&event.event_type, payload),
        shell_exit_code: extract_shell_exit_code(&event.event_type, payload),
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

fn extract_shell_command(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> Option<String> {
    if !is_command_shell_result(event_type, payload) {
        return None;
    }

    let input = payload.and_then(|obj| obj.get("input")).and_then(Value::as_object)?;
    for key in ["command", "cmd"] {
        let Some(value) = input.get(key).and_then(Value::as_str) else {
            continue;
        };
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    None
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

fn is_command_shell_result(
    event_type: &str,
    payload: Option<&serde_json::Map<String, Value>>,
) -> bool {
    if event_type != SHELL_RESULT {
        return false;
    }

    matches!(
        payload
            .and_then(|obj| obj.get("tool_name"))
            .and_then(Value::as_str),
        Some("command_execution" | "exec_command")
    )
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

fn event_thread_id(event: &EventRecord, root_thread_id: Option<&str>) -> Option<String> {
    let payload = event.payload.as_object();
    payload
        .and_then(|obj| obj.get("thread_id"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            payload
                .and_then(|obj| obj.get("sender_thread_id"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .or_else(|| match actor_type(event).as_deref() {
            Some("agent") => root_thread_id.map(str::to_string),
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
    projector: &EventProjector,
    first_seq_by_thread: &HashMap<String, u64>,
    root_thread_id: &str,
) -> (Vec<String>, HashMap<String, Vec<String>>) {
    let mut root_threads = Vec::new();
    let mut children_by_parent: HashMap<String, Vec<String>> = HashMap::new();

    for thread_id in all_thread_ids {
        let parent_thread_id = projector
            .snapshot
            .agents
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
    use codex_worker_rs::models::EventRecord;

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
            web_search_completed.event.parent_event_id.as_deref(),
            Some("run-1:2")
        );
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
            patch_completed.event.parent_event_id.as_deref(),
            Some("run-1:2")
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
                "plan.update",
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
                "plan.update",
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

        assert_eq!(plan_update_started.event.event_type, "plan.update");
        assert_eq!(plan_update_completed.event.event_type, "plan.update");
        assert_eq!(
            plan_update_completed.event.parent_event_id.as_deref(),
            Some("run-1:2")
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
    fn load_records_from_run_input_replays_fixture_run() {
        let run_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
            "target/manual-smoke/.codex-worker/tasks/all-operation-emulation--07f4203b/runs/20260406T145552Z--18a3cc56f6037e36-1",
        );
        let input_path = run_dir.join("events.jsonl");

        let (source_path, events) =
            load_records_from_run_input(&input_path).expect("fixture run should replay");

        assert_eq!(source_path, run_dir);
        assert!(!events.is_empty());
        assert_eq!(
            events.first().map(|event| event.task_id.as_str()),
            Some("all-operation-emulation")
        );
    }

    #[test]
    fn load_records_from_run_input_rejects_non_run_paths() {
        let err = load_records_from_run_input(Path::new("/tmp/not-a-run/events.jsonl"))
            .expect_err("non-run path should fail");
        assert!(err.to_string().contains("путь не распознан"));
    }
}
