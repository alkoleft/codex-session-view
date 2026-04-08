use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::events::record::EventRecord;
use crate::events::types::{
    COLLAB_CLOSE_AGENT, COLLAB_RESUME_AGENT, COLLAB_SEND_INPUT, COLLAB_SPAWN_AGENT, COLLAB_WAIT,
    CONTEXT_COMPACTED_DUPLICATE, FILE_CHANGE, MCP_CALL, MCP_RESULT, PATCH_APPLY,
    PATCH_APPLY_DUPLICATE, PLAN_UPDATE, SHELL_CALL, SHELL_RESULT, STDIN_WRITE, TODO_UPDATE,
    TOOL_CALL, TOOL_RESULT, USER_INPUT_REQUEST, WEB_OPEN, WEB_SEARCH,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationClass {
    Atomic,
    Lifecycle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OperationKind {
    Tool,
    Shell,
    Mcp,
    StdinWrite,
    WebSearch,
    WebOpen,
    PlanUpdate,
    UserInputRequest,
    PatchApply,
    CollabSpawnAgent,
    CollabSendInput,
    CollabWait,
    CollabCloseAgent,
    CollabResumeAgent,
    FileChange,
    TodoUpdate,
}

impl OperationKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tool => "tool",
            Self::Shell => "shell",
            Self::Mcp => "mcp",
            Self::StdinWrite => "stdin.write",
            Self::WebSearch => "web.search",
            Self::WebOpen => "web.open",
            Self::PlanUpdate => "plan.update",
            Self::UserInputRequest => "user.input.request",
            Self::PatchApply => "patch.apply",
            Self::CollabSpawnAgent => "collab.spawn_agent",
            Self::CollabSendInput => "collab.send_input",
            Self::CollabWait => "collab.wait",
            Self::CollabCloseAgent => "collab.close_agent",
            Self::CollabResumeAgent => "collab.resume_agent",
            Self::FileChange => "file.change",
            Self::TodoUpdate => "todo.update",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleRole {
    Start,
    Update,
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PolicyShape {
    Split {
        start_event_type: &'static str,
        result_event_type: &'static str,
    },
    Phased {
        event_type: &'static str,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperationPolicy {
    pub kind: OperationKind,
    pub class: OperationClass,
    pub mutable_after_terminal: bool,
    shape: PolicyShape,
}

impl OperationPolicy {
    fn matches_event_type(&self, event_type: &str) -> bool {
        match self.shape {
            PolicyShape::Split {
                start_event_type,
                result_event_type,
            } => event_type == start_event_type || event_type == result_event_type,
            PolicyShape::Phased { event_type: kind } => event_type == kind,
        }
    }

    fn classify_role(&self, event: &EventRecord) -> Option<LifecycleRole> {
        let payload = event.payload.as_object();
        let phase = phase_for_event(event, payload);
        let terminal = is_terminal_event(event.event_type.as_str(), phase.as_deref(), payload);

        match self.shape {
            PolicyShape::Split {
                start_event_type,
                result_event_type,
            } => {
                if event.event_type == start_event_type {
                    Some(LifecycleRole::Start)
                } else if event.event_type == result_event_type {
                    if terminal {
                        Some(LifecycleRole::Terminal)
                    } else {
                        Some(LifecycleRole::Update)
                    }
                } else {
                    None
                }
            }
            PolicyShape::Phased { event_type } => {
                if event.event_type != event_type {
                    return None;
                }
                match phase.as_deref() {
                    Some("started") => Some(LifecycleRole::Start),
                    Some("updated") => Some(LifecycleRole::Update),
                    Some("completed" | "failed" | "cancelled" | "error") => {
                        Some(LifecycleRole::Terminal)
                    }
                    _ if terminal => Some(LifecycleRole::Terminal),
                    _ => Some(LifecycleRole::Update),
                }
            }
        }
    }
}

const LIFECYCLE_POLICIES: [OperationPolicy; 16] = [
    OperationPolicy {
        kind: OperationKind::Tool,
        class: OperationClass::Lifecycle,
        mutable_after_terminal: true,
        shape: PolicyShape::Split {
            start_event_type: TOOL_CALL,
            result_event_type: TOOL_RESULT,
        },
    },
    OperationPolicy {
        kind: OperationKind::Shell,
        class: OperationClass::Lifecycle,
        mutable_after_terminal: true,
        shape: PolicyShape::Split {
            start_event_type: SHELL_CALL,
            result_event_type: SHELL_RESULT,
        },
    },
    OperationPolicy {
        kind: OperationKind::Mcp,
        class: OperationClass::Lifecycle,
        mutable_after_terminal: true,
        shape: PolicyShape::Split {
            start_event_type: MCP_CALL,
            result_event_type: MCP_RESULT,
        },
    },
    OperationPolicy {
        kind: OperationKind::StdinWrite,
        class: OperationClass::Lifecycle,
        mutable_after_terminal: false,
        shape: PolicyShape::Phased {
            event_type: STDIN_WRITE,
        },
    },
    OperationPolicy {
        kind: OperationKind::WebSearch,
        class: OperationClass::Lifecycle,
        mutable_after_terminal: true,
        shape: PolicyShape::Phased {
            event_type: WEB_SEARCH,
        },
    },
    OperationPolicy {
        kind: OperationKind::WebOpen,
        class: OperationClass::Lifecycle,
        mutable_after_terminal: true,
        shape: PolicyShape::Phased {
            event_type: WEB_OPEN,
        },
    },
    OperationPolicy {
        kind: OperationKind::PlanUpdate,
        class: OperationClass::Lifecycle,
        mutable_after_terminal: true,
        shape: PolicyShape::Phased {
            event_type: PLAN_UPDATE,
        },
    },
    OperationPolicy {
        kind: OperationKind::UserInputRequest,
        class: OperationClass::Lifecycle,
        mutable_after_terminal: false,
        shape: PolicyShape::Phased {
            event_type: USER_INPUT_REQUEST,
        },
    },
    OperationPolicy {
        kind: OperationKind::PatchApply,
        class: OperationClass::Lifecycle,
        mutable_after_terminal: true,
        shape: PolicyShape::Phased {
            event_type: PATCH_APPLY,
        },
    },
    OperationPolicy {
        kind: OperationKind::CollabSpawnAgent,
        class: OperationClass::Lifecycle,
        mutable_after_terminal: true,
        shape: PolicyShape::Phased {
            event_type: COLLAB_SPAWN_AGENT,
        },
    },
    OperationPolicy {
        kind: OperationKind::CollabSendInput,
        class: OperationClass::Lifecycle,
        mutable_after_terminal: true,
        shape: PolicyShape::Phased {
            event_type: COLLAB_SEND_INPUT,
        },
    },
    OperationPolicy {
        kind: OperationKind::CollabWait,
        class: OperationClass::Lifecycle,
        mutable_after_terminal: true,
        shape: PolicyShape::Phased {
            event_type: COLLAB_WAIT,
        },
    },
    OperationPolicy {
        kind: OperationKind::CollabCloseAgent,
        class: OperationClass::Lifecycle,
        mutable_after_terminal: true,
        shape: PolicyShape::Phased {
            event_type: COLLAB_CLOSE_AGENT,
        },
    },
    OperationPolicy {
        kind: OperationKind::CollabResumeAgent,
        class: OperationClass::Lifecycle,
        mutable_after_terminal: true,
        shape: PolicyShape::Phased {
            event_type: COLLAB_RESUME_AGENT,
        },
    },
    OperationPolicy {
        kind: OperationKind::FileChange,
        class: OperationClass::Lifecycle,
        mutable_after_terminal: false,
        shape: PolicyShape::Phased {
            event_type: FILE_CHANGE,
        },
    },
    OperationPolicy {
        kind: OperationKind::TodoUpdate,
        class: OperationClass::Lifecycle,
        mutable_after_terminal: false,
        shape: PolicyShape::Phased {
            event_type: TODO_UPDATE,
        },
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventClassification {
    Atomic,
    DuplicateMarker,
    Lifecycle {
        kind: OperationKind,
        role: LifecycleRole,
        terminal: bool,
        mutable_after_terminal: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OperationScope {
    pub run_id: String,
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct OperationKey {
    pub kind: OperationKind,
    pub scope: OperationScope,
    pub operation_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationSnapshot {
    pub key: OperationKey,
    pub revision: u64,
    pub started_seq: Option<u64>,
    pub terminal_seq: Option<u64>,
    pub last_seq: u64,
    pub last_event_type: String,
    pub last_phase: Option<String>,
    pub last_status: Option<String>,
    pub terminal: bool,
    pub mutable_after_terminal: bool,
    terminal_preference: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationSnapshotUpdate {
    pub key: OperationKey,
    pub revision: u64,
    pub seq: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateMarkerRecord {
    pub event_type: String,
    pub seq: u64,
    pub duplicate_of: Option<String>,
    pub operation_id: Option<String>,
    pub scope: OperationScope,
}

#[derive(Debug, Default)]
pub struct OperationStream {
    snapshots: BTreeMap<OperationKey, OperationSnapshot>,
    updates: Vec<OperationSnapshotUpdate>,
    duplicate_markers: Vec<DuplicateMarkerRecord>,
}

#[derive(Debug, Default)]
pub struct OperationProjection {
    pub snapshots: Vec<OperationSnapshot>,
    pub updates: Vec<OperationSnapshotUpdate>,
    pub duplicate_markers: Vec<DuplicateMarkerRecord>,
}

pub fn policy_registry() -> &'static [OperationPolicy] {
    &LIFECYCLE_POLICIES
}

pub fn policy_for_kind(kind: OperationKind) -> Option<&'static OperationPolicy> {
    LIFECYCLE_POLICIES.iter().find(|policy| policy.kind == kind)
}

pub fn policy_for_event_type(event_type: &str) -> Option<&'static OperationPolicy> {
    LIFECYCLE_POLICIES
        .iter()
        .find(|policy| policy.matches_event_type(event_type))
}

pub fn classify_event(event: &EventRecord) -> EventClassification {
    if is_duplicate_marker(event.event_type.as_str()) {
        return EventClassification::DuplicateMarker;
    }

    let Some(policy) = policy_for_event_type(event.event_type.as_str()) else {
        return EventClassification::Atomic;
    };

    let Some(role) = policy.classify_role(event) else {
        return EventClassification::Atomic;
    };

    let payload = event.payload.as_object();
    let phase = phase_for_event(event, payload);
    let terminal = is_terminal_event(event.event_type.as_str(), phase.as_deref(), payload);

    EventClassification::Lifecycle {
        kind: policy.kind,
        role,
        terminal,
        mutable_after_terminal: policy.mutable_after_terminal,
    }
}

pub fn project_operation_stream(events: &[EventRecord]) -> OperationProjection {
    let mut stream = OperationStream::new();
    for event in events {
        stream.apply_event(event);
    }
    stream.into_projection()
}

pub fn project_operation_snapshots(events: &[EventRecord]) -> Vec<OperationSnapshot> {
    project_operation_stream(events).snapshots
}

impl OperationStream {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn updates(&self) -> &[OperationSnapshotUpdate] {
        &self.updates
    }

    pub fn snapshots(&self) -> &BTreeMap<OperationKey, OperationSnapshot> {
        &self.snapshots
    }

    pub fn duplicate_markers(&self) -> &[DuplicateMarkerRecord] {
        &self.duplicate_markers
    }

    pub fn apply_event(&mut self, event: &EventRecord) -> bool {
        match classify_event(event) {
            EventClassification::Atomic => false,
            EventClassification::DuplicateMarker => {
                self.duplicate_markers.push(DuplicateMarkerRecord {
                    event_type: event.event_type.clone(),
                    seq: event.seq,
                    duplicate_of: payload_string(event.payload.as_object(), "duplicate_of"),
                    operation_id: operation_id(event.payload.as_object()),
                    scope: operation_scope(event),
                });
                false
            }
            EventClassification::Lifecycle {
                kind,
                role,
                terminal,
                mutable_after_terminal,
            } => {
                let Some(operation_id) = operation_id(event.payload.as_object()) else {
                    return false;
                };

                let key = OperationKey {
                    kind,
                    scope: operation_scope(event),
                    operation_id,
                };

                let snapshot =
                    self.snapshots
                        .entry(key.clone())
                        .or_insert_with(|| OperationSnapshot {
                            key: key.clone(),
                            revision: 0,
                            started_seq: None,
                            terminal_seq: None,
                            last_seq: 0,
                            last_event_type: String::new(),
                            last_phase: None,
                            last_status: None,
                            terminal: false,
                            mutable_after_terminal,
                            terminal_preference: 0,
                        });

                let previous = snapshot.clone();

                if event.seq <= snapshot.last_seq {
                    return false;
                }

                if snapshot.terminal && !snapshot.mutable_after_terminal {
                    return false;
                }

                if snapshot.started_seq.is_none() && matches!(role, LifecycleRole::Start) {
                    snapshot.started_seq = Some(event.seq);
                } else if snapshot.started_seq.is_none() {
                    snapshot.started_seq = Some(event.seq);
                }

                snapshot.last_seq = event.seq;
                snapshot.last_event_type = event.event_type.clone();
                snapshot.last_phase = phase_for_event(event, event.payload.as_object());
                snapshot.last_status = payload_string(event.payload.as_object(), "status");

                if terminal || matches!(role, LifecycleRole::Terminal) {
                    let terminal_preference = terminal_event_preference(event);
                    snapshot.terminal = true;
                    if snapshot.terminal_seq.is_none()
                        || terminal_preference > snapshot.terminal_preference
                        || (terminal_preference == snapshot.terminal_preference
                            && Some(event.seq) >= snapshot.terminal_seq)
                    {
                        snapshot.terminal_seq = Some(event.seq);
                        snapshot.terminal_preference = terminal_preference;
                    }
                }

                if *snapshot == previous {
                    return false;
                }

                snapshot.revision += 1;
                self.updates.push(OperationSnapshotUpdate {
                    key,
                    revision: snapshot.revision,
                    seq: event.seq,
                });
                true
            }
        }
    }

    pub fn into_projection(self) -> OperationProjection {
        let mut snapshots = self.snapshots.into_values().collect::<Vec<_>>();
        snapshots.sort_by_key(|snapshot| {
            (
                snapshot.started_seq.unwrap_or(snapshot.last_seq),
                snapshot.key.kind,
                snapshot.key.operation_id.clone(),
            )
        });

        OperationProjection {
            snapshots,
            updates: self.updates,
            duplicate_markers: self.duplicate_markers,
        }
    }
}

pub fn operation_scope(event: &EventRecord) -> OperationScope {
    let payload = event.payload.as_object();
    let thread_id = payload
        .and_then(|obj| {
            obj.get("thread_id")
                .or_else(|| obj.get("sender_thread_id"))
                .or_else(|| obj.get("parent_thread_id"))
        })
        .and_then(Value::as_str)
        .and_then(normalize_scope_value);

    OperationScope {
        run_id: event.run_id.trim().to_string(),
        thread_id,
    }
}

pub fn normalize_scope_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub fn operation_id(payload: Option<&Map<String, Value>>) -> Option<String> {
    payload
        .and_then(|obj| {
            obj.get("tool_use_id")
                .or_else(|| obj.get("item_id"))
                .or_else(|| obj.get("call_id"))
        })
        .and_then(Value::as_str)
        .and_then(normalize_scope_value)
}

fn payload_string(payload: Option<&Map<String, Value>>, key: &str) -> Option<String> {
    payload
        .and_then(|obj| obj.get(key))
        .and_then(Value::as_str)
        .and_then(normalize_scope_value)
}

fn terminal_event_preference(event: &EventRecord) -> u8 {
    match payload_string(event.payload.as_object(), "duplicate_of").as_deref() {
        Some(reference) if reference.starts_with("response_item.") => 2,
        Some(_) => 1,
        None => 0,
    }
}

fn phase_for_event(event: &EventRecord, payload: Option<&Map<String, Value>>) -> Option<String> {
    payload
        .and_then(|obj| obj.get("phase"))
        .and_then(Value::as_str)
        .and_then(normalize_scope_value)
        .map(|value| value.to_ascii_lowercase())
        .or_else(|| match event.raw_type.as_str() {
            "item.started" => Some("started".to_string()),
            "item.updated" => Some("updated".to_string()),
            "item.completed" => Some("completed".to_string()),
            _ => None,
        })
}

fn status_for_event(payload: Option<&Map<String, Value>>) -> Option<String> {
    payload
        .and_then(|obj| obj.get("status"))
        .and_then(Value::as_str)
        .and_then(normalize_scope_value)
        .map(|value| value.to_ascii_lowercase())
}

fn is_terminal_event(
    event_type: &str,
    phase: Option<&str>,
    payload: Option<&Map<String, Value>>,
) -> bool {
    if matches!(phase, Some("completed" | "failed" | "cancelled" | "error")) {
        return true;
    }

    if matches!(
        status_for_event(payload).as_deref(),
        Some("completed" | "failed" | "cancelled" | "error")
    ) {
        return true;
    }

    matches!(event_type, TOOL_RESULT | SHELL_RESULT | MCP_RESULT)
}

fn is_duplicate_marker(event_type: &str) -> bool {
    matches!(
        event_type,
        PATCH_APPLY_DUPLICATE | CONTEXT_COMPACTED_DUPLICATE
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn event(event_type: &str, raw_type: &str, seq: u64, payload: Value) -> EventRecord {
        EventRecord {
            schema_version: 1,
            ts: "2026-04-08T00:00:00Z".to_string(),
            task_id: "task-1".to_string(),
            run_id: " run-1 ".to_string(),
            seq,
            event_type: event_type.to_string(),
            raw_type: raw_type.to_string(),
            parse_status: "parsed".to_string(),
            payload,
        }
    }

    #[test]
    fn normalizes_scope_and_operation_id() {
        let record = event(
            WEB_SEARCH,
            "item.started",
            1,
            json!({
                "thread_id": "  thread-A  ",
                "tool_use_id": "  op-1  ",
                "phase": "Started"
            }),
        );

        let scope = operation_scope(&record);
        assert_eq!(scope.run_id, "run-1");
        assert_eq!(scope.thread_id.as_deref(), Some("thread-A"));

        let op_id = operation_id(record.payload.as_object());
        assert_eq!(op_id.as_deref(), Some("op-1"));

        let class = classify_event(&record);
        assert_eq!(
            class,
            EventClassification::Lifecycle {
                kind: OperationKind::WebSearch,
                role: LifecycleRole::Start,
                terminal: false,
                mutable_after_terminal: true,
            }
        );
    }

    #[test]
    fn resolves_terminal_for_split_and_phased() {
        let shell_call = event(
            SHELL_CALL,
            "response_item",
            1,
            json!({"tool_use_id":"exec-1"}),
        );
        let shell_result = event(
            SHELL_RESULT,
            "response_item",
            2,
            json!({"tool_use_id":"exec-1"}),
        );
        let web_update = event(
            WEB_OPEN,
            "response_item",
            3,
            json!({"tool_use_id":"web-1","phase":"updated"}),
        );
        let web_done = event(
            WEB_OPEN,
            "response_item",
            4,
            json!({"tool_use_id":"web-1","phase":"completed"}),
        );

        assert_eq!(
            classify_event(&shell_call),
            EventClassification::Lifecycle {
                kind: OperationKind::Shell,
                role: LifecycleRole::Start,
                terminal: false,
                mutable_after_terminal: true,
            }
        );
        assert_eq!(
            classify_event(&shell_result),
            EventClassification::Lifecycle {
                kind: OperationKind::Shell,
                role: LifecycleRole::Terminal,
                terminal: true,
                mutable_after_terminal: true,
            }
        );
        assert_eq!(
            classify_event(&web_update),
            EventClassification::Lifecycle {
                kind: OperationKind::WebOpen,
                role: LifecycleRole::Update,
                terminal: false,
                mutable_after_terminal: true,
            }
        );
        assert_eq!(
            classify_event(&web_done),
            EventClassification::Lifecycle {
                kind: OperationKind::WebOpen,
                role: LifecycleRole::Terminal,
                terminal: true,
                mutable_after_terminal: true,
            }
        );
    }

    #[test]
    fn tracks_duplicate_markers_without_snapshot_mutation() {
        let mut stream = OperationStream::new();

        let start = event(
            PATCH_APPLY,
            "response_item",
            1,
            json!({"tool_use_id":"patch-1","phase":"started"}),
        );
        let done = event(
            PATCH_APPLY,
            "response_item",
            2,
            json!({"tool_use_id":"patch-1","phase":"completed"}),
        );
        let duplicate_marker = event(
            PATCH_APPLY_DUPLICATE,
            "event_msg",
            3,
            json!({"tool_use_id":"patch-1","duplicate_of":"event_msg.patch_apply_end"}),
        );

        assert!(stream.apply_event(&start));
        assert!(stream.apply_event(&done));
        assert!(!stream.apply_event(&duplicate_marker));

        let snapshots = stream.snapshots();
        assert_eq!(snapshots.len(), 1);
        let snapshot = snapshots.values().next().expect("snapshot");
        assert_eq!(snapshot.revision, 2);
        assert_eq!(snapshot.terminal_seq, Some(2));

        let duplicates = stream.duplicate_markers();
        assert_eq!(duplicates.len(), 1);
        assert_eq!(duplicates[0].event_type, PATCH_APPLY_DUPLICATE);
    }

    #[test]
    fn supports_post_terminal_mutability_only_where_allowed() {
        let mut patch_stream = OperationStream::new();
        let patch_start = event(
            PATCH_APPLY,
            "response_item",
            1,
            json!({"tool_use_id":"patch-2","phase":"started"}),
        );
        let patch_done = event(
            PATCH_APPLY,
            "response_item",
            2,
            json!({"tool_use_id":"patch-2","phase":"completed","status":"completed"}),
        );
        let patch_done_late = event(
            PATCH_APPLY,
            "event_msg",
            3,
            json!({"tool_use_id":"patch-2","phase":"completed","status":"failed"}),
        );

        assert!(patch_stream.apply_event(&patch_start));
        assert!(patch_stream.apply_event(&patch_done));
        assert!(patch_stream.apply_event(&patch_done_late));

        let patch_snapshot = patch_stream
            .snapshots()
            .values()
            .next()
            .expect("patch snapshot");
        assert_eq!(patch_snapshot.revision, 3);
        assert_eq!(patch_snapshot.last_seq, 3);
        assert_eq!(patch_snapshot.last_status.as_deref(), Some("failed"));

        let mut stdin_stream = OperationStream::new();
        let stdin_start = event(
            STDIN_WRITE,
            "response_item",
            10,
            json!({"tool_use_id":"stdin-1","phase":"started"}),
        );
        let stdin_done = event(
            STDIN_WRITE,
            "response_item",
            11,
            json!({"tool_use_id":"stdin-1","phase":"completed"}),
        );
        let stdin_done_late = event(
            STDIN_WRITE,
            "event_msg",
            12,
            json!({"tool_use_id":"stdin-1","phase":"completed","status":"failed"}),
        );

        assert!(stdin_stream.apply_event(&stdin_start));
        assert!(stdin_stream.apply_event(&stdin_done));
        assert!(!stdin_stream.apply_event(&stdin_done_late));

        let stdin_snapshot = stdin_stream
            .snapshots()
            .values()
            .next()
            .expect("stdin snapshot");
        assert_eq!(stdin_snapshot.revision, 2);
        assert_eq!(stdin_snapshot.last_seq, 11);
    }

    #[test]
    fn keeps_duplicate_backed_terminal_when_later_response_item_arrives() {
        let mut stream = OperationStream::new();
        let shell_start = event(
            SHELL_CALL,
            "response_item",
            1,
            json!({"tool_use_id":"exec-1"}),
        );
        let canonical_terminal = event(
            SHELL_RESULT,
            "event_msg",
            2,
            json!({
                "tool_use_id":"exec-1",
                "duplicate_of":"response_item.function_call_output",
                "exit_code": 0
            }),
        );
        let duplicate_terminal = event(
            SHELL_RESULT,
            "response_item",
            3,
            json!({"tool_use_id":"exec-1"}),
        );

        assert!(stream.apply_event(&shell_start));
        assert!(stream.apply_event(&canonical_terminal));
        assert!(stream.apply_event(&duplicate_terminal));

        let snapshot = stream.snapshots().values().next().expect("shell snapshot");
        assert_eq!(snapshot.terminal_seq, Some(2));
        assert_eq!(snapshot.last_seq, 3);
    }

    #[test]
    fn revision_is_monotonic_and_emits_only_on_change() {
        let mut stream = OperationStream::new();

        let first = event(
            WEB_SEARCH,
            "response_item",
            1,
            json!({"tool_use_id":"web-emit","phase":"started"}),
        );
        let duplicate_first = event(
            WEB_SEARCH,
            "response_item",
            1,
            json!({"tool_use_id":"web-emit","phase":"started"}),
        );
        let second = event(
            WEB_SEARCH,
            "response_item",
            2,
            json!({"tool_use_id":"web-emit","phase":"updated"}),
        );

        assert!(stream.apply_event(&first));
        assert!(!stream.apply_event(&duplicate_first));
        assert!(stream.apply_event(&second));

        let updates = stream.updates();
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].revision, 1);
        assert_eq!(updates[1].revision, 2);
        assert!(updates[0].revision < updates[1].revision);
    }
}
