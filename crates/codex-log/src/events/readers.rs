use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;
use serde_json::{Map, Value};

use crate::events::payloads::{
    payload_to_value, AgentMessagePayload, AgentTurnPayload, ErrorPayload, FileChange,
    FileChangePayload, InfoTokensPayload, McpCallPayload, McpResultPayload, RawPayload,
    TextLinksPayload, TodoItem, TodoUpdatePayload, ToolCallPayload, ToolResultPayload,
};
use crate::events::record::{CodexEvent, EventRecord};
use crate::events::types::{
    AGENT_ABORTED, AGENT_COMPLETED, AGENT_FAILED, AGENT_SESSION_FOREIGN, AGENT_STARTED,
    COLLAB_CLOSE_AGENT, COLLAB_RESUME_AGENT, COLLAB_SEND_INPUT, COLLAB_SPAWN_AGENT, COLLAB_WAIT,
    CONTEXT_COMPACTED, CONTEXT_COMPACTED_DUPLICATE, INFO_TOKENS, MCP_CALL, MCP_RESULT,
    MESSAGE_AGENT, MESSAGE_USER, PATCH_APPLY, PATCH_APPLY_DUPLICATE, RAW_UNPARSED, RUNTIME_CONTEXT,
    SHELL_CALL, SHELL_RESULT, STDIN_WRITE, TASK_COMPLETED, TASK_STARTED, THREAD_STARTED,
    TODO_UPDATE, TOOL_CALL, TOOL_RESULT, USER_INPUT_REQUEST, WEB_OPEN, WEB_SEARCH,
};
use crate::util::utc_now_iso;

fn request_id_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\brequest ID ([0-9a-fA-F-]{8,})\b").expect("regex"))
}
fn reconnect_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"Reconnecting\.\.\.\s*(\d+)\s*/\s*(\d+)").expect("regex"))
}
fn status_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\bstatus\s+(\d{3})\s+([A-Za-z]+)\b").expect("regex"))
}
fn url_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\burl:\s*(https?://[^\s,)]+|wss?://[^\s,)]+)").expect("regex"))
}
fn cf_ray_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)\bcf-ray:\s*([^\s,)]+)").expect("regex"))
}
fn disconnect_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)^(stream disconnected before completion)\b").expect("regex"))
}
fn fallback_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)Falling back from ([A-Za-z]+)s? to ([A-Za-z]+) transport").expect("regex")
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunEventContext {
    pub task_id: String,
    pub run_id: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JsonOutputState {
    pub valid_json: bool,
    pub saw_turn_completed: bool,
    pub saw_turn_failed: bool,
    pub saw_thread_started: bool,
    pub saw_top_error: bool,
    pub top_errors: Vec<Value>,
    pub item_errors: Vec<Value>,
    pub last_terminal: Option<String>,
    pub final_agent_message: Option<String>,
    pub current_turn_messages: Vec<String>,
    pub turn_open: bool,
    pub thread_id: Option<String>,
    pub patch_apply_end_call_ids: HashSet<String>,
    pub pending_context_compacted_duplicate: bool,
    pub last_subagent_message: Option<RecentSubagentMessage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecentSubagentMessageSource {
    ResponseItemMessage,
    EventMsgMessage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentSubagentMessage {
    pub role: String,
    pub phase: Option<String>,
    pub text: String,
    pub source: RecentSubagentMessageSource,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportedSubagentSessionMeta {
    pub parent_thread_id: Option<String>,
    pub forked_from_id: Option<String>,
}

impl JsonOutputState {
    pub fn new() -> Self {
        Self {
            valid_json: true,
            ..Self::default()
        }
    }
}

pub struct JsonOutputEventReader {
    context: RunEventContext,
    pub state: JsonOutputState,
}

mod session;
mod stdout;

pub use self::session::imported_subagent_session_meta;

impl JsonOutputEventReader {
    pub fn new(context: RunEventContext, state: Option<JsonOutputState>) -> Self {
        Self {
            context,
            state: state.unwrap_or_else(JsonOutputState::new),
        }
    }

    fn build_error_payload(&self, message: Option<&str>, error_type: Option<&str>) -> ErrorPayload {
        let text_links = message.and_then(|text| self.extract_text_links(text));
        let mut payload = ErrorPayload {
            actor_type: Some("agent".to_string()),
            thread_id: self.state.thread_id.clone(),
            message: message.map(str::to_string),
            error_type: error_type.map(str::to_string),
            text_links: text_links.clone(),
            ..ErrorPayload::default()
        };
        if let Some(links) = text_links {
            payload.request_id = links.request_id.clone();
            payload.reconnect_attempt = links.reconnect_attempt;
            payload.reconnect_limit = links.reconnect_limit;
            payload.is_fallback = links.is_fallback;
            payload.fallback_from = links.fallback_from.clone();
            payload.fallback_to = links.fallback_to.clone();
        }
        payload
    }

    fn extract_text_links(&self, text: &str) -> Option<TextLinksPayload> {
        if text.trim().is_empty() {
            return None;
        }

        let mut links = TextLinksPayload::default();

        if let Some(caps) = request_id_re().captures(text) {
            links.request_id = Some(caps[1].to_string());
        }
        if let Some(caps) = reconnect_re().captures(text) {
            links.reconnect_attempt = caps[1].parse::<u32>().ok();
            links.reconnect_limit = caps[2].parse::<u32>().ok();
        }
        if let Some(caps) = status_re().captures(text) {
            links.status_code = caps[1].parse::<u16>().ok();
            links.status_text = Some(caps[2].to_string());
        }
        if let Some(caps) = url_re().captures(text) {
            let url = caps[1].to_string();
            links.url = Some(url.clone());
            if let Some((scheme, host)) = parse_url_parts(&url) {
                links.scheme = Some(scheme);
                links.host = Some(host);
            }
        }
        if let Some(caps) = cf_ray_re().captures(text) {
            links.cf_ray = Some(caps[1].to_string());
        }
        if let Some(caps) = disconnect_re().captures(text) {
            links.disconnect_reason = Some(caps[1].to_string());
        }
        if let Some(caps) = fallback_re().captures(text) {
            links.is_fallback = Some(true);
            links.fallback_from = Some(normalize_transport_name(&caps[1]));
            links.fallback_to = Some(normalize_transport_name(&caps[2]));
        }

        if payload_to_value(&links) == Value::Object(Map::new()) {
            return None;
        }
        Some(links)
    }
}

pub struct EventLogFileReader;

impl EventLogFileReader {
    pub fn load_events(path: &Path) -> std::io::Result<Vec<CodexEvent>> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let handle = File::open(path)?;
        let reader = BufReader::new(handle);
        let mut events: Vec<CodexEvent> = Vec::new();
        for line in reader.lines() {
            let raw_line = line?;
            let trimmed = raw_line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let value: Value = match serde_json::from_str(trimmed) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let Some(obj) = value.as_object() else {
                continue;
            };
            let record = EventRecord::from_dict(obj);
            events.push(CodexEvent::from_record(record, "event_log"));
        }
        Ok(events)
    }

    pub fn load_records(path: &Path) -> std::io::Result<Vec<EventRecord>> {
        Ok(Self::load_events(path)?
            .into_iter()
            .map(|event| event.to_record())
            .collect())
    }
}

fn increment(counter: &mut HashMap<String, u64>, key: &str) {
    *counter.entry(key.to_string()).or_insert(0) += 1;
}

fn normalized_tool_event_type(tool_name: &str, is_result: bool) -> String {
    if tool_name == "update_plan" {
        TODO_UPDATE.to_string()
    } else if tool_name == "request_user_input" {
        USER_INPUT_REQUEST.to_string()
    } else if tool_name == "write_stdin" {
        STDIN_WRITE.to_string()
    } else if let Some(collab_event_type) = collab_event_type(tool_name) {
        collab_event_type.to_string()
    } else if is_shell_tool(tool_name) {
        if is_result { SHELL_RESULT } else { SHELL_CALL }.to_string()
    } else if is_result {
        TOOL_RESULT.to_string()
    } else {
        TOOL_CALL.to_string()
    }
}

fn custom_tool_output_status(output: Option<&Value>) -> Option<String> {
    let text = output
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default();
    if text.is_empty() {
        return Some("completed".to_string());
    }
    let lowercase = text.to_ascii_lowercase();
    if lowercase.contains("failed") || lowercase.contains("error") {
        Some("failed".to_string())
    } else {
        Some("completed".to_string())
    }
}

fn parse_embedded_json_value(value: Option<&Value>) -> Value {
    match value {
        Some(Value::String(text)) => {
            serde_json::from_str::<Value>(text).unwrap_or_else(|_| Value::String(text.to_string()))
        }
        Some(other) => other.clone(),
        None => Value::Null,
    }
}

fn codex_mcp_server(tool_name: &str) -> Option<&'static str> {
    match tool_name {
        "list_mcp_resources" | "list_mcp_resource_templates" | "read_mcp_resource" => Some("codex"),
        _ => None,
    }
}

fn is_shell_tool(tool_name: &str) -> bool {
    matches!(tool_name, "command_execution" | "exec_command")
}

fn enrich_skill_usage_payload(payload: &mut Value) {
    let Some(payload_obj) = payload.as_object_mut() else {
        return;
    };
    let identifiers = extract_skill_identifiers_from_value(&Value::Object(payload_obj.clone()));
    if identifiers.is_empty() {
        return;
    }
    payload_obj.insert(
        "skill_identifiers".to_string(),
        Value::Array(identifiers.into_iter().map(Value::from).collect()),
    );
}

fn extract_skill_identifiers_from_value(value: &Value) -> Vec<String> {
    fn walk(value: &Value, out: &mut BTreeSet<String>) {
        match value {
            Value::String(text) => {
                for token in text.split(|ch: char| {
                    ch.is_whitespace() || matches!(ch, '"' | '\'' | ',' | ';' | '(' | ')')
                }) {
                    if let Some(identifier) = skill_identifier_from_candidate(token) {
                        out.insert(identifier);
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    walk(item, out);
                }
            }
            Value::Object(obj) => {
                for item in obj.values() {
                    walk(item, out);
                }
            }
            _ => {}
        }
    }

    let mut out = BTreeSet::new();
    walk(value, &mut out);
    out.into_iter().collect()
}

fn skill_identifier_from_candidate(candidate: &str) -> Option<String> {
    let trimmed = candidate
        .trim_matches(|ch: char| matches!(ch, '"' | '\'' | '[' | ']' | '{' | '}'))
        .trim();
    if !trimmed.ends_with("SKILL.md") {
        return None;
    }
    Path::new(trimmed)
        .parent()
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
}

fn collab_event_type(tool_name: &str) -> Option<&'static str> {
    match tool_name {
        "spawn_agent" => Some(COLLAB_SPAWN_AGENT),
        "send_input" => Some(COLLAB_SEND_INPUT),
        "wait" | "wait_agent" => Some(COLLAB_WAIT),
        "close_agent" => Some(COLLAB_CLOSE_AGENT),
        "resume_agent" => Some(COLLAB_RESUME_AGENT),
        _ => None,
    }
}

fn is_singleton_tool_event_type(event_type: &str) -> bool {
    matches!(
        event_type,
        TODO_UPDATE
            | USER_INPUT_REQUEST
            | STDIN_WRITE
            | COLLAB_SPAWN_AGENT
            | COLLAB_SEND_INPUT
            | COLLAB_WAIT
            | COLLAB_CLOSE_AGENT
            | COLLAB_RESUME_AGENT
    )
}

fn parse_url_parts(url: &str) -> Option<(String, String)> {
    let (scheme, rest) = url.split_once("://")?;
    let host = rest
        .split('/')
        .next()
        .unwrap_or_default()
        .trim()
        .to_string();
    if host.is_empty() {
        return None;
    }
    Some((scheme.to_string(), host))
}

fn normalize_transport_name(value: &str) -> String {
    let normalized = value.trim().to_lowercase();
    if matches!(
        normalized.as_str(),
        "websocket" | "websockets" | "ws" | "wss"
    ) {
        return "websocket".to_string();
    }
    if matches!(normalized.as_str(), "https" | "http") {
        return normalized;
    }
    normalized
}

fn is_subagent_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "spawn_agent" | "send_input" | "wait" | "wait_agent" | "close_agent" | "resume_agent"
    )
}

fn is_collab_high_confidence_tool(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "spawn_agent" | "send_input" | "wait" | "wait_agent" | "close_agent" | "resume_agent"
    )
}

fn is_open_page_action(action: Option<&Value>) -> bool {
    match action {
        Some(Value::String(value)) => value.trim() == "open_page",
        Some(Value::Object(value)) => value
            .get("type")
            .and_then(Value::as_str)
            .map(|value| value.trim() == "open_page")
            .unwrap_or(false),
        _ => false,
    }
}

fn response_item_phase(item: &Map<String, Value>) -> String {
    if let Some(phase) = item.get("phase").and_then(Value::as_str) {
        let phase = phase.trim();
        if !phase.is_empty() {
            return phase.to_string();
        }
    }

    match item.get("status").and_then(Value::as_str) {
        Some("started" | "in_progress" | "running") => "started".to_string(),
        Some("updated") => "updated".to_string(),
        Some("completed" | "failed" | "error" | "cancelled") => "completed".to_string(),
        _ => "completed".to_string(),
    }
}
