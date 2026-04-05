use std::collections::{HashMap, VecDeque};
use std::path::Path;

use serde_json::{Map, Value};

use crate::events::readers::EventLogFileReader;
use crate::events::record::EventRecord;

const DEFAULT_TIMELINE_LIMIT: usize = 12;
const DEFAULT_AGENT_LINE_LIMIT: usize = 4;
const AGENT_PALETTE: &[&str] = &[
    "#e76f51",
    "#f4a261",
    "#e9c46a",
    "#90be6d",
    "#43aa8b",
    "#4d908e",
    "#577590",
    "#277da1",
    "#9b5de5",
    "#f15bb5",
    "#ff006e",
    "#fb5607",
    "#3a86ff",
    "#06d6a0",
    "#8ecae6",
    "#bc6c25",
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
            agent_palette: AGENT_PALETTE.iter().map(|value| value.to_string()).collect(),
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

        let preserved: Vec<TimelineEntry> = if self.snapshot.task_id.as_deref() == Some(task_id.as_str())
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
        self.push_timeline("start", format!("start: {task_id} cwd={}", cwd.unwrap_or_else(|| ".".to_string())), "");
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
        self.push_timeline(event.event_type.clone(), summarize_event(event), event.ts.clone());
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

    fn ensure_agent(&mut self, thread_id: &str, parent_thread_id: Option<&str>) -> &mut AgentSnapshot {
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
                recent_lines: VecDeque::new(),
            });
        if agent.parent_thread_id.is_none() {
            agent.parent_thread_id = parent_thread_id.map(str::to_string);
        }
        agent
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

    fn apply_event_state(&mut self, event: &EventRecord) {
        let payload = event.payload.as_object();
        match event.event_type.as_str() {
            "thread.started" => {
                let thread_id = payload
                    .and_then(|p| p.get("thread_id"))
                    .and_then(Value::as_str)
                    .unwrap_or("root")
                    .to_string();
                self.snapshot.root_thread_id = Some(thread_id.clone());
                self.ensure_agent(&thread_id, None).status = "running".to_string();
            }
            "agent.message" => {
                let thread_id = thread_id(event).or_else(|| self.snapshot.root_thread_id.clone());
                if let Some(thread_id) = thread_id {
                    let text = payload_string(payload, "text").unwrap_or_default();
                    let parent_thread_id = payload_string(payload, "parent_thread_id");
                    self.append_agent_line(&thread_id, &text, parent_thread_id.as_deref());
                }
            }
            "agent.turn.completed" => {
                if let Some(root_thread_id) = self.snapshot.root_thread_id.clone() {
                    self.snapshot.status = "completed".to_string();
                    self.ensure_agent(&root_thread_id, None).status = "completed".to_string();
                }
            }
            "agent.turn.failed" => {
                if let Some(root_thread_id) = self.snapshot.root_thread_id.clone() {
                    self.snapshot.status = "failed".to_string();
                    self.ensure_agent(&root_thread_id, None).status = "failed".to_string();
                }
            }
            "tool.result" if payload_has_non_empty_array(payload, "receiver_thread_ids") => {
                let sender = thread_id(event)
                    .or_else(|| self.snapshot.root_thread_id.clone())
                    .unwrap_or_else(|| "root".to_string());
                self.ensure_agent(&sender, None);
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
                        let Some(raw_state) = states.get(&state_thread_id).and_then(Value::as_object) else {
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
            "agent.session" => {
                let thread_id = thread_id(event).unwrap_or_else(|| "unknown".to_string());
                self.assign_subagent_color(&thread_id);
                let parent_thread_id = payload_string(payload, "parent_thread_id");
                let nickname = payload_string(payload, "agent_nickname");
                let role = payload_string(payload, "agent_role");
                let cwd = payload_string(payload, "cwd");
                let agent = self.ensure_agent(&thread_id, parent_thread_id.as_deref());
                agent.nickname = nickname;
                agent.role = role;
                agent.cwd = cwd;
                agent.status = "session_loaded".to_string();
            }
            "tool.call" if actor_type(event) == "subagent" => {
                let thread_id = thread_id(event).unwrap_or_else(|| "unknown".to_string());
                let tool_name = payload_string(payload, "tool_name").unwrap_or_else(|| "unknown".to_string());
                self.assign_subagent_color(&thread_id);
                self.append_agent_line(&thread_id, &format!("tool: {tool_name}"), None);
                self.ensure_agent(&thread_id, None).status = "working".to_string();
            }
            "tool.result" if actor_type(event) == "subagent" => {
                let thread_id = thread_id(event).unwrap_or_else(|| "unknown".to_string());
                let tool_name = payload_string(payload, "tool_name").unwrap_or_else(|| "unknown".to_string());
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
                self.append_agent_line(&thread_id, &line, None);
            }
            "agent.meta" if actor_type(event) == "subagent" => {
                let thread_id = thread_id(event).unwrap_or_else(|| "unknown".to_string());
                self.assign_subagent_color(&thread_id);
                let parent_thread_id = payload_string(payload, "parent_thread_id");
                let meta_type = payload_string(payload, "meta_type").unwrap_or_else(|| "meta".to_string());
                let agent = self.ensure_agent(&thread_id, parent_thread_id.as_deref());
                match meta_type.as_str() {
                    "task_started" => {
                        agent.status = "running".to_string();
                    }
                    "task_complete" => {
                        agent.status = "completed".to_string();
                        if let Some(last_message) = payload_string(payload, "last_agent_message") {
                            if !last_message.trim().is_empty() {
                                self.append_agent_line(
                                    &thread_id,
                                    &last_message,
                                    parent_thread_id.as_deref(),
                                );
                            }
                        }
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
            _ => {}
        }
    }
}

pub fn truncate_text(value: &str, limit: usize) -> String {
    let normalized = value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
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
    let payload = event.payload.as_object();
    match event.event_type.as_str() {
        "agent.message" => {
            let label = if actor_type(event) == "subagent" {
                "subagent"
            } else {
                "assistant"
            };
            let thread_label = if actor_type(event) == "subagent" {
                format!("[{}]", thread_id(event).unwrap_or_else(|| "?".to_string()))
            } else {
                String::new()
            };
            format!(
                "{label}{thread_label}: {}{}",
                truncate_text(&payload_string(payload, "text").unwrap_or_default(), 100),
                text_links_suffix(payload)
            )
        }
        "agent.session" => {
            format!(
                "subagent session loaded: {}",
                thread_id(event).unwrap_or_else(|| "?".to_string())
            )
        }
        "agent.meta" => {
            let thread_id = thread_id(event).unwrap_or_else(|| "?".to_string());
            let meta_type = payload_string(payload, "meta_type").unwrap_or_else(|| "meta".to_string());
            match meta_type.as_str() {
                "message" => {
                    let role = payload_string(payload, "role").unwrap_or_else(|| "unknown".to_string());
                    let text = truncate_text(&payload_string(payload, "text").unwrap_or_default(), 100);
                    if text.is_empty() {
                        format!("subagent[{thread_id}] {role}")
                    } else {
                        format!("subagent[{thread_id}] {role}: {text}")
                    }
                }
                "user_message" => {
                    let text = truncate_text(&payload_string(payload, "text").unwrap_or_default(), 100);
                    if text.is_empty() {
                        format!("subagent[{thread_id}] user")
                    } else {
                        format!("subagent[{thread_id}] user: {text}")
                    }
                }
                "task_started" => {
                    let mode = payload_string(payload, "collaboration_mode_kind").unwrap_or_default();
                    if mode.is_empty() {
                        format!("subagent[{thread_id}] task started")
                    } else {
                        format!("subagent[{thread_id}] task started mode={mode}")
                    }
                }
                "task_complete" => {
                    let text = truncate_text(
                        &payload_string(payload, "last_agent_message").unwrap_or_default(),
                        80,
                    );
                    if text.is_empty() {
                        format!("subagent[{thread_id}] task complete")
                    } else {
                        format!("subagent[{thread_id}] task complete: {text}")
                    }
                }
                "token_count" => {
                    let total_suffix = payload
                        .and_then(|obj| obj.get("total_tokens"))
                        .map(|value| format!(" total={}", json_scalar_to_string(value)))
                        .unwrap_or_default();
                    let primary_suffix = payload
                        .and_then(|obj| obj.get("rate_limits"))
                        .and_then(Value::as_object)
                        .and_then(|limits| limits.get("primary"))
                        .and_then(Value::as_object)
                        .and_then(|primary| primary.get("used_percent"))
                        .map(|value| format!(" primary={}%", json_scalar_to_string(value)))
                        .unwrap_or_default();
                    format!("subagent[{thread_id}] tokens{total_suffix}{primary_suffix}")
                }
                "turn_context" => {
                    let cwd = payload_string(payload, "cwd").unwrap_or_default();
                    if cwd.is_empty() {
                        format!("subagent[{thread_id}] turn context")
                    } else {
                        format!(
                            "subagent[{thread_id}] turn context cwd={}",
                            truncate_text(&cwd, 60)
                        )
                    }
                }
                _ => format!("subagent[{thread_id}] meta {meta_type}"),
            }
        }
        "tool.call" => {
            if actor_type(event) == "subagent" {
                format!(
                    "subagent[{}] tool {}",
                    thread_id(event).unwrap_or_else(|| "?".to_string()),
                    payload_string(payload, "tool_name").unwrap_or_else(|| "?".to_string())
                )
            } else {
                format_tool_event("tool", payload)
            }
        }
        "tool.result" => {
            let has_subagent_state = payload
                .map(|obj| {
                    obj.contains_key("receiver_thread_ids")
                        || obj.contains_key("sender_thread_id")
                        || obj.contains_key("agents_states")
                })
                .unwrap_or(false);
            if has_subagent_state {
                let tool_name = payload_string(payload, "tool_name").unwrap_or_else(|| "subagent".to_string());
                let mut details = vec![tool_name];
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
                format!("subagent: {}", details.join(" "))
            } else if actor_type(event) == "subagent" {
                let output = payload
                    .and_then(|obj| obj.get("output"))
                    .map(stringify_tool_detail)
                    .unwrap_or_default()
                    .trim()
                    .replace('\n', " ");
                if output.is_empty() {
                    format!(
                        "subagent[{}] result {}",
                        thread_id(event).unwrap_or_else(|| "?".to_string()),
                        payload_string(payload, "tool_name").unwrap_or_else(|| "?".to_string())
                    )
                } else {
                    format!(
                        "subagent[{}] result {} -> {}",
                        thread_id(event).unwrap_or_else(|| "?".to_string()),
                        payload_string(payload, "tool_name").unwrap_or_else(|| "?".to_string()),
                        truncate_text(&output, 60)
                    )
                }
            } else {
                format_tool_event("tool result", payload)
            }
        }
        "file.change" => {
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
                format!(" {}", truncate_text(path, 80))
            };
            let extra = if changes.len() > 1 {
                format!(" (+{})", changes.len() - 1)
            } else {
                String::new()
            };
            format!("files: {kind}{suffix}{extra}")
        }
        "todo.update" => {
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
                        if item.get("completed").and_then(Value::as_bool).unwrap_or(false) {
                            return None;
                        }
                        item.get("text").and_then(Value::as_str).map(|text| truncate_text(text, 80))
                    })
                })
                .unwrap_or_default();
            let mut summary = format!("todo {phase}: {completed}/{total}");
            if !next_item.is_empty() {
                summary.push_str(&format!(" next={next_item}"));
            }
            summary
        }
        "stderr.line" => {
            format!(
                "stderr: {}",
                truncate_text(&payload_string(payload, "text").unwrap_or_default(), 100)
            )
        }
        "error" => {
            format!(
                "error: {}{}",
                truncate_text(&payload_string(payload, "message").unwrap_or_default(), 100),
                text_links_suffix(payload)
            )
        }
        "agent.turn.failed" => {
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
                format!("agent turn failed{}", text_links_suffix(payload))
            } else {
                format!(
                    "agent turn failed: {}{}",
                    truncate_text(&message, 100),
                    text_links_suffix(payload)
                )
            }
        }
        _ => event.event_type.clone(),
    }
}

pub fn load_event_records(path: &Path) -> std::io::Result<Vec<EventRecord>> {
    EventLogFileReader::load_records(path)
}

fn format_tool_event(prefix: &str, payload: Option<&Map<String, Value>>) -> String {
    let tool_name = payload_string(payload, "tool_name").unwrap_or_else(|| "?".to_string());
    let mut label = format!("{prefix}: {tool_name}");
    let detail = if tool_name == "web_search" {
        web_search_detail(payload)
    } else if let Some(command) = command_from_payload(payload) {
        truncate_command_text(&command, 120)
    } else {
        String::new()
    };
    if !detail.is_empty() {
        label.push(' ');
        label.push_str(&detail);
    }
    if prefix == "tool result" {
        if let Some(exit_code) = payload.and_then(|obj| obj.get("exit_code")) {
            label.push_str(&format!(" exit={}", json_scalar_to_string(exit_code)));
        }
        let diagnostic = tool_result_diagnostic(payload);
        if !diagnostic.is_empty() {
            label.push_str(&format!(" -> {diagnostic}"));
        }
    }
    label
}

fn command_from_payload(payload: Option<&Map<String, Value>>) -> Option<String> {
    let raw_input = payload.and_then(|obj| obj.get("input")).and_then(Value::as_object)?;
    for key in ["command", "cmd"] {
        let value = raw_input.get(key).and_then(Value::as_str)?.trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
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

fn web_search_detail(payload: Option<&Map<String, Value>>) -> String {
    let input = payload.and_then(|obj| obj.get("input")).and_then(Value::as_object);
    let output = payload.and_then(|obj| obj.get("output")).and_then(Value::as_object);
    let container = input.or(output);
    let Some(container) = container else {
        return String::new();
    };
    let mut parts = Vec::new();
    if let Some(query) = container.get("query").and_then(Value::as_str) {
        if !query.trim().is_empty() {
            parts.push(format!("query={}", truncate_text(query.trim(), 120)));
        } else if input == Some(container) {
            parts.push("query=<pending>".to_string());
        }
    } else if input == Some(container) {
        parts.push("query=<pending>".to_string());
    }
    if let Some(action) = container.get("action").and_then(Value::as_object) {
        if let Some(action_type) = action.get("type").and_then(Value::as_str) {
            if !action_type.trim().is_empty() {
                parts.push(format!("action={}", truncate_text(action_type.trim(), 40)));
            }
        }
        if let Some(url) = action.get("url").and_then(Value::as_str) {
            if !url.trim().is_empty() {
                parts.push(format!("url={}", truncate_text(url.trim(), 120)));
            }
        }
    }
    parts.join(" ")
}

fn is_failed_tool_result(payload: Option<&Map<String, Value>>) -> bool {
    if let Some(exit_code) = payload.and_then(|obj| obj.get("exit_code")).and_then(Value::as_i64) {
        return exit_code != 0;
    }
    payload_string(payload, "status")
        .map(|status| matches!(status.to_lowercase().as_str(), "failed" | "error" | "cancelled"))
        .unwrap_or(false)
}

fn stringify_tool_detail(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.trim().to_string(),
        Value::Object(obj) => {
            for key in ["stderr", "error", "message", "output"] {
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

fn tool_result_diagnostic(payload: Option<&Map<String, Value>>) -> String {
    if !is_failed_tool_result(payload) {
        return String::new();
    }
    for key in ["stderr", "error", "output"] {
        let rendered = payload
            .and_then(|obj| obj.get(key))
            .map(stringify_tool_detail)
            .unwrap_or_default();
        if !rendered.is_empty() {
            return truncate_text(&rendered.replace('\n', " "), 120);
        }
    }
    String::new()
}

fn actor_type(event: &EventRecord) -> String {
    payload_string(event.payload.as_object(), "actor_type").unwrap_or_else(|| "agent".to_string())
}

fn thread_id(event: &EventRecord) -> Option<String> {
    payload_string(event.payload.as_object(), "thread_id")
}

fn text_links_suffix(payload: Option<&Map<String, Value>>) -> String {
    let Some(raw_links) = payload.and_then(|obj| obj.get("text_links")).and_then(Value::as_object) else {
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
    if raw_links.get("is_fallback").and_then(Value::as_bool).unwrap_or(false) {
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
    payload.and_then(|obj| obj.get(key)).and_then(Value::as_str).map(str::to_string)
}

fn payload_object<'a>(payload: Option<&'a Map<String, Value>>, key: &str) -> Option<&'a Map<String, Value>> {
    payload.and_then(|obj| obj.get(key)).and_then(Value::as_object)
}

fn payload_array<'a>(payload: Option<&'a Map<String, Value>>, key: &str) -> Option<&'a Vec<Value>> {
    payload.and_then(|obj| obj.get(key)).and_then(Value::as_array)
}

fn payload_has_non_empty_array(payload: Option<&Map<String, Value>>, key: &str) -> bool {
    payload_array(payload, key).map(|items| !items.is_empty()).unwrap_or(false)
}

fn json_scalar_to_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::String(text) => text.clone(),
        _ => value.to_string(),
    }
}
