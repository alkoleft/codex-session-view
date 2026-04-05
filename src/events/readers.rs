use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;
use serde_json::{Map, Value};

use crate::events::payloads::{
    payload_to_value, AgentMessagePayload, AgentTurnPayload, ErrorPayload, FileChange,
    FileChangePayload, RawPayload, TextLinksPayload, TodoItem, TodoUpdatePayload, ToolCallPayload,
    ToolResultPayload,
};
use crate::events::record::{CodexEvent, EventRecord};
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

impl JsonOutputEventReader {
    pub fn new(context: RunEventContext, state: Option<JsonOutputState>) -> Self {
        Self {
            context,
            state: state.unwrap_or_else(JsonOutputState::new),
        }
    }

    pub fn parse_main_output_line(
        &mut self,
        seq: u64,
        line: &str,
        tool_counts: &mut HashMap<String, u64>,
        subagent_counts: &mut HashMap<String, u64>,
        subagent_threads: &mut HashSet<String>,
    ) -> (u64, CodexEvent) {
        let next_seq = seq + 1;
        let stripped = line.trim_end_matches('\n');
        let ts = utc_now_iso();
        let parsed = serde_json::from_str::<Value>(stripped);
        let Some(parsed_obj) = parsed.ok().and_then(|v| v.as_object().cloned()) else {
            self.state.valid_json = false;
            let event = CodexEvent {
                schema_version: 1,
                ts,
                task_id: self.context.task_id.clone(),
                run_id: self.context.run_id.clone(),
                seq: next_seq,
                event_type: "raw.unparsed".to_string(),
                raw_type: "invalid_json".to_string(),
                parse_status: "unparsed".to_string(),
                payload: payload_to_value(&RawPayload {
                    text: Some(stripped.to_string()),
                    ..RawPayload::default()
                }),
                source: "json_output".to_string(),
            };
            return (next_seq, event);
        };

        let raw_type = parsed_obj
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let mut event_type = "raw.unparsed".to_string();
        let mut payload = payload_to_value(&RawPayload {
            actor_type: Some("agent".to_string()),
            thread_id: self.state.thread_id.clone(),
            raw: Some(Value::Object(parsed_obj.clone())),
            ..RawPayload::default()
        });
        let mut parse_status = "best_effort".to_string();

        match raw_type.as_str() {
            "thread.started" => {
                self.state.saw_thread_started = true;
                self.state.thread_id = parsed_obj
                    .get("thread_id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                event_type = "thread.started".to_string();
                parse_status = "parsed".to_string();
                payload = Value::Object(Map::from_iter([
                    (
                        "thread_id".to_string(),
                        parsed_obj.get("thread_id").cloned().unwrap_or(Value::Null),
                    ),
                    (
                        "model".to_string(),
                        parsed_obj.get("model").cloned().unwrap_or(Value::Null),
                    ),
                ]));
            }
            "turn.started" => {
                self.state.turn_open = true;
                self.state.current_turn_messages.clear();
                event_type = "agent.turn.started".to_string();
                parse_status = "parsed".to_string();
                payload = payload_to_value(&AgentTurnPayload {
                    actor_type: Some("agent".to_string()),
                    thread_id: self.state.thread_id.clone(),
                    ..AgentTurnPayload::default()
                });
            }
            "turn.completed" => {
                self.state.saw_turn_completed = true;
                self.state.last_terminal = Some("turn.completed".to_string());
                self.state.turn_open = false;
                if let Some(last) = self.state.current_turn_messages.last() {
                    self.state.final_agent_message = Some(last.clone());
                }
                event_type = "agent.turn.completed".to_string();
                parse_status = "parsed".to_string();
                payload = payload_to_value(&AgentTurnPayload {
                    actor_type: Some("agent".to_string()),
                    thread_id: self.state.thread_id.clone(),
                    usage: parsed_obj.get("usage").cloned(),
                    result: parsed_obj.get("result").cloned(),
                    ..AgentTurnPayload::default()
                });
            }
            "turn.failed" => {
                self.state.saw_turn_failed = true;
                self.state.last_terminal = Some("turn.failed".to_string());
                self.state.turn_open = false;
                let turn_message = parsed_obj.get("message").and_then(Value::as_str);
                let mut error_message = turn_message.map(str::to_string);
                if error_message.is_none() {
                    error_message = parsed_obj
                        .get("error")
                        .and_then(Value::as_object)
                        .and_then(|v| v.get("message"))
                        .and_then(Value::as_str)
                        .map(str::to_string);
                }
                event_type = "agent.turn.failed".to_string();
                parse_status = "parsed".to_string();
                payload = payload_to_value(&AgentTurnPayload {
                    actor_type: Some("agent".to_string()),
                    thread_id: self.state.thread_id.clone(),
                    error: parsed_obj.get("error").cloned(),
                    message: turn_message.map(str::to_string),
                    text_links: error_message.and_then(|msg| self.extract_text_links(&msg)),
                    ..AgentTurnPayload::default()
                });
            }
            "error" => {
                self.state.saw_top_error = true;
                self.state.top_errors.push(Value::Object(parsed_obj.clone()));
                event_type = "error".to_string();
                parse_status = "parsed".to_string();
                payload = payload_to_value(
                    &self.build_error_payload(
                        parsed_obj.get("message").and_then(Value::as_str),
                        parsed_obj.get("type").and_then(Value::as_str),
                    ),
                );
            }
            "item.started" | "item.updated" | "item.completed" => {
                if let Some(item) = parsed_obj.get("item").and_then(Value::as_object) {
                    let (parsed_event_type, parsed_payload, status) = self.parse_item_event(
                        item,
                        &raw_type,
                        tool_counts,
                        subagent_counts,
                        subagent_threads,
                    );
                    if parsed_event_type == "error" {
                        self.state.item_errors.push(parsed_payload.clone());
                    }
                    if parsed_event_type == "agent.message" {
                        if let Some(text) = parsed_payload.get("text").and_then(Value::as_str) {
                            if !text.trim().is_empty() {
                                if self.state.turn_open {
                                    self.state.current_turn_messages.push(text.to_string());
                                } else if self.state.last_terminal.as_deref() == Some("turn.completed") {
                                    self.state.final_agent_message = Some(text.to_string());
                                }
                            }
                        }
                    }
                    event_type = parsed_event_type;
                    payload = parsed_payload;
                    parse_status = status;
                }
            }
            _ => {}
        }

        (
            next_seq,
            CodexEvent {
                schema_version: 1,
                ts,
                task_id: self.context.task_id.clone(),
                run_id: self.context.run_id.clone(),
                seq: next_seq,
                event_type,
                raw_type,
                parse_status,
                payload,
                source: "json_output".to_string(),
            },
        )
    }

    pub fn parse_subagent_session_payload(
        &mut self,
        seq: u64,
        parsed: &Map<String, Value>,
        imported_path: &Path,
        parent_thread_id: &str,
        thread_id: &str,
        call_names: &mut HashMap<String, String>,
        tool_counts: &mut HashMap<String, u64>,
        subagent_counts: &mut HashMap<String, u64>,
    ) -> Option<CodexEvent> {
        let ts = parsed
            .get("timestamp")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(utc_now_iso);
        let raw_type = parsed
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();

        let mut event_type = "raw.unparsed".to_string();
        let mut payload = payload_to_value(&RawPayload {
            actor_type: Some("subagent".to_string()),
            thread_id: Some(thread_id.to_string()),
            raw: Some(Value::Object(parsed.clone())),
            ..RawPayload::default()
        });
        let mut parse_status = "best_effort".to_string();

        match raw_type.as_str() {
            "session_meta" => {
                let Some(meta) = parsed.get("payload").and_then(Value::as_object) else {
                    return Some(self.build_subagent_event(
                        seq,
                        &ts,
                        event_type,
                        raw_type,
                        parse_status,
                        payload,
                    ));
                };
                let meta_thread_id = meta
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or(thread_id)
                    .to_string();
                if meta_thread_id != thread_id {
                    return None;
                }
                let source_parent = meta
                    .get("source")
                    .and_then(Value::as_object)
                    .and_then(|source| source.get("subagent"))
                    .and_then(Value::as_object)
                    .and_then(|subagent| subagent.get("thread_spawn"))
                    .and_then(Value::as_object)
                    .and_then(|spawn| spawn.get("parent_thread_id"))
                    .and_then(Value::as_str)
                    .map(str::to_string);

                event_type = "agent.session".to_string();
                parse_status = "parsed".to_string();
                payload = Value::Object(Map::from_iter([
                    ("actor_type".to_string(), Value::from("subagent")),
                    ("thread_id".to_string(), Value::from(meta_thread_id)),
                    (
                        "parent_thread_id".to_string(),
                        Value::from(source_parent.unwrap_or_else(|| parent_thread_id.to_string())),
                    ),
                    (
                        "forked_from_id".to_string(),
                        meta.get("forked_from_id").cloned().unwrap_or(Value::Null),
                    ),
                    ("cwd".to_string(), meta.get("cwd").cloned().unwrap_or(Value::Null)),
                    (
                        "agent_nickname".to_string(),
                        meta.get("agent_nickname").cloned().unwrap_or(Value::Null),
                    ),
                    (
                        "agent_role".to_string(),
                        meta.get("agent_role").cloned().unwrap_or(Value::Null),
                    ),
                    (
                        "session_path".to_string(),
                        Value::from(imported_path.display().to_string()),
                    ),
                ]));
            }
            "response_item" => {
                let Some(item) = parsed.get("payload").and_then(Value::as_object) else {
                    return Some(self.build_subagent_event(
                        seq,
                        &ts,
                        event_type,
                        raw_type,
                        parse_status,
                        payload,
                    ));
                };
                let item_type = item
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                match item_type.as_str() {
                    "function_call" => {
                        let name = item
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown")
                            .to_string();
                        let call_id = item
                            .get("call_id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        if !call_id.is_empty() {
                            call_names.insert(call_id.clone(), name.clone());
                        }
                        increment(tool_counts, &name);
                        if is_subagent_tool(&name) {
                            increment(subagent_counts, &name);
                        }

                        let parsed_arguments = item
                            .get("arguments")
                            .and_then(Value::as_str)
                            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                            .or_else(|| item.get("arguments").cloned())
                            .unwrap_or(Value::Null);

                        event_type = "tool.call".to_string();
                        parse_status = "parsed".to_string();
                        payload = Value::Object(Map::from_iter([
                            ("actor_type".to_string(), Value::from("subagent")),
                            ("thread_id".to_string(), Value::from(thread_id)),
                            ("parent_thread_id".to_string(), Value::from(parent_thread_id)),
                            ("tool_name".to_string(), Value::from(name)),
                            ("tool_use_id".to_string(), Value::from(call_id)),
                            ("input".to_string(), parsed_arguments),
                            (
                                "session_path".to_string(),
                                Value::from(imported_path.display().to_string()),
                            ),
                        ]));
                    }
                    "function_call_output" => {
                        let call_id = item
                            .get("call_id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        let name = call_names
                            .get(&call_id)
                            .cloned()
                            .unwrap_or_else(|| "unknown".to_string());
                        increment(tool_counts, &name);
                        event_type = "tool.result".to_string();
                        parse_status = "parsed".to_string();
                        payload = Value::Object(Map::from_iter([
                            ("actor_type".to_string(), Value::from("subagent")),
                            ("thread_id".to_string(), Value::from(thread_id)),
                            ("parent_thread_id".to_string(), Value::from(parent_thread_id)),
                            ("tool_name".to_string(), Value::from(name)),
                            ("tool_use_id".to_string(), Value::from(call_id)),
                            ("output".to_string(), item.get("output").cloned().unwrap_or(Value::Null)),
                            (
                                "session_path".to_string(),
                                Value::from(imported_path.display().to_string()),
                            ),
                        ]));
                    }
                    "message" => {
                        let text = self.extract_message_text(item);
                        event_type = "agent.message".to_string();
                        parse_status = "parsed".to_string();
                        payload = Value::Object(Map::from_iter([
                            ("actor_type".to_string(), Value::from("subagent")),
                            ("thread_id".to_string(), Value::from(thread_id)),
                            ("parent_thread_id".to_string(), Value::from(parent_thread_id)),
                            (
                                "role".to_string(),
                                item.get("role").cloned().unwrap_or(Value::Null),
                            ),
                            (
                                "phase".to_string(),
                                item.get("phase").cloned().unwrap_or(Value::Null),
                            ),
                            ("text".to_string(), Value::from(text.clone())),
                            (
                                "text_links".to_string(),
                                self.extract_text_links(&text)
                                    .map(|v| payload_to_value(&v))
                                    .unwrap_or(Value::Null),
                            ),
                            (
                                "session_path".to_string(),
                                Value::from(imported_path.display().to_string()),
                            ),
                        ]));
                    }
                    "reasoning" => {
                        let text = self.extract_reasoning_text(item);
                        event_type = "agent.reasoning".to_string();
                        parse_status = "parsed".to_string();
                        payload = Value::Object(Map::from_iter([
                            ("actor_type".to_string(), Value::from("subagent")),
                            ("thread_id".to_string(), Value::from(thread_id)),
                            ("parent_thread_id".to_string(), Value::from(parent_thread_id)),
                            (
                                "role".to_string(),
                                item.get("role").cloned().unwrap_or(Value::Null),
                            ),
                            (
                                "phase".to_string(),
                                item.get("phase").cloned().unwrap_or(Value::Null),
                            ),
                            ("text".to_string(), Value::from(text.clone())),
                            (
                                "text_links".to_string(),
                                self.extract_text_links(&text)
                                    .map(|v| payload_to_value(&v))
                                    .unwrap_or(Value::Null),
                            ),
                            (
                                "session_path".to_string(),
                                Value::from(imported_path.display().to_string()),
                            ),
                        ]));
                    }
                    _ => {}
                }
            }
            "event_msg" => {
                let Some(item) = parsed.get("payload").and_then(Value::as_object) else {
                    return Some(self.build_subagent_event(
                        seq,
                        &ts,
                        event_type,
                        raw_type,
                        parse_status,
                        payload,
                    ));
                };
                let msg_type = item
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                if msg_type == "agent_message" {
                    let text = item
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    event_type = "agent.message".to_string();
                    parse_status = "parsed".to_string();
                    payload = Value::Object(Map::from_iter([
                        ("actor_type".to_string(), Value::from("subagent")),
                        ("thread_id".to_string(), Value::from(thread_id)),
                        ("parent_thread_id".to_string(), Value::from(parent_thread_id)),
                        ("text".to_string(), Value::from(text.clone())),
                        (
                            "phase".to_string(),
                            item.get("phase").cloned().unwrap_or(Value::Null),
                        ),
                        (
                            "text_links".to_string(),
                            self.extract_text_links(&text)
                                .map(|v| payload_to_value(&v))
                                .unwrap_or(Value::Null),
                        ),
                        (
                            "session_path".to_string(),
                            Value::from(imported_path.display().to_string()),
                        ),
                    ]));
                } else {
                    event_type = "agent.meta".to_string();
                    parse_status = "parsed".to_string();
                    let mut payload_map = Map::from_iter([
                        ("actor_type".to_string(), Value::from("subagent")),
                        ("thread_id".to_string(), Value::from(thread_id)),
                        ("parent_thread_id".to_string(), Value::from(parent_thread_id)),
                        ("meta_type".to_string(), Value::from(msg_type.clone())),
                        (
                            "session_path".to_string(),
                            Value::from(imported_path.display().to_string()),
                        ),
                    ]);
                    match msg_type.as_str() {
                        "token_count" => {
                            let total_tokens = item
                                .get("info")
                                .and_then(Value::as_object)
                                .and_then(|v| v.get("total_token_usage"))
                                .and_then(Value::as_object)
                                .and_then(|v| v.get("total_tokens"))
                                .cloned()
                                .unwrap_or(Value::Null);
                            payload_map.insert("total_tokens".to_string(), total_tokens);
                            payload_map.insert(
                                "rate_limits".to_string(),
                                item.get("rate_limits").cloned().unwrap_or(Value::Null),
                            );
                        }
                        "task_started" => {
                            payload_map.insert(
                                "turn_id".to_string(),
                                item.get("turn_id").cloned().unwrap_or(Value::Null),
                            );
                            payload_map.insert(
                                "model_context_window".to_string(),
                                item.get("model_context_window").cloned().unwrap_or(Value::Null),
                            );
                            payload_map.insert(
                                "collaboration_mode_kind".to_string(),
                                item.get("collaboration_mode_kind")
                                    .cloned()
                                    .unwrap_or(Value::Null),
                            );
                        }
                        "task_complete" => {
                            payload_map.insert(
                                "turn_id".to_string(),
                                item.get("turn_id").cloned().unwrap_or(Value::Null),
                            );
                            payload_map.insert(
                                "last_agent_message".to_string(),
                                item.get("last_agent_message")
                                    .cloned()
                                    .unwrap_or(Value::Null),
                            );
                        }
                        "user_message" => {
                            payload_map.insert(
                                "text".to_string(),
                                item.get("message").cloned().unwrap_or(Value::Null),
                            );
                            payload_map.insert(
                                "images_count".to_string(),
                                Value::from(
                                    item.get("images")
                                        .and_then(Value::as_array)
                                        .map(|v| v.len() as u64)
                                        .unwrap_or(0),
                                ),
                            );
                            payload_map.insert(
                                "local_images_count".to_string(),
                                Value::from(
                                    item.get("local_images")
                                        .and_then(Value::as_array)
                                        .map(|v| v.len() as u64)
                                        .unwrap_or(0),
                                ),
                            );
                            payload_map.insert(
                                "text_elements_count".to_string(),
                                Value::from(
                                    item.get("text_elements")
                                        .and_then(Value::as_array)
                                        .map(|v| v.len() as u64)
                                        .unwrap_or(0),
                                ),
                            );
                        }
                        _ => {
                            payload_map.insert("raw".to_string(), Value::Object(item.clone()));
                        }
                    }
                    payload = Value::Object(payload_map);
                }
            }
            "turn_context" => {
                event_type = "agent.meta".to_string();
                parse_status = "parsed".to_string();
                let context = parsed.get("payload").and_then(Value::as_object);
                let sandbox_policy_type = context
                    .and_then(|ctx| ctx.get("sandbox_policy"))
                    .and_then(Value::as_object)
                    .and_then(|v| v.get("type"))
                    .cloned()
                    .unwrap_or(Value::Null);
                let collaboration_mode_kind = context
                    .and_then(|ctx| ctx.get("collaboration_mode"))
                    .and_then(Value::as_object)
                    .and_then(|v| v.get("mode"))
                    .cloned()
                    .unwrap_or(Value::Null);

                payload = Value::Object(Map::from_iter([
                    ("actor_type".to_string(), Value::from("subagent")),
                    ("thread_id".to_string(), Value::from(thread_id)),
                    ("parent_thread_id".to_string(), Value::from(parent_thread_id)),
                    ("meta_type".to_string(), Value::from("turn_context")),
                    (
                        "session_path".to_string(),
                        Value::from(imported_path.display().to_string()),
                    ),
                    (
                        "turn_id".to_string(),
                        context
                            .and_then(|ctx| ctx.get("turn_id"))
                            .cloned()
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "cwd".to_string(),
                        context
                            .and_then(|ctx| ctx.get("cwd"))
                            .cloned()
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "current_date".to_string(),
                        context
                            .and_then(|ctx| ctx.get("current_date"))
                            .cloned()
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "timezone".to_string(),
                        context
                            .and_then(|ctx| ctx.get("timezone"))
                            .cloned()
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "approval_policy".to_string(),
                        context
                            .and_then(|ctx| ctx.get("approval_policy"))
                            .cloned()
                            .unwrap_or(Value::Null),
                    ),
                    ("sandbox_policy_type".to_string(), sandbox_policy_type),
                    (
                        "model".to_string(),
                        context
                            .and_then(|ctx| ctx.get("model"))
                            .cloned()
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "effort".to_string(),
                        context
                            .and_then(|ctx| ctx.get("effort"))
                            .cloned()
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "summary".to_string(),
                        context
                            .and_then(|ctx| ctx.get("summary"))
                            .cloned()
                            .unwrap_or(Value::Null),
                    ),
                    ("collaboration_mode_kind".to_string(), collaboration_mode_kind),
                ]));
            }
            _ => {}
        }

        Some(self.build_subagent_event(
            seq,
            &ts,
            event_type,
            raw_type,
            parse_status,
            payload,
        ))
    }

    fn build_subagent_event(
        &self,
        seq: u64,
        ts: &str,
        event_type: String,
        raw_type: String,
        parse_status: String,
        payload: Value,
    ) -> CodexEvent {
        CodexEvent {
            schema_version: 1,
            ts: ts.to_string(),
            task_id: self.context.task_id.clone(),
            run_id: self.context.run_id.clone(),
            seq,
            event_type,
            raw_type,
            parse_status,
            payload,
            source: "subagent_session".to_string(),
        }
    }

    fn parse_item_event(
        &mut self,
        item: &Map<String, Value>,
        event_type: &str,
        tool_counts: &mut HashMap<String, u64>,
        subagent_counts: &mut HashMap<String, u64>,
        subagent_threads: &mut HashSet<String>,
    ) -> (String, Value, String) {
        let item_type = item
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        if matches!(
            item_type.as_str(),
            "item.started" | "item.updated" | "item.completed"
        ) {
            if let Some(nested_item) = item.get("item").and_then(Value::as_object) {
                return self.parse_item_event(
                    nested_item,
                    &item_type,
                    tool_counts,
                    subagent_counts,
                    subagent_threads,
                );
            }
        }
        let phase = match event_type {
            "item.started" => "started",
            "item.updated" => "updated",
            _ => "completed",
        }
        .to_string();

        match item_type.as_str() {
            "agent_message" => (
                "agent.message".to_string(),
                payload_to_value(&AgentMessagePayload {
                    actor_type: Some("agent".to_string()),
                    thread_id: self.state.thread_id.clone(),
                    item_id: item.get("id").and_then(Value::as_str).map(str::to_string),
                    text: item
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    text_links: item
                        .get("text")
                        .and_then(Value::as_str)
                        .and_then(|text| self.extract_text_links(text)),
                    ..AgentMessagePayload::default()
                }),
                "parsed".to_string(),
            ),
            "reasoning" => (
                "agent.reasoning".to_string(),
                payload_to_value(&AgentMessagePayload {
                    actor_type: Some("agent".to_string()),
                    thread_id: self.state.thread_id.clone(),
                    text: item
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    text_links: item
                        .get("text")
                        .and_then(Value::as_str)
                        .and_then(|text| self.extract_text_links(text)),
                    ..AgentMessagePayload::default()
                }),
                "parsed".to_string(),
            ),
            "error" => {
                let mut payload = self.build_error_payload(
                    item.get("message").and_then(Value::as_str),
                    item.get("type").and_then(Value::as_str),
                );
                payload.item_id = item.get("id").and_then(Value::as_str).map(str::to_string);
                payload.status = item
                    .get("status")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                payload.phase = Some(phase);
                ("error".to_string(), payload_to_value(&payload), "parsed".to_string())
            }
            "command_execution" => {
                increment(tool_counts, "command_execution");
                if phase == "started" {
                    (
                        "tool.call".to_string(),
                        payload_to_value(&ToolCallPayload {
                            actor_type: Some("agent".to_string()),
                            thread_id: self.state.thread_id.clone(),
                            tool_name: "command_execution".to_string(),
                            tool_use_id: item.get("id").and_then(Value::as_str).map(str::to_string),
                            status: item
                                .get("status")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            input: Some(Value::Object(Map::from_iter([(
                                "command".to_string(),
                                item.get("command").cloned().unwrap_or(Value::Null),
                            )]))),
                            ..ToolCallPayload::default()
                        }),
                        "parsed".to_string(),
                    )
                } else {
                    (
                        "tool.result".to_string(),
                        payload_to_value(&ToolResultPayload {
                            actor_type: Some("agent".to_string()),
                            thread_id: self.state.thread_id.clone(),
                            tool_name: "command_execution".to_string(),
                            tool_use_id: item.get("id").and_then(Value::as_str).map(str::to_string),
                            status: item
                                .get("status")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            input: Some(Value::Object(Map::from_iter([(
                                "command".to_string(),
                                item.get("command").cloned().unwrap_or(Value::Null),
                            )]))),
                            exit_code: item.get("exit_code").and_then(Value::as_i64).map(|v| v as i32),
                            stderr: item.get("stderr").cloned(),
                            output: item.get("aggregated_output").cloned(),
                            ..ToolResultPayload::default()
                        }),
                        "parsed".to_string(),
                    )
                }
            }
            "mcp_tool_call" => {
                let tool_name = item
                    .get("tool")
                    .and_then(Value::as_str)
                    .unwrap_or("mcp_tool_call")
                    .to_string();
                increment(tool_counts, &tool_name);
                if phase == "started" {
                    (
                        "tool.call".to_string(),
                        payload_to_value(&ToolCallPayload {
                            actor_type: Some("agent".to_string()),
                            thread_id: self.state.thread_id.clone(),
                            tool_name,
                            tool_use_id: item.get("id").and_then(Value::as_str).map(str::to_string),
                            status: item
                                .get("status")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            server: item
                                .get("server")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            input: item.get("arguments").cloned(),
                            ..ToolCallPayload::default()
                        }),
                        "parsed".to_string(),
                    )
                } else {
                    (
                        "tool.result".to_string(),
                        payload_to_value(&ToolResultPayload {
                            actor_type: Some("agent".to_string()),
                            thread_id: self.state.thread_id.clone(),
                            tool_name,
                            tool_use_id: item.get("id").and_then(Value::as_str).map(str::to_string),
                            status: item
                                .get("status")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            server: item
                                .get("server")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            error: item.get("error").cloned(),
                            output: item.get("result").cloned(),
                            ..ToolResultPayload::default()
                        }),
                        "parsed".to_string(),
                    )
                }
            }
            "web_search" => {
                increment(tool_counts, "web_search");
                if phase == "started" {
                    (
                        "tool.call".to_string(),
                        payload_to_value(&ToolCallPayload {
                            actor_type: Some("agent".to_string()),
                            thread_id: self.state.thread_id.clone(),
                            tool_name: "web_search".to_string(),
                            tool_use_id: item.get("id").and_then(Value::as_str).map(str::to_string),
                            input: Some(Value::Object(Map::from_iter([
                                ("query".to_string(), item.get("query").cloned().unwrap_or(Value::Null)),
                                ("action".to_string(), item.get("action").cloned().unwrap_or(Value::Null)),
                            ]))),
                            ..ToolCallPayload::default()
                        }),
                        "parsed".to_string(),
                    )
                } else {
                    (
                        "tool.result".to_string(),
                        payload_to_value(&ToolResultPayload {
                            actor_type: Some("agent".to_string()),
                            thread_id: self.state.thread_id.clone(),
                            tool_name: "web_search".to_string(),
                            tool_use_id: item.get("id").and_then(Value::as_str).map(str::to_string),
                            output: Some(Value::Object(Map::from_iter([
                                ("query".to_string(), item.get("query").cloned().unwrap_or(Value::Null)),
                                ("action".to_string(), item.get("action").cloned().unwrap_or(Value::Null)),
                            ]))),
                            ..ToolResultPayload::default()
                        }),
                        "parsed".to_string(),
                    )
                }
            }
            "todo_list" => {
                let mut normalized_items: Vec<TodoItem> = Vec::new();
                let mut completed_count: u32 = 0;
                if let Some(items) = item.get("items").and_then(Value::as_array) {
                    for entry in items {
                        if let Some(entry_obj) = entry.as_object() {
                            let completed = entry_obj
                                .get("completed")
                                .and_then(Value::as_bool)
                                .unwrap_or(false);
                            if completed {
                                completed_count += 1;
                            }
                            normalized_items.push(TodoItem {
                                text: entry_obj
                                    .get("text")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string(),
                                completed,
                            });
                        }
                    }
                }
                (
                    "todo.update".to_string(),
                    payload_to_value(&TodoUpdatePayload {
                        actor_type: Some("agent".to_string()),
                        thread_id: self.state.thread_id.clone(),
                        item_id: item.get("id").and_then(Value::as_str).map(str::to_string),
                        status: item
                            .get("status")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        phase: Some(phase),
                        total_count: normalized_items.len() as u32,
                        completed_count,
                        items: normalized_items,
                    }),
                    "parsed".to_string(),
                )
            }
            "collab_tool_call" => {
                let tool_name = item
                    .get("tool")
                    .and_then(Value::as_str)
                    .unwrap_or("collab_tool_call")
                    .to_string();
                increment(tool_counts, &tool_name);
                increment(subagent_counts, &tool_name);
                let receiver_ids = item
                    .get("receiver_thread_ids")
                    .and_then(Value::as_array)
                    .map(|list| {
                        list.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect::<Vec<String>>()
                    })
                    .unwrap_or_default();
                for receiver in &receiver_ids {
                    subagent_threads.insert(receiver.clone());
                }
                (
                    "tool.result".to_string(),
                    Value::Object(Map::from_iter([
                        ("actor_type".to_string(), Value::from("agent")),
                        (
                            "thread_id".to_string(),
                            item.get("sender_thread_id")
                                .cloned()
                                .unwrap_or_else(|| {
                                    self.state
                                        .thread_id
                                        .clone()
                                        .map(Value::from)
                                        .unwrap_or(Value::Null)
                                }),
                        ),
                        ("tool_name".to_string(), Value::from(tool_name.clone())),
                        (
                            "tool_use_id".to_string(),
                            item.get("id").cloned().unwrap_or(Value::Null),
                        ),
                        ("phase".to_string(), Value::from(phase)),
                        (
                            "status".to_string(),
                            item.get("status").cloned().unwrap_or(Value::Null),
                        ),
                        (
                            "sender_thread_id".to_string(),
                            item.get("sender_thread_id")
                                .cloned()
                                .unwrap_or(Value::Null),
                        ),
                        (
                            "receiver_thread_ids".to_string(),
                            Value::Array(receiver_ids.into_iter().map(Value::from).collect()),
                        ),
                        (
                            "prompt".to_string(),
                            item.get("prompt").cloned().unwrap_or(Value::Null),
                        ),
                        (
                            "agents_states".to_string(),
                            item.get("agents_states").cloned().unwrap_or(Value::Object(Map::new())),
                        ),
                        ("detection_source".to_string(), Value::from("tool_payload")),
                        (
                            "detection_confidence".to_string(),
                            Value::from(if is_collab_high_confidence_tool(&tool_name) {
                                "high"
                            } else {
                                "medium"
                            }),
                        ),
                        (
                            "raw_ref".to_string(),
                            item.get("id").cloned().unwrap_or(Value::Null),
                        ),
                    ])),
                    "parsed".to_string(),
                )
            }
            "tool_use" => {
                let tool_name = item
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                increment(tool_counts, &tool_name);
                let is_subagent_tool = is_subagent_tool(&tool_name);
                if is_subagent_tool {
                    increment(subagent_counts, &tool_name);
                }
                (
                    "tool.call".to_string(),
                    payload_to_value(&ToolCallPayload {
                        actor_type: Some("agent".to_string()),
                        thread_id: self.state.thread_id.clone(),
                        tool_name,
                        tool_use_id: item.get("id").and_then(Value::as_str).map(str::to_string),
                        input: item.get("input").cloned(),
                        detection_source: is_subagent_tool.then(|| "tool_name".to_string()),
                        detection_confidence: is_subagent_tool.then(|| "high".to_string()),
                        raw_ref: is_subagent_tool
                            .then(|| item.get("id").and_then(Value::as_str).map(str::to_string))
                            .flatten(),
                        ..ToolCallPayload::default()
                    }),
                    "parsed".to_string(),
                )
            }
            "tool_result" => {
                let tool_name = item
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                increment(tool_counts, &tool_name);
                (
                    "tool.result".to_string(),
                    payload_to_value(&ToolResultPayload {
                        actor_type: Some("agent".to_string()),
                        thread_id: self.state.thread_id.clone(),
                        tool_name,
                        tool_use_id: item
                            .get("tool_use_id")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                            .or_else(|| item.get("id").and_then(Value::as_str).map(str::to_string)),
                        status: item
                            .get("status")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        stderr: item.get("stderr").cloned(),
                        error: item.get("error").cloned(),
                        output: item
                            .get("output")
                            .cloned()
                            .or_else(|| item.get("content").cloned())
                            .or_else(|| item.get("result").cloned()),
                        ..ToolResultPayload::default()
                    }),
                    "parsed".to_string(),
                )
            }
            "file_change" => {
                let mut normalized_changes: Vec<FileChange> = Vec::new();
                if let Some(changes) = item.get("changes").and_then(Value::as_array) {
                    for change in changes {
                        if let Some(change_obj) = change.as_object() {
                            normalized_changes.push(FileChange {
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
                (
                    "file.change".to_string(),
                    payload_to_value(&FileChangePayload {
                        actor_type: Some("agent".to_string()),
                        thread_id: self.state.thread_id.clone(),
                        item_id: item.get("id").and_then(Value::as_str).map(str::to_string),
                        status: item
                            .get("status")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        changes: normalized_changes,
                    }),
                    "parsed".to_string(),
                )
            }
            _ => (
                "raw.unparsed".to_string(),
                payload_to_value(&RawPayload {
                    actor_type: Some("agent".to_string()),
                    thread_id: self.state.thread_id.clone(),
                    raw: Some(Value::Object(item.clone())),
                    ..RawPayload::default()
                }),
                "best_effort".to_string(),
            ),
        }
    }

    fn extract_message_text(&self, item: &Map<String, Value>) -> String {
        let Some(content) = item.get("content").and_then(Value::as_array) else {
            return String::new();
        };
        let mut parts: Vec<String> = Vec::new();
        for chunk in content {
            if let Some(chunk_obj) = chunk.as_object() {
                if let Some(text) = chunk_obj.get("text").and_then(Value::as_str) {
                    if !text.is_empty() {
                        parts.push(text.to_string());
                    }
                }
            }
        }
        parts.join("\n").trim().to_string()
    }

    fn extract_reasoning_text(&self, item: &Map<String, Value>) -> String {
        let Some(summary) = item.get("summary").and_then(Value::as_array) else {
            return String::new();
        };
        let mut parts: Vec<String> = Vec::new();
        for entry in summary {
            if let Some(entry_obj) = entry.as_object() {
                if let Some(text) = entry_obj.get("text").and_then(Value::as_str) {
                    if !text.is_empty() {
                        parts.push(text.to_string());
                    }
                }
            }
        }
        parts.join("\n").trim().to_string()
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
    if matches!(normalized.as_str(), "websocket" | "websockets" | "ws" | "wss") {
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
        "spawn_agent" | "send_input" | "wait_agent" | "close_agent" | "resume_agent"
    )
}

fn is_collab_high_confidence_tool(tool_name: &str) -> bool {
    matches!(tool_name, "spawn_agent" | "wait")
}
