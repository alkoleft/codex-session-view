use std::collections::{HashMap, VecDeque};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::events::readers::EventLogFileReader;
use crate::events::record::EventRecord;
use crate::events::types::{
    AGENT_ABORTED, AGENT_COMPLETED, AGENT_FAILED, AGENT_META, AGENT_SESSION, AGENT_SESSION_FOREIGN,
    COLLAB_CLOSE_AGENT, COLLAB_RESUME_AGENT, COLLAB_SEND_INPUT, COLLAB_SPAWN_AGENT, COLLAB_WAIT,
    CONTEXT_COMPACTED, CONTEXT_COMPACTED_DUPLICATE, ERROR, FILE_CHANGE, INFO_TOKENS, MCP_CALL,
    MCP_RESULT, MESSAGE_COMMENTARY, MESSAGE_USER, PATCH_APPLY, PATCH_APPLY_DUPLICATE, RAW_UNPARSED,
    RUNTIME_CONTEXT, SHELL_CALL, SHELL_RESULT, STDERR_LINE, STDIN_WRITE, TASK_COMPLETED,
    TASK_STARTED, THREAD_STARTED, TODO_UPDATE, TOOL_CALL, TOOL_RESULT, USER_INPUT_REQUEST,
    WEB_OPEN, WEB_SEARCH,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EventSummaryCategory {
    Default,
    Assistant,
    Command,
    Search,
    Subagent,
    File,
    Todo,
    Error,
}

const DEFAULT_TIMELINE_LIMIT: usize = 12;
const DEFAULT_AGENT_LINE_LIMIT: usize = 4;
const AGENT_PALETTE: &[&str] = &[
    "#e76f51", "#f4a261", "#e9c46a", "#90be6d", "#43aa8b", "#4d908e", "#577590", "#277da1",
    "#9b5de5", "#f15bb5", "#ff006e", "#fb5607", "#3a86ff", "#06d6a0", "#8ecae6", "#bc6c25",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineEntry {
    pub ts: String,
    pub label: String,
    pub event_kind: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSnapshot {
    pub thread_id: String,
    pub parent_thread_id: Option<String>,
    pub status: String,
    pub nickname: Option<String>,
    pub role: Option<String>,
    pub cwd: Option<String>,
    pub color: Option<String>,
    pub pending_response_category: Option<EventSummaryCategory>,
    pub recent_lines: VecDeque<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunSnapshot {
    pub task_id: Option<String>,
    pub task_title: Option<String>,
    pub run_id: Option<String>,
    pub cwd: Option<String>,
    pub status: String,
    pub root_thread_id: Option<String>,
    pub agents: HashMap<String, AgentSnapshot>,
    pub timeline: VecDeque<TimelineEntry>,
}

impl RunSnapshot {
    fn empty() -> Self {
        Self {
            task_id: None,
            task_title: None,
            run_id: None,
            cwd: None,
            status: "idle".to_string(),
            root_thread_id: None,
            agents: HashMap::new(),
            timeline: VecDeque::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EventProjector {
    pub timeline_limit: usize,
    pub agent_line_limit: usize,
    pub snapshot: RunSnapshot,
    agent_palette: VecDeque<String>,
    claimed_agent_colors: HashMap<String, String>,
}

impl Default for EventProjector {
    fn default() -> Self {
        Self::new(DEFAULT_TIMELINE_LIMIT, DEFAULT_AGENT_LINE_LIMIT)
    }
}

impl EventProjector {
    pub fn new(timeline_limit: usize, agent_line_limit: usize) -> Self {
        Self {
            timeline_limit,
            agent_line_limit,
            snapshot: RunSnapshot::empty(),
            agent_palette: AGENT_PALETTE
                .iter()
                .map(|value| value.to_string())
                .collect(),
            claimed_agent_colors: HashMap::new(),
        }
    }

    pub fn reset_run(
        &mut self,
        task_id: impl Into<String>,
        task_title: impl Into<String>,
        run_id: impl Into<String>,
        cwd: Option<String>,
    ) {
        let task_id = task_id.into();
        let task_title = task_title.into();
        let run_id = run_id.into();

        let preserved: Vec<TimelineEntry> = if self.snapshot.task_id.as_deref()
            == Some(task_id.as_str())
            && self.snapshot.run_id.as_deref() == Some(run_id.as_str())
        {
            self.snapshot
                .timeline
                .iter()
                .filter(|entry| entry.event_kind == "claim")
                .cloned()
                .collect()
        } else {
            Vec::new()
        };

        for color in self.claimed_agent_colors.values() {
            self.agent_palette.push_back(color.clone());
        }
        self.claimed_agent_colors.clear();

        self.snapshot = RunSnapshot {
            task_id: Some(task_id.clone()),
            task_title: Some(task_title),
            run_id: Some(run_id),
            cwd: cwd.clone(),
            status: "running".to_string(),
            root_thread_id: None,
            agents: HashMap::new(),
            timeline: VecDeque::new(),
        };
        for entry in preserved {
            self.push_entry(entry);
        }
        self.push_timeline(
            "start",
            format!(
                "start: {task_id} cwd={}",
                cwd.unwrap_or_else(|| ".".to_string())
            ),
            "",
        );
    }

    pub fn set_claimed(
        &mut self,
        task_id: impl Into<String>,
        task_title: impl Into<String>,
        run_id: impl Into<String>,
    ) {
        let task_id = task_id.into();
        let task_title = task_title.into();
        let run_id = run_id.into();
        self.snapshot.task_id = Some(task_id);
        self.snapshot.task_title = Some(task_title);
        self.snapshot.run_id = Some(run_id);
        self.snapshot.status = "claimed".to_string();
        self.push_timeline(
            "claim",
            format!(
                "claim: {} -> {}",
                self.snapshot.task_id.clone().unwrap_or_default(),
                self.snapshot.run_id.clone().unwrap_or_default()
            ),
            "",
        );
    }

    pub fn set_status(&mut self, status: impl Into<String>, label: Option<String>) {
        self.snapshot.status = status.into();
        if let Some(label) = label {
            self.push_timeline(self.snapshot.status.clone(), label, "");
        }
    }

    pub fn push_timeline(
        &mut self,
        event_kind: impl Into<String>,
        label: impl Into<String>,
        ts: impl Into<String>,
    ) {
        self.push_entry(TimelineEntry {
            ts: ts.into(),
            label: truncate_text(&label.into(), 160),
            event_kind: event_kind.into(),
        });
    }

    pub fn apply_event(&mut self, event: &EventRecord) {
        self.apply_event_state(event);
        self.push_timeline(
            event.event_type.clone(),
            summarize_event(event),
            event.ts.clone(),
        );
    }

    pub fn apply_events<'a, I>(&mut self, events: I) -> &RunSnapshot
    where
        I: IntoIterator<Item = &'a EventRecord>,
    {
        for event in events {
            self.apply_event(event);
        }
        &self.snapshot
    }

    fn push_entry(&mut self, entry: TimelineEntry) {
        if self.snapshot.timeline.len() == self.timeline_limit {
            self.snapshot.timeline.pop_front();
        }
        self.snapshot.timeline.push_back(entry);
    }

    fn ensure_agent(
        &mut self,
        thread_id: &str,
        parent_thread_id: Option<&str>,
    ) -> &mut AgentSnapshot {
        let parent_thread_id = normalize_parent_thread_id(parent_thread_id);
        let agent = self
            .snapshot
            .agents
            .entry(thread_id.to_string())
            .or_insert_with(|| AgentSnapshot {
                thread_id: thread_id.to_string(),
                parent_thread_id: parent_thread_id.map(str::to_string),
                status: "idle".to_string(),
                nickname: None,
                role: None,
                cwd: None,
                color: None,
                pending_response_category: None,
                recent_lines: VecDeque::new(),
            });
        if agent.parent_thread_id.is_none() {
            agent.parent_thread_id = parent_thread_id.map(str::to_string);
        }
        agent
    }

    fn set_agent_parent(
        &mut self,
        thread_id: &str,
        parent_thread_id: Option<&str>,
        authoritative: bool,
    ) {
        let Some(parent_thread_id) = normalize_parent_thread_id(parent_thread_id) else {
            return;
        };
        let agent = self.ensure_agent(thread_id, None);
        if authoritative || agent.parent_thread_id.is_none() {
            agent.parent_thread_id = Some(parent_thread_id.to_string());
        }
    }

    fn claim_agent_color(&mut self, thread_id: &str) -> Option<String> {
        if let Some(color) = self.claimed_agent_colors.get(thread_id) {
            return Some(color.clone());
        }
        let color = self.agent_palette.pop_front()?;
        self.claimed_agent_colors
            .insert(thread_id.to_string(), color.clone());
        Some(color)
    }

    fn assign_subagent_color(&mut self, thread_id: &str) {
        if self.snapshot.root_thread_id.as_deref() == Some(thread_id) {
            return;
        }
        if self.ensure_agent(thread_id, None).color.is_some() {
            return;
        }
        let color = self.claim_agent_color(thread_id);
        self.ensure_agent(thread_id, None).color = color;
    }

    fn release_subagent_color(&mut self, thread_id: &str) {
        let Some(agent) = self.snapshot.agents.get_mut(thread_id) else {
            return;
        };
        if agent.color.is_none() {
            return;
        }
        if let Some(color) = self.claimed_agent_colors.remove(thread_id) {
            self.agent_palette.push_front(color);
        }
        agent.color = None;
    }

    fn append_agent_line(&mut self, thread_id: &str, line: &str, parent_thread_id: Option<&str>) {
        let agent_line_limit = self.agent_line_limit;
        let agent = self.ensure_agent(thread_id, parent_thread_id);
        if agent.recent_lines.len() == agent_line_limit {
            agent.recent_lines.pop_front();
        }
        agent.recent_lines.push_back(truncate_text(line, 140));
    }

    fn set_pending_response_category(
        &mut self,
        thread_id: &str,
        parent_thread_id: Option<&str>,
        category: Option<EventSummaryCategory>,
    ) {
        self.ensure_agent(thread_id, parent_thread_id)
            .pending_response_category = category;
    }

    pub fn display_category(&self, event: &EventRecord) -> EventSummaryCategory {
        let fallback = categorize_event(event);
        match event.event_type.as_str() {
            event_type if is_non_user_message_event_type(event_type) => thread_id(event)
                .or_else(|| self.snapshot.root_thread_id.clone())
                .and_then(|thread_id| {
                    self.snapshot
                        .agents
                        .get(&thread_id)
                        .and_then(|agent| agent.pending_response_category)
                })
                .unwrap_or(fallback),
            "agent.meta" if actor_type(event) == "subagent" => {
                let meta_type =
                    payload_string(event.payload.as_object(), "meta_type").unwrap_or_default();
                if matches!(meta_type.as_str(), "message" | "task_complete") {
                    thread_id(event)
                        .and_then(|thread_id| {
                            self.snapshot
                                .agents
                                .get(&thread_id)
                                .and_then(|agent| agent.pending_response_category)
                        })
                        .unwrap_or(fallback)
                } else {
                    fallback
                }
            }
            _ => fallback,
        }
    }

    fn apply_event_state(&mut self, event: &EventRecord) {
        let payload = event.payload.as_object();
        match event.event_type.as_str() {
            THREAD_STARTED => {
                let thread_id = payload
                    .and_then(|p| p.get("thread_id"))
                    .and_then(Value::as_str)
                    .unwrap_or("root")
                    .to_string();
                self.snapshot.root_thread_id = Some(thread_id.clone());
                self.ensure_agent(&thread_id, None).status = "running".to_string();
            }
            event_type if is_non_user_message_event_type(event_type) => {
                let thread_id = thread_id(event).or_else(|| self.snapshot.root_thread_id.clone());
                if let Some(thread_id) = thread_id {
                    let text = payload_string(payload, "text").unwrap_or_default();
                    let parent_thread_id = payload_string(payload, "parent_thread_id");
                    self.append_agent_line(&thread_id, &text, parent_thread_id.as_deref());
                    if !is_commentary_message_event(event_type, payload) {
                        self.set_pending_response_category(
                            &thread_id,
                            parent_thread_id.as_deref(),
                            None,
                        );
                    }
                }
            }
            AGENT_COMPLETED => {
                if let Some(root_thread_id) = self.snapshot.root_thread_id.clone() {
                    self.snapshot.status = "completed".to_string();
                    self.ensure_agent(&root_thread_id, None).status = "completed".to_string();
                }
            }
            AGENT_FAILED => {
                if let Some(root_thread_id) = self.snapshot.root_thread_id.clone() {
                    self.snapshot.status = "failed".to_string();
                    self.ensure_agent(&root_thread_id, None).status = "failed".to_string();
                }
            }
            kind if (kind == TOOL_RESULT || is_collab_event_type(kind))
                && payload_has_non_empty_array(payload, "receiver_thread_ids") =>
            {
                let sender = thread_id(event)
                    .or_else(|| self.snapshot.root_thread_id.clone())
                    .unwrap_or_else(|| "root".to_string());
                self.ensure_agent(&sender, None);
                self.set_pending_response_category(
                    &sender,
                    None,
                    Some(tool_event_category(payload)),
                );
                let tool_name = payload_string(payload, "tool_name").unwrap_or_default();
                if let Some(receivers) = payload_array(payload, "receiver_thread_ids") {
                    for receiver in receivers {
                        let Some(receiver_id) = receiver.as_str() else {
                            continue;
                        };
                        self.assign_subagent_color(receiver_id);
                        let status = payload_string(payload, "status");
                        let agent = self.ensure_agent(receiver_id, Some(&sender));
                        if let Some(status) = status.clone() {
                            agent.status = status;
                        }
                        if tool_name == "close_agent"
                            || matches!(agent.status.as_str(), "completed" | "failed" | "cancelled")
                        {
                            self.release_subagent_color(receiver_id);
                        }
                    }
                }
                if let Some(states) = payload_object(payload, "agents_states") {
                    let state_ids: Vec<String> = states.keys().cloned().collect();
                    for state_thread_id in state_ids {
                        let Some(raw_state) =
                            states.get(&state_thread_id).and_then(Value::as_object)
                        else {
                            continue;
                        };
                        self.assign_subagent_color(&state_thread_id);
                        let status = raw_state
                            .get("status")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        let message = raw_state
                            .get("message")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        let agent = self.ensure_agent(&state_thread_id, Some(&sender));
                        if let Some(status) = status {
                            agent.status = status;
                        }
                        if let Some(message) = message {
                            self.append_agent_line(&state_thread_id, &message, Some(&sender));
                        }
                        let status = self
                            .snapshot
                            .agents
                            .get(&state_thread_id)
                            .map(|agent| agent.status.clone())
                            .unwrap_or_default();
                        if matches!(status.as_str(), "completed" | "failed" | "cancelled") {
                            self.release_subagent_color(&state_thread_id);
                        }
                    }
                }
            }
            AGENT_SESSION => {
                let thread_id = thread_id(event).unwrap_or_else(|| "unknown".to_string());
                self.assign_subagent_color(&thread_id);
                let parent_thread_id = payload_string(payload, "parent_thread_id");
                self.set_agent_parent(&thread_id, parent_thread_id.as_deref(), true);
                let nickname = payload_string(payload, "agent_nickname");
                let role = payload_string(payload, "agent_role");
                let cwd = payload_string(payload, "cwd");
                let agent = self.ensure_agent(&thread_id, None);
                agent.nickname = nickname;
                agent.role = role;
                agent.cwd = cwd;
                if agent.status == "idle" {
                    agent.status = "session_loaded".to_string();
                }
            }
            TOOL_CALL | SHELL_CALL if actor_type(event) == "subagent" => {
                let thread_id = thread_id(event).unwrap_or_else(|| "unknown".to_string());
                let parent_thread_id = payload_string(payload, "parent_thread_id");
                let tool_name =
                    payload_string(payload, "tool_name").unwrap_or_else(|| "unknown".to_string());
                self.assign_subagent_color(&thread_id);
                self.append_agent_line(
                    &thread_id,
                    &format!("tool: {tool_name}"),
                    parent_thread_id.as_deref(),
                );
                self.ensure_agent(&thread_id, parent_thread_id.as_deref())
                    .status = "working".to_string();
                self.set_pending_response_category(
                    &thread_id,
                    parent_thread_id.as_deref(),
                    Some(tool_event_category(payload)),
                );
            }
            TOOL_RESULT | SHELL_RESULT if actor_type(event) == "subagent" => {
                let thread_id = thread_id(event).unwrap_or_else(|| "unknown".to_string());
                let parent_thread_id = payload_string(payload, "parent_thread_id");
                let tool_name =
                    payload_string(payload, "tool_name").unwrap_or_else(|| "unknown".to_string());
                let output = payload
                    .and_then(|p| p.get("output"))
                    .map(stringify_tool_detail)
                    .unwrap_or_default();
                let output = output.trim();
                let line = if output.is_empty() {
                    format!("result {tool_name}: ok")
                } else {
                    format!("result {tool_name}: {output}")
                };
                self.assign_subagent_color(&thread_id);
                self.append_agent_line(&thread_id, &line, parent_thread_id.as_deref());
                self.set_pending_response_category(
                    &thread_id,
                    parent_thread_id.as_deref(),
                    Some(tool_event_category(payload)),
                );
            }
            TODO_UPDATE
                if actor_type(event) == "subagent" && is_update_plan_todo_payload(payload) =>
            {
                let thread_id = thread_id(event).unwrap_or_else(|| "unknown".to_string());
                let parent_thread_id = payload_string(payload, "parent_thread_id");
                let detail = plan_update_detail(payload, true);
                let line = if detail.is_empty() {
                    "todo update".to_string()
                } else {
                    format!("todo update: {detail}")
                };
                self.assign_subagent_color(&thread_id);
                self.append_agent_line(&thread_id, &line, parent_thread_id.as_deref());
                if payload_phase(payload).as_deref() == Some("started") {
                    self.ensure_agent(&thread_id, parent_thread_id.as_deref())
                        .status = "working".to_string();
                }
                self.set_pending_response_category(
                    &thread_id,
                    parent_thread_id.as_deref(),
                    Some(EventSummaryCategory::Default),
                );
            }
            USER_INPUT_REQUEST if actor_type(event) == "subagent" => {
                let thread_id = thread_id(event).unwrap_or_else(|| "unknown".to_string());
                let parent_thread_id = payload_string(payload, "parent_thread_id");
                let detail = user_input_request_detail(payload, true);
                let line = if detail.is_empty() {
                    "user input request".to_string()
                } else {
                    format!("user input request: {detail}")
                };
                self.assign_subagent_color(&thread_id);
                self.append_agent_line(&thread_id, &line, parent_thread_id.as_deref());
                if payload_phase(payload).as_deref() == Some("started") {
                    self.ensure_agent(&thread_id, parent_thread_id.as_deref())
                        .status = "working".to_string();
                }
                self.set_pending_response_category(
                    &thread_id,
                    parent_thread_id.as_deref(),
                    Some(EventSummaryCategory::Default),
                );
            }
            STDIN_WRITE if actor_type(event) == "subagent" => {
                let thread_id = thread_id(event).unwrap_or_else(|| "unknown".to_string());
                let parent_thread_id = payload_string(payload, "parent_thread_id");
                let detail = stdin_write_detail(payload, true);
                let line = if detail.is_empty() {
                    "stdin write".to_string()
                } else {
                    format!("stdin write: {detail}")
                };
                self.assign_subagent_color(&thread_id);
                self.append_agent_line(&thread_id, &line, parent_thread_id.as_deref());
                if payload_phase(payload).as_deref() == Some("started") {
                    self.ensure_agent(&thread_id, parent_thread_id.as_deref())
                        .status = "working".to_string();
                }
                self.set_pending_response_category(
                    &thread_id,
                    parent_thread_id.as_deref(),
                    Some(EventSummaryCategory::Command),
                );
            }
            PATCH_APPLY if actor_type(event) == "subagent" => {
                let thread_id = thread_id(event).unwrap_or_else(|| "unknown".to_string());
                let parent_thread_id = payload_string(payload, "parent_thread_id");
                let detail = patch_apply_detail(payload, true);
                let line = if detail.is_empty() {
                    "patch apply".to_string()
                } else {
                    format!("patch apply: {detail}")
                };
                self.assign_subagent_color(&thread_id);
                self.append_agent_line(&thread_id, &line, parent_thread_id.as_deref());
                self.ensure_agent(&thread_id, parent_thread_id.as_deref())
                    .status = if is_failed_tool_result(payload) {
                    "failed".to_string()
                } else if payload_phase(payload).as_deref() == Some("completed") {
                    "working".to_string()
                } else {
                    "working".to_string()
                };
                self.set_pending_response_category(
                    &thread_id,
                    parent_thread_id.as_deref(),
                    Some(EventSummaryCategory::File),
                );
            }
            TASK_STARTED => {
                let thread_id = thread_id(event).unwrap_or_else(|| "unknown".to_string());
                let parent_thread_id = payload_string(payload, "parent_thread_id");
                self.assign_subagent_color(&thread_id);
                self.ensure_agent(&thread_id, parent_thread_id.as_deref())
                    .status = "running".to_string();
            }
            TASK_COMPLETED => {
                let thread_id = thread_id(event).unwrap_or_else(|| "unknown".to_string());
                let parent_thread_id = payload_string(payload, "parent_thread_id");
                self.assign_subagent_color(&thread_id);
                self.ensure_agent(&thread_id, parent_thread_id.as_deref())
                    .status = "completed".to_string();
                if let Some(last_message) = payload_string(payload, "last_agent_message") {
                    if !last_message.trim().is_empty() {
                        self.append_agent_line(
                            &thread_id,
                            &last_message,
                            parent_thread_id.as_deref(),
                        );
                    }
                }
                self.ensure_agent(&thread_id, parent_thread_id.as_deref())
                    .pending_response_category = None;
                self.release_subagent_color(&thread_id);
            }
            MESSAGE_USER => {
                let thread_id = thread_id(event).unwrap_or_else(|| "unknown".to_string());
                let parent_thread_id = payload_string(payload, "parent_thread_id");
                self.assign_subagent_color(&thread_id);
                if let Some(text) = payload_string(payload, "text") {
                    if !text.trim().is_empty() {
                        self.append_agent_line(
                            &thread_id,
                            &format!("user: {text}"),
                            parent_thread_id.as_deref(),
                        );
                    }
                }
            }
            RUNTIME_CONTEXT => {
                let thread_id = thread_id(event).unwrap_or_else(|| "unknown".to_string());
                let parent_thread_id = payload_string(payload, "parent_thread_id");
                self.assign_subagent_color(&thread_id);
                let cwd = payload_string(payload, "cwd");
                if let Some(cwd) = cwd {
                    let agent = self.ensure_agent(&thread_id, parent_thread_id.as_deref());
                    if agent.cwd.is_none() {
                        agent.cwd = Some(cwd);
                    }
                }
            }
            AGENT_ABORTED => {
                if let Some(thread_id) = thread_id(event) {
                    self.ensure_agent(&thread_id, None).status = "interrupted".to_string();
                }
            }
            AGENT_META if actor_type(event) == "subagent" => {
                let thread_id = thread_id(event).unwrap_or_else(|| "unknown".to_string());
                self.assign_subagent_color(&thread_id);
                let parent_thread_id = payload_string(payload, "parent_thread_id");
                let meta_type =
                    payload_string(payload, "meta_type").unwrap_or_else(|| "meta".to_string());
                match meta_type.as_str() {
                    "task_started" => {
                        self.ensure_agent(&thread_id, parent_thread_id.as_deref())
                            .status = "running".to_string();
                    }
                    "task_complete" => {
                        self.ensure_agent(&thread_id, parent_thread_id.as_deref())
                            .status = "completed".to_string();
                        if let Some(last_message) = payload_string(payload, "last_agent_message") {
                            if !last_message.trim().is_empty() {
                                self.append_agent_line(
                                    &thread_id,
                                    &last_message,
                                    parent_thread_id.as_deref(),
                                );
                            }
                        }
                        self.ensure_agent(&thread_id, parent_thread_id.as_deref())
                            .pending_response_category = None;
                        self.release_subagent_color(&thread_id);
                    }
                    "user_message" => {
                        if let Some(text) = payload_string(payload, "text") {
                            if !text.trim().is_empty() {
                                self.append_agent_line(
                                    &thread_id,
                                    &format!("user: {text}"),
                                    parent_thread_id.as_deref(),
                                );
                            }
                        }
                    }
                    _ => {}
                }
            }
            kind if is_root_toolish_event_type(kind) => {
                let thread_id = thread_id(event)
                    .or_else(|| self.snapshot.root_thread_id.clone())
                    .unwrap_or_else(|| "root".to_string());
                self.set_pending_response_category(
                    &thread_id,
                    None,
                    Some(tool_event_category(payload)),
                );
            }
            _ => {}
        }
    }
}

pub fn truncate_text(value: &str, limit: usize) -> String {
    let normalized = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= limit {
        return normalized;
    }
    let keep = limit.saturating_sub(3);
    let mut clipped = String::new();
    for ch in normalized.chars().take(keep) {
        clipped.push(ch);
    }
    format!("{clipped}...")
}

pub fn summarize_event(event: &EventRecord) -> String {
    summarize_event_impl(event, false)
}

pub fn summarize_event_full(event: &EventRecord) -> String {
    summarize_event_impl(event, true)
}

fn summarize_event_impl(event: &EventRecord, full: bool) -> String {
    let payload = event.payload.as_object();
    match event.event_type.as_str() {
        event_type if is_non_user_message_event_type(event_type) => summary_text(
            &payload_string(payload, "text").unwrap_or_default(),
            100,
            full,
        ),
        AGENT_SESSION => {
            let details = agent_session_detail(payload, full);
            if details.is_empty() {
                "ready".to_string()
            } else {
                format!("ready: {details}")
            }
        }
        AGENT_SESSION_FOREIGN => {
            let mut parts = Vec::new();
            if let Some(foreign_thread_id) = payload_string(payload, "foreign_thread_id") {
                if !foreign_thread_id.is_empty() {
                    parts.push(format!("thread={foreign_thread_id}"));
                }
            }
            let details = agent_session_detail(payload, full);
            if !details.is_empty() {
                parts.push(details);
            }
            if parts.is_empty() {
                "foreign session meta".to_string()
            } else {
                format!("foreign session meta: {}", parts.join(" "))
            }
        }
        AGENT_META => {
            let meta_type =
                payload_string(payload, "meta_type").unwrap_or_else(|| "meta".to_string());
            match meta_type.as_str() {
                "message" | "user_message" => summary_text(
                    &payload_string(payload, "text").unwrap_or_default(),
                    100,
                    full,
                ),
                "task_started" => {
                    let detail = task_started_detail(payload, full);
                    if detail.is_empty() {
                        "started".to_string()
                    } else {
                        format!("started: {detail}")
                    }
                }
                "task_complete" => {
                    let text = summary_text(
                        &payload_string(payload, "last_agent_message").unwrap_or_default(),
                        80,
                        full,
                    );
                    if text.is_empty() {
                        "done".to_string()
                    } else {
                        format!("done: {text}")
                    }
                }
                "turn_context" => {
                    let cwd = payload_string(payload, "cwd").unwrap_or_default();
                    if cwd.is_empty() {
                        "turn context".to_string()
                    } else {
                        format!("turn context: cwd={}", summary_text(&cwd, 60, full))
                    }
                }
                _ => format!("meta {meta_type}"),
            }
        }
        MESSAGE_USER => summary_text(
            &payload_string(payload, "text").unwrap_or_default(),
            100,
            full,
        ),
        TASK_STARTED => {
            let detail = task_started_detail(payload, full);
            if detail.is_empty() {
                "started".to_string()
            } else {
                format!("started: {detail}")
            }
        }
        TASK_COMPLETED => {
            let text = summary_text(
                &payload_string(payload, "last_agent_message").unwrap_or_default(),
                80,
                full,
            );
            if text.is_empty() {
                "done".to_string()
            } else {
                format!("done: {text}")
            }
        }
        RUNTIME_CONTEXT => {
            let detail = runtime_context_detail(payload, full);
            if detail.is_empty() {
                "context".to_string()
            } else {
                format!("context: {detail}")
            }
        }
        CONTEXT_COMPACTED => {
            let detail = context_compacted_detail(payload, full);
            if detail.is_empty() {
                "context compacted".to_string()
            } else {
                format!("context compacted: {detail}")
            }
        }
        CONTEXT_COMPACTED_DUPLICATE => {
            let detail = payload_string(payload, "duplicate_of").unwrap_or_default();
            if detail.is_empty() {
                "context compacted duplicate".to_string()
            } else {
                format!("context compacted duplicate: source={detail}")
            }
        }
        INFO_TOKENS => {
            let detail = info_tokens_detail(payload);
            if detail.is_empty() {
                "tokens".to_string()
            } else {
                format!("tokens: {detail}")
            }
        }
        TOOL_CALL | SHELL_CALL => {
            if actor_type(event) == "subagent" {
                let tool_name =
                    payload_string(payload, "tool_name").unwrap_or_else(|| "?".to_string());
                let detail = if is_web_tool_name(&tool_name) {
                    web_event_detail(payload, full)
                } else if let Some(command) = command_from_payload(payload) {
                    summarize_command_text(&command, 120, full)
                } else {
                    String::new()
                };
                if detail.is_empty() {
                    format!("tool {tool_name}")
                } else {
                    format!("tool {tool_name}: {detail}")
                }
            } else {
                format_root_tool_event(payload, false, full)
            }
        }
        TOOL_RESULT | SHELL_RESULT => {
            if actor_type(event) == "subagent" {
                let output = payload
                    .and_then(|obj| obj.get("output"))
                    .map(stringify_tool_detail)
                    .unwrap_or_default()
                    .trim()
                    .replace('\n', " ");
                if output.is_empty() {
                    format!(
                        "result {}",
                        payload_string(payload, "tool_name").unwrap_or_else(|| "?".to_string())
                    )
                } else {
                    format!(
                        "result {}: {}",
                        payload_string(payload, "tool_name").unwrap_or_else(|| "?".to_string()),
                        summary_text(&output, 60, full)
                    )
                }
            } else {
                format_root_tool_event(payload, true, full)
            }
        }
        MCP_CALL => format_mcp_event(payload, false, full),
        MCP_RESULT => format_mcp_event(payload, true, full),
        STDIN_WRITE => {
            let detail = stdin_write_detail(payload, full);
            if actor_type(event) == "subagent" {
                if detail.is_empty() {
                    "stdin write".to_string()
                } else {
                    format!("stdin write: {detail}")
                }
            } else if detail.is_empty() {
                "stdin write".to_string()
            } else {
                format!("stdin write: {detail}")
            }
        }
        TODO_UPDATE if is_update_plan_todo_payload(payload) => {
            let detail = plan_update_detail(payload, full);
            if actor_type(event) == "subagent" {
                if detail.is_empty() {
                    "todo update".to_string()
                } else {
                    format!("todo update: {detail}")
                }
            } else if detail.is_empty() {
                "todo update".to_string()
            } else {
                format!("todo update: {detail}")
            }
        }
        USER_INPUT_REQUEST => {
            let detail = user_input_request_detail(payload, full);
            if actor_type(event) == "subagent" {
                if detail.is_empty() {
                    "user input request".to_string()
                } else {
                    format!("user input request: {detail}")
                }
            } else if detail.is_empty() {
                "user input request".to_string()
            } else {
                format!("user input request: {detail}")
            }
        }
        PATCH_APPLY => {
            let detail = patch_apply_detail(payload, full);
            if detail.is_empty() {
                "patch apply".to_string()
            } else {
                format!("patch apply: {detail}")
            }
        }
        PATCH_APPLY_DUPLICATE => {
            let detail = patch_apply_detail(payload, full);
            if detail.is_empty() {
                "patch apply duplicate".to_string()
            } else {
                format!("patch apply duplicate: {detail}")
            }
        }
        WEB_SEARCH | WEB_OPEN | COLLAB_SPAWN_AGENT | COLLAB_SEND_INPUT | COLLAB_WAIT
        | COLLAB_CLOSE_AGENT | COLLAB_RESUME_AGENT => format_root_tool_event(
            payload,
            payload_phase(payload).as_deref() == Some("completed"),
            full,
        ),
        FILE_CHANGE => {
            let Some(changes) = payload_array(payload, "changes") else {
                return "files changed".to_string();
            };
            let Some(first) = changes.first().and_then(Value::as_object) else {
                return "files changed".to_string();
            };
            let kind = first
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("change");
            let path = first
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or_default();
            let suffix = if path.is_empty() {
                String::new()
            } else {
                format!(" {}", summary_text(path, 80, full))
            };
            let extra = if changes.len() > 1 {
                format!(" (+{})", changes.len() - 1)
            } else {
                String::new()
            };
            format!("files: {kind}{suffix}{extra}")
        }
        TODO_UPDATE => {
            let phase = payload_string(payload, "phase").unwrap_or_default();
            let completed = payload
                .and_then(|obj| obj.get("completed_count"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let total = payload
                .and_then(|obj| obj.get("total_count"))
                .and_then(Value::as_u64)
                .unwrap_or(0);
            let next_item = payload_array(payload, "items")
                .and_then(|items| {
                    items.iter().find_map(|item| {
                        let item = item.as_object()?;
                        if item
                            .get("completed")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                        {
                            return None;
                        }
                        item.get("text")
                            .and_then(Value::as_str)
                            .map(|text| summary_text(text, 80, full))
                    })
                })
                .unwrap_or_default();
            let mut summary = format!("todo {phase}: {completed}/{total}");
            if !next_item.is_empty() {
                summary.push_str(&format!(" next={next_item}"));
            }
            summary
        }
        STDERR_LINE => {
            format!(
                "stderr: {}",
                summary_text(
                    &payload_string(payload, "text").unwrap_or_default(),
                    100,
                    full
                )
            )
        }
        ERROR => {
            format!(
                "error: {}{}",
                summary_text(
                    &payload_string(payload, "message").unwrap_or_default(),
                    100,
                    full
                ),
                text_links_suffix(payload)
            )
        }
        AGENT_FAILED => {
            let message = payload_string(payload, "message")
                .or_else(|| {
                    payload
                        .and_then(|obj| obj.get("error"))
                        .and_then(Value::as_object)
                        .and_then(|obj| obj.get("message"))
                        .and_then(Value::as_str)
                        .map(str::to_string)
                })
                .unwrap_or_default();
            if message.is_empty() {
                format!("agent failed{}", text_links_suffix(payload))
            } else {
                format!(
                    "agent failed: {}{}",
                    summary_text(&message, 100, full),
                    text_links_suffix(payload)
                )
            }
        }
        AGENT_ABORTED => {
            let reason = payload_string(payload, "reason").unwrap_or_default();
            if reason.is_empty() {
                "agent aborted".to_string()
            } else {
                format!("agent aborted: reason={reason}")
            }
        }
        _ => event.event_type.clone(),
    }
}

pub fn load_event_records(path: &Path) -> std::io::Result<Vec<EventRecord>> {
    EventLogFileReader::load_records(path)
}

pub fn categorize_event(event: &EventRecord) -> EventSummaryCategory {
    let payload = event.payload.as_object();
    match event.event_type.as_str() {
        event_type if is_commentary_message_event(event_type, payload) => {
            EventSummaryCategory::Subagent
        }
        event_type if is_non_user_message_event_type(event_type) => EventSummaryCategory::Assistant,
        AGENT_SESSION
        | AGENT_SESSION_FOREIGN
        | AGENT_META
        | MESSAGE_USER
        | TASK_STARTED
        | TASK_COMPLETED
        | RUNTIME_CONTEXT
        | CONTEXT_COMPACTED
        | CONTEXT_COMPACTED_DUPLICATE
        | INFO_TOKENS
        | PATCH_APPLY_DUPLICATE => EventSummaryCategory::Subagent,
        TOOL_CALL | TOOL_RESULT | SHELL_CALL | SHELL_RESULT | MCP_CALL | MCP_RESULT
        | STDIN_WRITE | WEB_SEARCH | WEB_OPEN | COLLAB_SPAWN_AGENT | COLLAB_SEND_INPUT
        | COLLAB_WAIT | COLLAB_CLOSE_AGENT | COLLAB_RESUME_AGENT => tool_event_category(payload),
        USER_INPUT_REQUEST => EventSummaryCategory::Default,
        PATCH_APPLY | FILE_CHANGE => EventSummaryCategory::File,
        TODO_UPDATE => {
            if is_update_plan_todo_payload(payload) {
                EventSummaryCategory::Default
            } else {
                EventSummaryCategory::Todo
            }
        }
        STDERR_LINE | ERROR | AGENT_FAILED | AGENT_ABORTED | RAW_UNPARSED => {
            EventSummaryCategory::Error
        }
        _ => EventSummaryCategory::Default,
    }
}

fn is_non_user_message_event_type(event_type: &str) -> bool {
    event_type.starts_with("message.") && event_type != MESSAGE_USER
}

fn is_commentary_message_event(event_type: &str, payload: Option<&Map<String, Value>>) -> bool {
    is_non_user_message_event_type(event_type)
        && (event_type == MESSAGE_COMMENTARY
            || payload_phase(payload).as_deref() == Some("commentary"))
}

fn format_tool_event(
    prefix: &str,
    payload: Option<&Map<String, Value>>,
    is_result: bool,
    full: bool,
) -> String {
    let tool_name = payload_string(payload, "tool_name").unwrap_or_else(|| "?".to_string());
    let mut label = format!("{prefix} [{tool_name}]");
    let detail = if has_subagent_state(payload) {
        subagent_state_detail(payload, full)
    } else if is_web_tool_name(&tool_name) {
        web_event_detail(payload, full)
    } else if let Some(command) = command_from_payload(payload) {
        summarize_command_text(&command, 120, full)
    } else {
        String::new()
    };
    let mut has_detail = false;
    if !detail.is_empty() {
        append_summary_detail(&mut label, &detail, &mut has_detail);
    }
    if is_result {
        if let Some(exit_code) = payload.and_then(|obj| obj.get("exit_code")) {
            append_summary_detail(
                &mut label,
                &format!("exit={}", json_scalar_to_string(exit_code)),
                &mut has_detail,
            );
        }
        let diagnostic = tool_result_diagnostic(payload, full);
        if !diagnostic.is_empty() {
            if !has_detail {
                label.push_str(": ");
            } else {
                label.push_str(" -> ");
            }
            label.push_str(&diagnostic);
        }
    }
    label
}

fn tool_event_category(payload: Option<&Map<String, Value>>) -> EventSummaryCategory {
    let tool_name = payload_string(payload, "tool_name").unwrap_or_default();
    if subagent_tool_label(&tool_name).is_some() || has_subagent_state(payload) {
        EventSummaryCategory::Subagent
    } else if is_web_tool_name(&tool_name) {
        EventSummaryCategory::Search
    } else if is_command_tool(&tool_name, payload) {
        EventSummaryCategory::Command
    } else {
        EventSummaryCategory::Default
    }
}

fn format_root_tool_event(
    payload: Option<&Map<String, Value>>,
    is_result: bool,
    full: bool,
) -> String {
    let tool_name = payload_string(payload, "tool_name").unwrap_or_else(|| "?".to_string());
    if let Some(label) = subagent_tool_label(&tool_name) {
        return format_tool_event(label, payload, is_result, full);
    }
    if is_web_tool_name(&tool_name) {
        return format_tool_event(
            web_event_label(payload, is_result),
            payload,
            is_result,
            full,
        );
    }
    if is_command_tool(&tool_name, payload) {
        return format_tool_event(
            if is_result {
                if is_failed_tool_result(payload) {
                    "command fail"
                } else {
                    "command ok"
                }
            } else {
                "command"
            },
            payload,
            is_result,
            full,
        );
    }
    format_tool_event(
        if is_result { "tool result" } else { "tool" },
        payload,
        is_result,
        full,
    )
}

fn format_mcp_event(payload: Option<&Map<String, Value>>, is_result: bool, full: bool) -> String {
    let mut label = if is_result {
        "mcp result".to_string()
    } else {
        "mcp call".to_string()
    };
    let mut has_detail = false;

    if let Some(phase) = payload_string(payload, "phase") {
        if !phase.is_empty() {
            append_summary_detail(&mut label, &format!("phase={phase}"), &mut has_detail);
        }
    }
    if let Some(status) = payload_string(payload, "status") {
        if !status.is_empty() {
            append_summary_detail(&mut label, &format!("status={status}"), &mut has_detail);
        }
    }
    if let Some(arguments) = payload
        .and_then(|obj| obj.get("arguments"))
        .map(|value| render_mcp_value(value, full))
        .filter(|value| !value.is_empty())
    {
        append_summary_detail(
            &mut label,
            &format!("arguments={arguments}"),
            &mut has_detail,
        );
    }
    if let Some(result) = payload
        .and_then(|obj| obj.get("result"))
        .map(|value| render_mcp_value(value, full))
        .filter(|value| !value.is_empty())
    {
        append_summary_detail(&mut label, &format!("result={result}"), &mut has_detail);
    }
    if let Some(error) = payload
        .and_then(|obj| obj.get("error"))
        .map(|value| render_mcp_value(value, full))
        .filter(|value| !value.is_empty())
    {
        append_summary_detail(&mut label, &format!("error={error}"), &mut has_detail);
    }
    if let Some(server) = payload_string(payload, "server") {
        if !server.is_empty() {
            append_summary_detail(&mut label, &format!("server={server}"), &mut has_detail);
        }
    }
    if let Some(tool) = payload_string(payload, "tool") {
        if !tool.is_empty() {
            append_summary_detail(&mut label, &format!("tool={tool}"), &mut has_detail);
        }
    }

    label
}

fn command_from_payload(payload: Option<&Map<String, Value>>) -> Option<String> {
    let raw_input = payload
        .and_then(|obj| obj.get("input"))
        .and_then(Value::as_object)?;
    for key in ["command", "cmd"] {
        if let Some(value) = raw_input.get(key).and_then(Value::as_str) {
            let value = value.trim();
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn is_collab_event_type(event_type: &str) -> bool {
    matches!(
        event_type,
        COLLAB_SPAWN_AGENT
            | COLLAB_SEND_INPUT
            | COLLAB_WAIT
            | COLLAB_CLOSE_AGENT
            | COLLAB_RESUME_AGENT
    )
}

fn is_root_toolish_event_type(event_type: &str) -> bool {
    matches!(
        event_type,
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
            | COLLAB_SPAWN_AGENT
            | COLLAB_WAIT
    )
}

fn payload_phase(payload: Option<&Map<String, Value>>) -> Option<String> {
    payload_string(payload, "phase")
}

fn is_update_plan_todo_payload(payload: Option<&Map<String, Value>>) -> bool {
    payload
        .and_then(|obj| obj.get("tool_name"))
        .and_then(Value::as_str)
        .map(|tool_name| tool_name == "update_plan")
        .unwrap_or(false)
        || payload.and_then(|obj| obj.get("tool_use_id")).is_some()
        || payload
            .and_then(|obj| obj.get("input"))
            .and_then(Value::as_object)
            .map(plan_payload_has_content)
            .unwrap_or(false)
        || payload
            .and_then(|obj| obj.get("output"))
            .and_then(Value::as_object)
            .map(plan_payload_has_content)
            .unwrap_or(false)
}

fn plan_payload_has_content(payload: &Map<String, Value>) -> bool {
    payload.contains_key("plan")
        || payload.contains_key("explanation")
        || payload.contains_key("text")
}

fn truncate_command_text(command: &str, limit: usize) -> String {
    let lines: Vec<String> = command
        .lines()
        .map(|line| truncate_text(line, limit))
        .collect();
    if lines.is_empty() {
        String::new()
    } else {
        lines.join("\n")
    }
}

fn summarize_command_text(command: &str, limit: usize, full: bool) -> String {
    if full {
        return command.trim().to_string();
    }
    truncate_command_text(command, limit)
}

fn is_web_tool_name(tool_name: &str) -> bool {
    matches!(tool_name, "web_search" | "web_search_call")
}

fn web_event_label(payload: Option<&Map<String, Value>>, is_result: bool) -> &'static str {
    if is_open_page_payload(payload) {
        if is_result {
            "open page result"
        } else {
            "open page"
        }
    } else if is_result {
        "search result"
    } else {
        "search"
    }
}

fn web_event_container(payload: Option<&Map<String, Value>>) -> Option<&Map<String, Value>> {
    let input = payload
        .and_then(|obj| obj.get("input"))
        .and_then(Value::as_object);
    let output = payload
        .and_then(|obj| obj.get("output"))
        .and_then(Value::as_object);
    input.or(output)
}

fn web_action_type(payload: Option<&Map<String, Value>>) -> Option<String> {
    let action = web_event_container(payload)?
        .get("action")
        .cloned()
        .unwrap_or(Value::Null);
    match action {
        Value::String(value) => {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        }
        Value::Object(value) => value
            .get("type")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        _ => None,
    }
}

fn web_action_url(payload: Option<&Map<String, Value>>) -> Option<String> {
    web_event_container(payload)?
        .get("action")
        .and_then(Value::as_object)
        .and_then(|action| action.get("url"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn is_open_page_payload(payload: Option<&Map<String, Value>>) -> bool {
    web_action_type(payload).as_deref() == Some("open_page")
}

fn web_event_detail(payload: Option<&Map<String, Value>>, full: bool) -> String {
    let input = payload
        .and_then(|obj| obj.get("input"))
        .and_then(Value::as_object);
    let Some(container) = web_event_container(payload) else {
        return String::new();
    };
    let mut parts = Vec::new();
    let query = container
        .get("query")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let action_type = web_action_type(payload);
    let action_url = web_action_url(payload);

    if action_type.as_deref() == Some("open_page") {
        if let Some(url) = action_url.as_deref() {
            parts.push(format!("url={}", summary_text(url, 120, full)));
        }
        if let Some(query) = query
            .as_deref()
            .filter(|query| Some(*query) != action_url.as_deref())
        {
            parts.push(format!("query={}", summary_text(query, 120, full)));
        }
        if parts.is_empty() && input == Some(container) {
            parts.push("query=<pending>".to_string());
        }
        return parts.join(" ");
    }

    if let Some(query) = query.as_deref() {
        parts.push(format!("query={}", summary_text(query, 120, full)));
    } else if input == Some(container) {
        parts.push("query=<pending>".to_string());
    }
    if let Some(action_type) = action_type.as_deref() {
        parts.push(format!("action={}", summary_text(action_type, 40, full)));
    }
    if let Some(url) = action_url.as_deref() {
        parts.push(format!("url={}", summary_text(url, 120, full)));
    }
    parts.join(" ")
}

fn plan_update_detail(payload: Option<&Map<String, Value>>, full: bool) -> String {
    let mut parts = Vec::new();
    if let Some(phase) = payload_string(payload, "phase") {
        if !phase.is_empty() {
            parts.push(format!("phase={phase}"));
        }
    }

    let container = payload
        .and_then(|obj| obj.get("input"))
        .and_then(Value::as_object)
        .or_else(|| {
            payload
                .and_then(|obj| obj.get("output"))
                .and_then(Value::as_object)
        });

    if let Some(container) = container {
        if let Some(explanation) = container.get("explanation").and_then(Value::as_str) {
            if !explanation.trim().is_empty() {
                parts.push(format!(
                    "explanation={}",
                    summary_text(explanation.trim(), 80, full)
                ));
            }
        }
        if let Some(plan) = container.get("plan").and_then(Value::as_array) {
            parts.push(format!("steps={}", plan.len()));
        }
    }

    parts.join(" ")
}

fn user_input_request_detail(payload: Option<&Map<String, Value>>, full: bool) -> String {
    let mut parts = Vec::new();
    if let Some(phase) = payload_string(payload, "phase") {
        if !phase.is_empty() {
            parts.push(format!("phase={phase}"));
        }
    }

    let input = payload
        .and_then(|obj| obj.get("input"))
        .and_then(Value::as_object);
    let output = payload
        .and_then(|obj| obj.get("output"))
        .and_then(Value::as_object);

    if let Some(questions) = input
        .and_then(|obj| obj.get("questions"))
        .and_then(Value::as_array)
    {
        parts.push(format!("questions={}", questions.len()));
        if let Some(first_question) = questions
            .first()
            .and_then(Value::as_object)
            .and_then(|question| question.get("header").or_else(|| question.get("question")))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            parts.push(format!("first={}", summary_text(first_question, 80, full)));
        }
    }

    if let Some(answer_count) = output
        .and_then(|obj| obj.get("answers"))
        .and_then(Value::as_object)
        .map(|answers| {
            answers
                .values()
                .map(|value| match value {
                    Value::Object(answer) => answer
                        .get("answers")
                        .and_then(Value::as_array)
                        .map(Vec::len)
                        .unwrap_or(0),
                    Value::Array(answer) => answer.len(),
                    Value::String(text) => usize::from(!text.trim().is_empty()),
                    _ => 0,
                })
                .sum::<usize>()
        })
    {
        parts.push(format!("answers={answer_count}"));
    }

    parts.join(" ")
}

fn patch_apply_detail(payload: Option<&Map<String, Value>>, full: bool) -> String {
    let mut parts = Vec::new();
    let changes = payload
        .and_then(|obj| obj.get("changes"))
        .and_then(Value::as_object);
    if let Some(phase) = payload_string(payload, "phase") {
        if !phase.is_empty() {
            parts.push(format!("phase={phase}"));
        }
    }
    if let Some(status) = payload_string(payload, "status") {
        if !status.is_empty() {
            parts.push(format!("status={status}"));
        }
    }
    if changes.is_none() {
        if let Some(input) = payload
            .and_then(|obj| obj.get("input"))
            .and_then(Value::as_str)
        {
            let target = summarize_patch_targets(input, full);
            if !target.is_empty() {
                parts.push(target);
            }
        }
    }
    if let Some(changes) = changes {
        let mut paths: Vec<&str> = changes.keys().map(String::as_str).collect();
        paths.sort_unstable();
        if let Some(first) = paths.first() {
            let mut detail = format!("file={}", summary_text(first, 80, full));
            if paths.len() > 1 {
                detail.push_str(&format!(" (+{})", paths.len() - 1));
            }
            parts.push(detail);
        } else {
            parts.push("files=0".to_string());
        }
    } else if let Some(output) = payload
        .and_then(|obj| obj.get("output"))
        .map(stringify_tool_detail)
        .map(|value| value.trim().replace('\n', " "))
        .filter(|value| !value.is_empty())
    {
        parts.push(format!("output={}", summary_text(&output, 100, full)));
    }
    parts.join(" ")
}

fn agent_session_detail(payload: Option<&Map<String, Value>>, full: bool) -> String {
    let mut details = Vec::new();
    if let Some(role) = payload_string(payload, "agent_role") {
        if !role.is_empty() {
            details.push(format!("role={role}"));
        }
    }
    if let Some(nickname) = payload_string(payload, "agent_nickname") {
        if !nickname.is_empty() {
            details.push(format!("nickname={nickname}"));
        }
    }
    if let Some(cwd) = payload_string(payload, "cwd") {
        if !cwd.is_empty() {
            details.push(format!("cwd={}", summary_text(&cwd, 60, full)));
        }
    }
    details.join(" ")
}

fn stdin_write_detail(payload: Option<&Map<String, Value>>, full: bool) -> String {
    let mut parts = Vec::new();
    if let Some(phase) = payload_string(payload, "phase") {
        if !phase.is_empty() {
            parts.push(format!("phase={phase}"));
        }
    }
    if let Some(status) = payload_string(payload, "status") {
        if !status.is_empty() {
            parts.push(format!("status={status}"));
        }
    }
    if let Some(input) = payload
        .and_then(|obj| obj.get("input"))
        .and_then(Value::as_object)
    {
        if let Some(session_id) = input.get("session_id").map(json_scalar_to_string) {
            if !session_id.is_empty() {
                parts.push(format!("session_id={session_id}"));
            }
        }
        if let Some(chars) = input
            .get("chars")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|chars| !chars.is_empty())
        {
            parts.push(format!("chars={}", summary_text(chars, 80, full)));
        }
    }
    if let Some(output) = payload
        .and_then(|obj| obj.get("output"))
        .map(stringify_tool_detail)
        .map(|value| value.trim().replace('\n', " "))
        .filter(|value| !value.is_empty())
    {
        parts.push(format!("output={}", summary_text(&output, 100, full)));
    }
    parts.join(" ")
}

fn info_tokens_detail(payload: Option<&Map<String, Value>>) -> String {
    let mut parts = Vec::new();
    for (key, label) in [
        ("input_tokens", "input"),
        ("cached_input_tokens", "cached input"),
        ("output_tokens", "output"),
        ("reasoning_output_tokens", "reasoning output"),
        ("total_tokens", "total"),
    ] {
        if let Some(value) = payload.and_then(|obj| obj.get(key)).and_then(Value::as_u64) {
            parts.push(format!("{label}: {}", format_summary_number(value)));
        } else if let Some(value) = payload.and_then(|obj| obj.get(key)) {
            parts.push(format!("{label}: {}", json_scalar_to_string(value)));
        }
    }
    parts.join(", ")
}

fn runtime_context_detail(payload: Option<&Map<String, Value>>, full: bool) -> String {
    let mut parts = Vec::new();
    if let Some(cwd) = payload_string(payload, "cwd") {
        if !cwd.is_empty() {
            parts.push(format!("cwd={}", summary_text(&cwd, 80, full)));
        }
    }
    if let Some(model) = payload_string(payload, "model") {
        if !model.is_empty() {
            parts.push(format!("model={model}"));
        }
    }
    if let Some(mode) = payload_object(payload, "collaboration_mode")
        .and_then(|obj| obj.get("mode"))
        .and_then(Value::as_str)
    {
        if !mode.is_empty() {
            parts.push(format!("mode={mode}"));
        }
    }
    parts.join(" ")
}

fn context_compacted_detail(payload: Option<&Map<String, Value>>, full: bool) -> String {
    let mut parts = Vec::new();
    if let Some(history) = payload
        .and_then(|obj| obj.get("replacement_history"))
        .and_then(Value::as_array)
    {
        parts.push(format!("items={}", history.len()));
    }
    if let Some(message) = payload_string(payload, "message") {
        if !message.trim().is_empty() {
            parts.push(format!(
                "message={}",
                summary_text(message.trim(), 80, full)
            ));
        }
    }
    parts.join(" ")
}

fn summarize_patch_targets(input: &str, full: bool) -> String {
    let mut paths = Vec::new();
    for line in input.lines() {
        for prefix in ["*** Update File: ", "*** Add File: ", "*** Delete File: "] {
            if let Some(path) = line.strip_prefix(prefix) {
                let path = path.trim();
                if !path.is_empty() {
                    paths.push(path.to_string());
                }
                break;
            }
        }
    }
    if let Some(first) = paths.first() {
        let mut detail = format!("file={}", summary_text(first, 80, full));
        if paths.len() > 1 {
            detail.push_str(&format!(" (+{})", paths.len() - 1));
        }
        detail
    } else {
        String::new()
    }
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

fn is_failed_tool_result(payload: Option<&Map<String, Value>>) -> bool {
    if let Some(exit_code) = payload
        .and_then(|obj| obj.get("exit_code"))
        .and_then(Value::as_i64)
    {
        return exit_code != 0;
    }
    payload_string(payload, "status")
        .map(|status| {
            matches!(
                status.to_lowercase().as_str(),
                "failed" | "error" | "cancelled"
            )
        })
        .unwrap_or(false)
}

fn is_command_tool(tool_name: &str, payload: Option<&Map<String, Value>>) -> bool {
    matches!(
        tool_name,
        "command_execution" | "exec_command" | "write_stdin"
    ) || command_from_payload(payload).is_some()
}

fn subagent_tool_label(tool_name: &str) -> Option<&'static str> {
    match tool_name {
        "spawn_agent" => Some("subagent launch"),
        "send_input" => Some("subagent input"),
        "wait_agent" | "wait" => Some("subagent wait"),
        "close_agent" => Some("subagent close"),
        "resume_agent" => Some("subagent resume"),
        _ => None,
    }
}

fn has_subagent_state(payload: Option<&Map<String, Value>>) -> bool {
    payload
        .map(|obj| {
            obj.contains_key("receiver_thread_ids")
                || obj.contains_key("sender_thread_id")
                || obj.contains_key("agents_states")
        })
        .unwrap_or(false)
}

fn subagent_state_detail(payload: Option<&Map<String, Value>>, full: bool) -> String {
    let mut details = Vec::new();
    if let Some(phase) = payload_string(payload, "phase") {
        if !phase.is_empty() {
            details.push(format!("phase={phase}"));
        }
    }
    if let Some(status) = payload_string(payload, "status") {
        if !status.is_empty() {
            details.push(format!("status={status}"));
        }
    }
    if payload_string(payload, "tool_name").as_deref() == Some("spawn_agent") {
        if let Some(prompt) = payload_string(payload, "prompt") {
            if !prompt.trim().is_empty() {
                details.push(format!("prompt={}", summary_text(prompt.trim(), 80, full)));
            }
        }
    }
    if let Some(receivers) = payload_array(payload, "receiver_thread_ids") {
        if !receivers.is_empty() {
            details.push(format!(
                "agents={}",
                receivers
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }
    }
    if let Some(states) = payload_object(payload, "agents_states") {
        let mut state_parts = Vec::new();
        for (thread_id, raw_state) in states {
            if let Some(status) = raw_state
                .as_object()
                .and_then(|obj| obj.get("status"))
                .and_then(Value::as_str)
            {
                state_parts.push(format!("{thread_id}:{status}"));
            }
        }
        if !state_parts.is_empty() {
            details.push(format!("states={}", state_parts.join(",")));
        }
    }
    details.join(" ")
}

fn task_started_detail(payload: Option<&Map<String, Value>>, full: bool) -> String {
    let mut details = Vec::new();
    if let Some(mode) = payload_string(payload, "collaboration_mode_kind") {
        let mode = mode.trim();
        if !mode.is_empty() {
            details.push(format!("mode={mode}"));
        }
    }
    if let Some(turn_id) = payload_string(payload, "turn_id") {
        let turn_id = turn_id.trim();
        if !turn_id.is_empty() {
            details.push(format!("turn={}", summary_text(turn_id, 80, full)));
        }
    }
    if let Some(window) = payload
        .and_then(|obj| obj.get("model_context_window"))
        .map(json_scalar_to_string)
    {
        let window = window.trim();
        if !window.is_empty() && window != "null" {
            details.push(format!("window={window}"));
        }
    }
    details.join(" ")
}

fn append_summary_detail(summary: &mut String, detail: &str, has_detail: &mut bool) {
    if detail.is_empty() {
        return;
    }
    if !*has_detail {
        summary.push_str(": ");
        *has_detail = true;
    } else {
        summary.push(' ');
    }
    summary.push_str(detail);
}

fn stringify_tool_detail(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.trim().to_string(),
        Value::Object(obj) => {
            for key in ["stderr", "error", "message", "output", "stdout"] {
                if let Some(nested) = obj.get(key) {
                    let rendered = stringify_tool_detail(nested);
                    if !rendered.is_empty() {
                        return rendered;
                    }
                }
            }
            serde_json::to_string(value).unwrap_or_default()
        }
        Value::Array(_) => serde_json::to_string(value).unwrap_or_default(),
        other => json_scalar_to_string(other),
    }
}

fn tool_result_diagnostic(payload: Option<&Map<String, Value>>, full: bool) -> String {
    if !is_failed_tool_result(payload) {
        return String::new();
    }
    for key in ["stderr", "error", "output"] {
        let rendered = payload
            .and_then(|obj| obj.get(key))
            .map(stringify_tool_detail)
            .unwrap_or_default();
        if !rendered.is_empty() {
            return summary_text(&rendered.replace('\n', " "), 120, full);
        }
    }
    String::new()
}

fn summary_text(value: &str, limit: usize, full: bool) -> String {
    if full {
        return value.trim().to_string();
    }
    truncate_text(value, limit)
}

fn render_mcp_value(value: &Value, full: bool) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => summary_text(text.trim(), 120, full),
        _ => summary_text(&serde_json::to_string(value).unwrap_or_default(), 120, full),
    }
}

fn actor_type(event: &EventRecord) -> String {
    payload_string(event.payload.as_object(), "actor_type").unwrap_or_else(|| "agent".to_string())
}

fn thread_id(event: &EventRecord) -> Option<String> {
    payload_string(event.payload.as_object(), "thread_id")
}

fn normalize_parent_thread_id(parent_thread_id: Option<&str>) -> Option<&str> {
    parent_thread_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn text_links_suffix(payload: Option<&Map<String, Value>>) -> String {
    let Some(raw_links) = payload
        .and_then(|obj| obj.get("text_links"))
        .and_then(Value::as_object)
    else {
        return String::new();
    };
    let mut parts = Vec::new();
    if let Some(request_id) = raw_links.get("request_id").and_then(Value::as_str) {
        if !request_id.is_empty() {
            parts.push(format!("request_id={request_id}"));
        }
    }
    if let Some(status_code) = raw_links.get("status_code") {
        parts.push(format!("status={}", json_scalar_to_string(status_code)));
    }
    if let Some(host) = raw_links.get("host").and_then(Value::as_str) {
        if !host.is_empty() {
            parts.push(format!("host={host}"));
        }
    }
    if let Some(cf_ray) = raw_links.get("cf_ray").and_then(Value::as_str) {
        if !cf_ray.is_empty() {
            parts.push(format!("cf_ray={cf_ray}"));
        }
    }
    if raw_links
        .get("is_fallback")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        if let Some(fallback_to) = raw_links.get("fallback_to").and_then(Value::as_str) {
            if !fallback_to.is_empty() {
                parts.push(format!("fallback={fallback_to}"));
            }
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" [{}]", parts.join(" "))
    }
}

fn payload_string(payload: Option<&Map<String, Value>>, key: &str) -> Option<String> {
    payload
        .and_then(|obj| obj.get(key))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn payload_object<'a>(
    payload: Option<&'a Map<String, Value>>,
    key: &str,
) -> Option<&'a Map<String, Value>> {
    payload
        .and_then(|obj| obj.get(key))
        .and_then(Value::as_object)
}

fn payload_array<'a>(payload: Option<&'a Map<String, Value>>, key: &str) -> Option<&'a Vec<Value>> {
    payload
        .and_then(|obj| obj.get(key))
        .and_then(Value::as_array)
}

fn payload_has_non_empty_array(payload: Option<&Map<String, Value>>, key: &str) -> bool {
    payload_array(payload, key)
        .map(|items| !items.is_empty())
        .unwrap_or(false)
}

fn json_scalar_to_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::String(text) => text.clone(),
        _ => value.to_string(),
    }
}
