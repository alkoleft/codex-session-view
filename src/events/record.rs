use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::events::payloads::{parse_payload, PayloadObject};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventRecord {
    pub schema_version: u32,
    pub ts: String,
    pub task_id: String,
    pub run_id: String,
    pub seq: u64,
    pub event_type: String,
    pub raw_type: String,
    pub parse_status: String,
    pub payload: Value,
}

impl EventRecord {
    pub fn payload_obj(&self) -> PayloadObject {
        parse_payload(&self.event_type, &self.payload)
    }

    pub fn to_dict(&self) -> Value {
        Value::Object(Map::from_iter([
            ("schema_version".to_string(), Value::from(self.schema_version)),
            ("ts".to_string(), Value::from(self.ts.clone())),
            ("task_id".to_string(), Value::from(self.task_id.clone())),
            ("run_id".to_string(), Value::from(self.run_id.clone())),
            ("seq".to_string(), Value::from(self.seq)),
            ("event_type".to_string(), Value::from(self.event_type.clone())),
            ("raw_type".to_string(), Value::from(self.raw_type.clone())),
            ("parse_status".to_string(), Value::from(self.parse_status.clone())),
            ("payload".to_string(), self.payload.clone()),
        ]))
    }

    pub fn from_dict(payload: &Map<String, Value>) -> Self {
        let event_type = payload
            .get("event_type")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| {
                let kind = payload
                    .get("kind")
                    .and_then(Value::as_str)
                    .unwrap_or("raw_unparsed");
                legacy_event_type_by_kind(kind)
            });

        Self {
            schema_version: payload
                .get("schema_version")
                .and_then(Value::as_u64)
                .map(|v| v as u32)
                .unwrap_or(1),
            ts: payload
                .get("ts")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            task_id: payload
                .get("task_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            run_id: payload
                .get("run_id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            seq: payload.get("seq").and_then(Value::as_u64).unwrap_or(0),
            event_type,
            raw_type: payload
                .get("raw_type")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            parse_status: payload
                .get("parse_status")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            payload: payload
                .get("payload")
                .and_then(Value::as_object)
                .map(|obj| Value::Object(obj.clone()))
                .unwrap_or_else(|| Value::Object(Map::new())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodexEvent {
    pub schema_version: u32,
    pub ts: String,
    pub task_id: String,
    pub run_id: String,
    pub seq: u64,
    pub event_type: String,
    pub raw_type: String,
    pub parse_status: String,
    pub payload: Value,
    pub source: String,
}

impl CodexEvent {
    pub fn payload_obj(&self) -> PayloadObject {
        parse_payload(&self.event_type, &self.payload)
    }

    pub fn to_record(&self) -> EventRecord {
        EventRecord {
            schema_version: self.schema_version,
            ts: self.ts.clone(),
            task_id: self.task_id.clone(),
            run_id: self.run_id.clone(),
            seq: self.seq,
            event_type: self.event_type.clone(),
            raw_type: self.raw_type.clone(),
            parse_status: self.parse_status.clone(),
            payload: self.payload.clone(),
        }
    }

    pub fn from_record(record: EventRecord, source: &str) -> Self {
        Self {
            schema_version: record.schema_version,
            ts: record.ts,
            task_id: record.task_id,
            run_id: record.run_id,
            seq: record.seq,
            event_type: record.event_type,
            raw_type: record.raw_type,
            parse_status: record.parse_status,
            payload: record.payload,
            source: source.to_string(),
        }
    }
}

fn legacy_event_type_by_kind(kind: &str) -> String {
    match kind {
        "thread_started" => "thread.started",
        "turn_started" => "agent.turn.started",
        "turn_completed" => "agent.turn.completed",
        "turn_failed" => "agent.turn.failed",
        "assistant_message" => "agent.message",
        "reasoning" => "agent.reasoning",
        "tool_call" => "tool.call",
        "tool_result" => "tool.result",
        "file_change" => "file.change",
        "todo_list" => "todo.update",
        "error_event" => "error",
        "raw_unparsed" => "raw.unparsed",
        "stderr_line" => "stderr.line",
        "subagent_event" => "tool.result",
        "subagent_session" => "agent.session",
        "subagent_tool_call" => "tool.call",
        "subagent_tool_result" => "tool.result",
        "subagent_message" => "agent.message",
        "subagent_meta" => "agent.meta",
        "subagent_raw_unparsed" => "raw.unparsed",
        _ => return kind.replace('_', "."),
    }
    .to_string()
}
