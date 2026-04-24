use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::events::types::{
    AGENT_ABORTED, AGENT_COMPLETED, AGENT_FAILED, AGENT_META, AGENT_REASONING, AGENT_SESSION,
    AGENT_SESSION_FOREIGN, AGENT_STARTED, COLLAB_CLOSE_AGENT, COLLAB_RESUME_AGENT,
    COLLAB_SEND_INPUT, COLLAB_SPAWN_AGENT, COLLAB_WAIT, CONTEXT_COMPACTED,
    CONTEXT_COMPACTED_DUPLICATE, ERROR, FILE_CHANGE, INFO_TOKENS, MCP_CALL, MCP_RESULT,
    MESSAGE_AGENT, MESSAGE_COMMENTARY, MESSAGE_USER, PATCH_APPLY, PATCH_APPLY_DUPLICATE,
    RAW_UNPARSED, RUNTIME_CONTEXT, SHELL_CALL, SHELL_RESULT, STDERR_LINE, STDIN_WRITE,
    TASK_COMPLETED, TASK_STARTED, TODO_UPDATE, TOOL_CALL, TOOL_RESULT, WEB_OPEN, WEB_SEARCH,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TextLinksPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconnect_attempt: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconnect_limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_fallback: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_code: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheme: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cf_ray: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disconnect_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AgentTurnPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_links: Option<TextLinksPayload>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AgentMessagePayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(default)]
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_links: Option<TextLinksPayload>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AgentSessionPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forked_from_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_nickname: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_role: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AgentMetaPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_path: Option<String>,
    #[serde(default = "default_meta_type")]
    pub meta_type: String,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}

fn default_meta_type() -> String {
    "meta".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct InfoTokensPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_token_usage: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rate_limits: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ToolCallPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_path: Option<String>,
    #[serde(default = "unknown_tool")]
    pub tool_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender_thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receiver_thread_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agents_states: Option<serde_json::Map<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detection_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detection_confidence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_identifiers: Option<Vec<String>>,
}

fn unknown_tool() -> String {
    "unknown".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ToolResultPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_path: Option<String>,
    #[serde(default = "unknown_tool")]
    pub tool_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender_thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receiver_thread_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agents_states: Option<serde_json::Map<String, Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detection_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detection_confidence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skill_identifiers: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct McpCallPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct McpResultPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FileChange {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct FileChangePayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default)]
    pub changes: Vec<FileChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TodoItem {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub completed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct TodoUpdatePayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(default)]
    pub items: Vec<TodoItem>,
    #[serde(default)]
    pub completed_count: u32,
    #[serde(default)]
    pub total_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ErrorPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconnect_attempt: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reconnect_limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_fallback: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_links: Option<TextLinksPayload>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RawPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct StderrPayload {
    #[serde(default)]
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PayloadObject {
    AgentTurn(AgentTurnPayload),
    AgentMessage(AgentMessagePayload),
    AgentSession(AgentSessionPayload),
    AgentMeta(AgentMetaPayload),
    InfoTokens(InfoTokensPayload),
    ToolCall(ToolCallPayload),
    ToolResult(ToolResultPayload),
    McpCall(McpCallPayload),
    McpResult(McpResultPayload),
    FileChange(FileChangePayload),
    TodoUpdate(TodoUpdatePayload),
    Error(ErrorPayload),
    Raw(RawPayload),
    Stderr(StderrPayload),
    Unknown(Value),
}

pub fn payload_to_value<T>(payload: &T) -> Value
where
    T: Serialize,
{
    serde_json::to_value(payload).unwrap_or(Value::Null)
}

pub fn parse_payload(event_type: &str, payload: &Value) -> PayloadObject {
    let Some(obj) = payload.as_object() else {
        return PayloadObject::Unknown(payload.clone());
    };
    let mut normalized = obj.clone();
    if matches!(
        event_type,
        AGENT_STARTED | AGENT_COMPLETED | AGENT_FAILED | MESSAGE_AGENT | AGENT_REASONING | ERROR
    ) {
        if let Some(links) = normalized.get("text_links").and_then(Value::as_object) {
            normalized.insert(
                "text_links".to_string(),
                serde_json::to_value(TextLinksPayload {
                    request_id: links
                        .get("request_id")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    reconnect_attempt: links
                        .get("reconnect_attempt")
                        .and_then(Value::as_u64)
                        .map(|v| v as u32),
                    reconnect_limit: links
                        .get("reconnect_limit")
                        .and_then(Value::as_u64)
                        .map(|v| v as u32),
                    is_fallback: links.get("is_fallback").and_then(Value::as_bool),
                    fallback_from: links
                        .get("fallback_from")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    fallback_to: links
                        .get("fallback_to")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    status_code: links
                        .get("status_code")
                        .and_then(Value::as_u64)
                        .map(|v| v as u16),
                    status_text: links
                        .get("status_text")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    url: links.get("url").and_then(Value::as_str).map(str::to_string),
                    scheme: links
                        .get("scheme")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    host: links
                        .get("host")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    cf_ray: links
                        .get("cf_ray")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                    disconnect_reason: links
                        .get("disconnect_reason")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                })
                .unwrap_or(Value::Null),
            );
        }
    }
    if event_type == "file.change" {
        let mut out = Vec::new();
        if let Some(changes) = normalized.get("changes").and_then(Value::as_array) {
            for change in changes {
                if let Some(change_obj) = change.as_object() {
                    out.push(FileChange {
                        kind: change_obj
                            .get("kind")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        path: change_obj
                            .get("path")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    });
                }
            }
        }
        normalized.insert(
            "changes".to_string(),
            serde_json::to_value(out).unwrap_or(Value::Null),
        );
    }
    if event_type == TODO_UPDATE && !is_tool_lifecycle_todo_update_payload(&normalized) {
        let mut out = Vec::new();
        if let Some(items) = normalized.get("items").and_then(Value::as_array) {
            for item in items {
                if let Some(item_obj) = item.as_object() {
                    out.push(TodoItem {
                        text: item_obj
                            .get("text")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        completed: item_obj
                            .get("completed")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    });
                }
            }
        }
        normalized.insert(
            "items".to_string(),
            serde_json::to_value(out).unwrap_or(Value::Null),
        );
    }

    let value = Value::Object(normalized);
    match event_type {
        AGENT_STARTED | AGENT_COMPLETED | AGENT_FAILED => serde_json::from_value(value)
            .map(PayloadObject::AgentTurn)
            .unwrap_or(PayloadObject::Unknown(payload.clone())),
        MESSAGE_AGENT | AGENT_REASONING | MESSAGE_COMMENTARY | MESSAGE_USER => {
            serde_json::from_value(value)
                .map(PayloadObject::AgentMessage)
                .unwrap_or(PayloadObject::Unknown(payload.clone()))
        }
        AGENT_SESSION => serde_json::from_value(value)
            .map(PayloadObject::AgentSession)
            .unwrap_or(PayloadObject::Unknown(payload.clone())),
        AGENT_SESSION_FOREIGN
        | AGENT_META
        | TASK_STARTED
        | TASK_COMPLETED
        | RUNTIME_CONTEXT
        | CONTEXT_COMPACTED
        | CONTEXT_COMPACTED_DUPLICATE
        | AGENT_ABORTED => serde_json::from_value(value)
            .map(PayloadObject::AgentMeta)
            .unwrap_or(PayloadObject::Unknown(payload.clone())),
        INFO_TOKENS => serde_json::from_value(value)
            .map(PayloadObject::InfoTokens)
            .unwrap_or(PayloadObject::Unknown(payload.clone())),
        TOOL_CALL | SHELL_CALL => serde_json::from_value(value)
            .map(PayloadObject::ToolCall)
            .unwrap_or(PayloadObject::Unknown(payload.clone())),
        MCP_CALL => serde_json::from_value(value)
            .map(PayloadObject::McpCall)
            .unwrap_or(PayloadObject::Unknown(payload.clone())),
        MCP_RESULT => serde_json::from_value(value)
            .map(PayloadObject::McpResult)
            .unwrap_or(PayloadObject::Unknown(payload.clone())),
        TOOL_RESULT
        | SHELL_RESULT
        | STDIN_WRITE
        | WEB_SEARCH
        | WEB_OPEN
        | PATCH_APPLY
        | PATCH_APPLY_DUPLICATE
        | COLLAB_SPAWN_AGENT
        | COLLAB_SEND_INPUT
        | COLLAB_WAIT
        | COLLAB_CLOSE_AGENT
        | COLLAB_RESUME_AGENT => serde_json::from_value(value)
            .map(PayloadObject::ToolResult)
            .unwrap_or(PayloadObject::Unknown(payload.clone())),
        FILE_CHANGE => serde_json::from_value(value)
            .map(PayloadObject::FileChange)
            .unwrap_or(PayloadObject::Unknown(payload.clone())),
        TODO_UPDATE => {
            let is_tool_lifecycle = value
                .as_object()
                .map(is_tool_lifecycle_todo_update_payload)
                .unwrap_or(false);
            if is_tool_lifecycle {
                serde_json::from_value(value)
                    .map(PayloadObject::ToolResult)
                    .unwrap_or(PayloadObject::Unknown(payload.clone()))
            } else {
                serde_json::from_value(value)
                    .map(PayloadObject::TodoUpdate)
                    .unwrap_or(PayloadObject::Unknown(payload.clone()))
            }
        }
        ERROR => serde_json::from_value(value)
            .map(PayloadObject::Error)
            .unwrap_or(PayloadObject::Unknown(payload.clone())),
        RAW_UNPARSED => serde_json::from_value(value)
            .map(PayloadObject::Raw)
            .unwrap_or(PayloadObject::Unknown(payload.clone())),
        STDERR_LINE => serde_json::from_value(value)
            .map(PayloadObject::Stderr)
            .unwrap_or(PayloadObject::Unknown(payload.clone())),
        _ => PayloadObject::Unknown(payload.clone()),
    }
}

fn is_tool_lifecycle_todo_update_payload(payload: &serde_json::Map<String, Value>) -> bool {
    payload
        .get("tool_name")
        .and_then(Value::as_str)
        .map(|tool_name| tool_name == "update_plan")
        .unwrap_or(false)
        || payload.contains_key("tool_use_id")
        || payload.contains_key("input")
        || payload.contains_key("output")
}
