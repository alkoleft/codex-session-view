use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{AppError, AppResult};

pub const TASK_START_MARKER: &str = "<!-- codex-task:start -->";
pub const TASK_END_MARKER: &str = "<!-- codex-task:end -->";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskBlock {
    pub title: String,
    pub status: String,
    pub metadata: BTreeMap<String, String>,
    pub explicit_metadata_keys: BTreeSet<String>,
    pub body: String,
    pub raw_text: String,
    pub start: usize,
    pub end: usize,
}

impl TaskBlock {
    pub fn task_id(&self) -> AppResult<&str> {
        let Some(task_id) = self.metadata.get("id").map(String::as_str) else {
            return Err(AppError::Validation {
                field: "metadata.id",
                reason: "is required".to_string(),
            });
        };

        if task_id.trim().is_empty() {
            return Err(AppError::Validation {
                field: "metadata.id",
                reason: "must not be empty".to_string(),
            });
        }

        Ok(task_id)
    }

    pub fn cwd(&self) -> Option<&str> {
        self.metadata.get("cwd").map(String::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkerConfig {
    pub task_file: PathBuf,
    pub codex_bin: String,
    pub codex_home: Option<PathBuf>,
    pub logs_dir: Option<PathBuf>,
    pub default_cwd: Option<PathBuf>,
    pub model: Option<String>,
    pub sandbox: Option<String>,
    pub approval_policy: Option<String>,
    pub prompt_template: Option<PathBuf>,
    pub poll_interval: f64,
    pub stale_after: f64,
    pub dry_run: bool,
    pub log_to_stdout: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunSummary {
    pub schema_version: u32,
    pub task_id: String,
    pub run_id: String,
    pub status: String,
    pub failure_reason: Option<String>,
    pub failure_analysis: Value,
    pub started_at: String,
    pub finished_at: String,
    pub exit_code: Option<i32>,
    pub event_counts: BTreeMap<String, u64>,
    pub tool_counts: BTreeMap<String, u64>,
    pub subagent_counts: BTreeMap<String, u64>,
    pub paths: BTreeMap<String, String>,
}

impl Default for RunSummary {
    fn default() -> Self {
        Self {
            schema_version: 1,
            task_id: String::new(),
            run_id: String::new(),
            status: "failed".to_string(),
            failure_reason: None,
            failure_analysis: Value::Object(serde_json::Map::new()),
            started_at: String::new(),
            finished_at: String::new(),
            exit_code: None,
            event_counts: BTreeMap::new(),
            tool_counts: BTreeMap::new(),
            subagent_counts: BTreeMap::new(),
            paths: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventRecord {
    pub schema_version: u32,
    pub ts: String,
    pub seq: u64,
    pub event_type: String,
    pub raw_type: String,
    pub parse_status: String,
    pub run_id: String,
    pub task_id: String,
    pub payload: Value,
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use serde_json::json;

    use super::{EventRecord, RunSummary, TaskBlock};

    #[test]
    fn task_block_explicit_metadata_keys_are_deduplicated() {
        let mut explicit_metadata_keys = BTreeSet::new();
        explicit_metadata_keys.insert("id".to_string());
        explicit_metadata_keys.insert("id".to_string());

        let task = TaskBlock {
            title: "demo".to_string(),
            status: "todo".to_string(),
            metadata: BTreeMap::new(),
            explicit_metadata_keys,
            body: String::new(),
            raw_text: String::new(),
            start: 0,
            end: 0,
        };

        assert_eq!(task.explicit_metadata_keys.len(), 1);
        assert!(task.explicit_metadata_keys.contains("id"));
    }

    #[test]
    fn task_block_task_id_requires_metadata_id() {
        let mut metadata = BTreeMap::new();
        metadata.insert("id".to_string(), "task-42".to_string());

        let task = TaskBlock {
            title: "demo".to_string(),
            status: "todo".to_string(),
            metadata,
            explicit_metadata_keys: BTreeSet::new(),
            body: String::new(),
            raw_text: String::new(),
            start: 0,
            end: 0,
        };

        assert_eq!(task.task_id().expect("task_id should exist"), "task-42");
    }

    #[test]
    fn task_block_task_id_rejects_missing_or_empty_metadata_id() {
        let missing_id = TaskBlock {
            title: "demo".to_string(),
            status: "todo".to_string(),
            metadata: BTreeMap::new(),
            explicit_metadata_keys: BTreeSet::new(),
            body: String::new(),
            raw_text: String::new(),
            start: 0,
            end: 0,
        };
        assert!(missing_id.task_id().is_err());

        let mut metadata = BTreeMap::new();
        metadata.insert("id".to_string(), "   ".to_string());
        let empty_id = TaskBlock {
            title: "demo".to_string(),
            status: "todo".to_string(),
            metadata,
            explicit_metadata_keys: BTreeSet::new(),
            body: String::new(),
            raw_text: String::new(),
            start: 0,
            end: 0,
        };
        assert!(empty_id.task_id().is_err());
    }

    #[test]
    fn event_record_payload_supports_nested_json() {
        let record = EventRecord {
            schema_version: 1,
            ts: "2026-01-01T00:00:00Z".to_string(),
            seq: 42,
            event_type: "tool.result".to_string(),
            raw_type: "response_item".to_string(),
            parse_status: "parsed".to_string(),
            run_id: "run-1".to_string(),
            task_id: "task-1".to_string(),
            payload: json!({
                "tool_name": "exec_command",
                "agent_id": "agent-1",
                "input": {"cmd": "echo ok"},
                "output": ["ok", {"exit_code": 0}]
            }),
        };

        let encoded = serde_json::to_string(&record).expect("event should serialize");
        let decoded: EventRecord = serde_json::from_str(&encoded).expect("event should deserialize");
        assert_eq!(decoded.payload["output"][1]["exit_code"], 0);
    }

    #[test]
    fn run_summary_serialization_has_expected_top_level_keys() {
        let summary = RunSummary {
            schema_version: 1,
            task_id: "task-1".to_string(),
            run_id: "run-1".to_string(),
            status: "completed".to_string(),
            failure_reason: None,
            failure_analysis: json!({}),
            started_at: "2026-01-01T00:00:00Z".to_string(),
            finished_at: "2026-01-01T00:01:00Z".to_string(),
            exit_code: Some(0),
            event_counts: BTreeMap::new(),
            tool_counts: BTreeMap::new(),
            subagent_counts: BTreeMap::new(),
            paths: BTreeMap::new(),
        };

        let value = serde_json::to_value(summary).expect("summary should serialize");
        let object = value
            .as_object()
            .expect("summary should serialize as a top-level object");
        let actual_keys: BTreeSet<String> = object.keys().cloned().collect();
        let expected_keys: BTreeSet<String> = [
            "schema_version",
            "task_id",
            "run_id",
            "status",
            "failure_reason",
            "failure_analysis",
            "started_at",
            "finished_at",
            "exit_code",
            "event_counts",
            "tool_counts",
            "subagent_counts",
            "paths",
        ]
        .iter()
        .map(|key| key.to_string())
        .collect();
        assert_eq!(actual_keys, expected_keys);
    }

    #[test]
    fn event_record_serialization_has_expected_top_level_keys_without_agent_id() {
        let event = EventRecord {
            schema_version: 1,
            ts: "2026-01-01T00:00:00Z".to_string(),
            seq: 1,
            event_type: "tool.call".to_string(),
            raw_type: "response_item".to_string(),
            parse_status: "parsed".to_string(),
            run_id: "run-1".to_string(),
            task_id: "task-1".to_string(),
            payload: json!({
                "tool_name": "spawn_agent",
                "agent_id": "agent-1"
            }),
        };

        let value = serde_json::to_value(event).expect("event should serialize");
        let object = value
            .as_object()
            .expect("event should serialize as a top-level object");
        assert!(!object.contains_key("agent_id"));

        let actual_keys: BTreeSet<String> = object.keys().cloned().collect();
        let expected_keys: BTreeSet<String> = [
            "schema_version",
            "ts",
            "seq",
            "event_type",
            "raw_type",
            "parse_status",
            "run_id",
            "task_id",
            "payload",
        ]
        .iter()
        .map(|key| key.to_string())
        .collect();
        assert_eq!(actual_keys, expected_keys);
    }
}
