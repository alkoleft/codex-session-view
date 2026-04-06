use std::io::{self, IsTerminal, Stdout, Write};

use crate::events::projector::{
    summarize_event, truncate_text, EventProjector, EventSummaryCategory,
};
use crate::events::types::{
    AGENT_ABORTED, AGENT_COMPLETED, AGENT_FAILED, AGENT_SESSION_FOREIGN, COLLAB_CLOSE_AGENT,
    COLLAB_RESUME_AGENT, COLLAB_SEND_INPUT, COLLAB_SPAWN_AGENT, COLLAB_WAIT, CONTEXT_COMPACTED,
    CONTEXT_COMPACTED_DUPLICATE, INFO_TOKENS, MCP_CALL, MCP_RESULT, MESSAGE_COMMENTARY,
    MESSAGE_USER, PATCH_APPLY, PATCH_APPLY_DUPLICATE, PLAN_UPDATE, RAW_UNPARSED, RUNTIME_CONTEXT,
    SHELL_CALL, SHELL_RESULT, STDERR_LINE, STDIN_WRITE, TASK_COMPLETED, TASK_STARTED, TOOL_CALL,
    TOOL_RESULT, WEB_OPEN, WEB_SEARCH,
};
use crate::models::{EventRecord, TaskBlock, WorkerConfig};

const ANSI_RESET: &str = "\x1b[0m";
const ANSI_BOLD_RED: &str = "\x1b[1;31m";
const ANSI_RED: &str = "\x1b[31m";
const ANSI_BOLD_GREEN: &str = "\x1b[1;32m";
const ANSI_GREEN: &str = "\x1b[32m";
const ANSI_BOLD_YELLOW: &str = "\x1b[1;33m";
const ANSI_YELLOW: &str = "\x1b[33m";
const ANSI_BOLD_BLUE: &str = "\x1b[1;34m";
const ANSI_BOLD_MAGENTA: &str = "\x1b[1;35m";
const ANSI_BOLD_CYAN: &str = "\x1b[1;36m";
const ANSI_BOLD_WHITE: &str = "\x1b[1;37m";
const ANSI_CYAN: &str = "\x1b[36m";
const ANSI_WHITE: &str = "\x1b[37m";
const ANSI_DIM: &str = "\x1b[2m";

#[derive(Debug, Clone, Copy)]
struct RenderPalette {
    badge: &'static str,
    label: &'static str,
    body: &'static str,
    guide: &'static str,
}

#[derive(Debug)]
pub struct WorkerConsole<W: Write = Stdout> {
    pub config: WorkerConfig,
    pub projector: EventProjector,
    pub started: bool,
    pub status_enabled: bool,
    pub status_line: String,
    ansi_enabled: bool,
    writer: W,
}

impl WorkerConsole<Stdout> {
    pub fn new(config: WorkerConfig) -> Self {
        let stdout = io::stdout();
        let interactive = stdout.is_terminal();
        Self::with_writer_options(config, stdout, interactive, interactive)
    }
}

impl<W: Write> WorkerConsole<W> {
    pub fn with_writer(config: WorkerConfig, writer: W, status_enabled: bool) -> Self {
        Self::with_writer_options(config, writer, status_enabled, false)
    }

    pub fn with_writer_options(
        config: WorkerConfig,
        writer: W,
        status_enabled: bool,
        ansi_enabled: bool,
    ) -> Self {
        Self {
            config,
            projector: EventProjector::new(12, 4),
            started: false,
            status_enabled,
            status_line: String::new(),
            ansi_enabled,
            writer,
        }
    }

    pub fn into_inner(self) -> W {
        self.writer
    }

    pub fn start(&mut self) -> io::Result<()> {
        self.started = true;
        self.redraw_status()
    }

    pub fn finish(&mut self) -> io::Result<()> {
        if self.status_enabled && !self.status_line.is_empty() {
            self.clear_status_line()?;
            writeln!(self.writer, "{}", self.render_status_line())?;
            self.writer.flush()?;
        }
        self.started = false;
        Ok(())
    }

    pub fn on_claim(&mut self, task: &TaskBlock, run_id: &str) -> io::Result<()> {
        let task_id = task.task_id().unwrap_or("task");
        self.projector
            .set_claimed(task_id.to_string(), task.title.clone(), run_id.to_string());
        self.update_status_line();
        let label = truncate_text(task_title(task, task_id), 96);
        self.print_stream_line(&self.format_timeline_line(
            &format!("Задача взята в работу: {label} (id={task_id}, run={run_id})"),
            "claim",
        ))
    }

    pub fn on_start(&mut self, task: &TaskBlock, run_id: &str) -> io::Result<()> {
        let task_id = task.task_id().unwrap_or("task");
        self.projector.reset_run(
            task_id.to_string(),
            task.title.clone(),
            run_id.to_string(),
            task.cwd().map(str::to_string),
        );
        self.update_status_line();
        self.print_stream_line(&self.format_timeline_line(
            &format!("Запуск worker в {}", task.cwd().unwrap_or(".")),
            "start",
        ))
    }

    pub fn on_result(
        &mut self,
        task_id: &str,
        task_title: &str,
        run_id: &str,
        status: &str,
        reason: Option<&str>,
    ) -> io::Result<()> {
        self.projector.set_status(status.to_string(), None);
        self.update_status_line();
        let suffix = reason
            .filter(|value| !value.is_empty())
            .map(|value| format!(" reason={value}"))
            .unwrap_or_default();
        let label = truncate_text(
            if task_title.is_empty() {
                task_id
            } else {
                task_title
            },
            96,
        );
        self.print_stream_line(&self.format_timeline_line(
            &format!(
                "Задача завершена: {label} (id={task_id}, run={run_id}, status={status}{suffix})"
            ),
            status,
        ))
    }

    pub fn on_emit(&mut self, kind: &str, message: &str) -> io::Result<()> {
        match kind {
            "idle" => {
                self.projector.set_status("idle".to_string(), None);
                self.update_status_line();
                self.print_stream_line(&self.format_timeline_line(message, "idle"))
            }
            "recovery" => self.print_stream_line(&self.format_timeline_line(message, "warning")),
            "stderr" => self.print_stream_line(&self.format_timeline_line(message, "failed")),
            "subagent" => self.print_stream_line(&self.format_timeline_line_with_category(
                &format!("subagent: {}", truncate_text(message, 180)),
                "info",
                EventSummaryCategory::Subagent,
            )),
            _ => Ok(()),
        }
    }

    pub fn on_event(&mut self, event: &EventRecord) -> io::Result<()> {
        let category = self.projector.display_category(event);
        self.projector.apply_event(event);
        self.update_status_line();
        self.print_stream_line(&self.format_event_line_with_category(event, category))
    }

    pub fn format_event_line(&self, event: &EventRecord) -> String {
        self.format_event_line_with_category(event, self.projector.display_category(event))
    }

    fn format_event_line_with_category(
        &self,
        event: &EventRecord,
        category: EventSummaryCategory,
    ) -> String {
        self.format_timeline_line_with_category(
            &summarize_event(event),
            self.event_timeline_kind(event),
            category,
        )
    }

    pub fn format_timeline_line(&self, message: &str, kind: &str) -> String {
        self.format_timeline_line_with_category(message, kind, EventSummaryCategory::Default)
    }

    fn format_timeline_line_with_category(
        &self,
        message: &str,
        kind: &str,
        category: EventSummaryCategory,
    ) -> String {
        let lines: Vec<&str> = if message.is_empty() {
            vec![""]
        } else {
            message.lines().collect()
        };
        let palette = self.render_palette(kind, category);
        let mut out = String::new();
        if let Some(first) = lines.first() {
            let badge_style = if self.split_subagent_prefix(first).is_some()
                && matches!(category, EventSummaryCategory::Subagent)
            {
                self.kind_badge_style(kind)
            } else {
                palette.badge
            };
            out.push_str(&self.paint("○", badge_style));
            out.push(' ');
            out.push_str(&self.paint_leading_line(first, palette));
        }
        for line in lines.iter().skip(1) {
            out.push('\n');
            out.push_str(&self.paint("│", palette.guide));
            out.push_str("   ");
            out.push_str(&self.paint(line, palette.body));
        }
        out
    }

    pub fn update_status_line(&mut self) {
        let snapshot = &self.projector.snapshot;
        let mut parts = vec![self.status_chip(&snapshot.status)];
        if let Some(task) = snapshot
            .task_title
            .as_deref()
            .or(snapshot.task_id.as_deref())
            .filter(|value| !value.is_empty())
        {
            parts.push(truncate_text(task, 96));
        }
        if let Some(run_chip) = self.format_run_chip(snapshot.run_id.as_deref()) {
            parts.push(run_chip);
        }
        self.status_line = truncate_text(&parts.join(" · "), 180);
    }

    fn print_stream_line(&mut self, renderable: &str) -> io::Result<()> {
        if self.status_enabled {
            self.clear_status_line()?;
        }
        writeln!(self.writer, "{renderable}")?;
        self.writer.flush()?;
        self.redraw_status()
    }

    fn clear_status_line(&mut self) -> io::Result<()> {
        if !self.status_enabled {
            return Ok(());
        }
        write!(self.writer, "\r\x1b[2K")?;
        self.writer.flush()
    }

    fn redraw_status(&mut self) -> io::Result<()> {
        if !self.status_enabled || self.status_line.is_empty() {
            return Ok(());
        }
        write!(self.writer, "\r\x1b[2K{}", self.render_status_line())?;
        self.writer.flush()
    }

    fn event_timeline_kind(&self, event: &EventRecord) -> &'static str {
        match event.event_type.as_str() {
            AGENT_FAILED | AGENT_ABORTED | RAW_UNPARSED | STDERR_LINE | "error" => "failed",
            AGENT_COMPLETED => "completed",
            INFO_TOKENS
            | TASK_STARTED
            | TASK_COMPLETED
            | MESSAGE_USER
            | MESSAGE_COMMENTARY
            | RUNTIME_CONTEXT
            | CONTEXT_COMPACTED
            | CONTEXT_COMPACTED_DUPLICATE
            | AGENT_SESSION_FOREIGN
            | PATCH_APPLY_DUPLICATE => "info",
            PATCH_APPLY => {
                if let Some(success) = event
                    .payload
                    .get("success")
                    .and_then(|value| value.as_bool())
                {
                    if success {
                        "file"
                    } else {
                        "failed"
                    }
                } else {
                    match event.payload.get("status").and_then(ValueExt::as_str) {
                        Some("failed" | "error" | "cancelled") => "failed",
                        _ if matches!(
                            event.payload.get("phase").and_then(ValueExt::as_str),
                            Some("started")
                        ) =>
                        {
                            "info"
                        }
                        _ => "file",
                    }
                }
            }
            TOOL_RESULT | SHELL_RESULT | MCP_RESULT | STDIN_WRITE | COLLAB_SPAWN_AGENT
            | COLLAB_SEND_INPUT | COLLAB_WAIT | COLLAB_CLOSE_AGENT | COLLAB_RESUME_AGENT
            | WEB_SEARCH | WEB_OPEN | PLAN_UPDATE => {
                if let Some(exit_code) = event.payload.get("exit_code").and_then(ValueExt::as_i64) {
                    if exit_code == 0 {
                        "completed"
                    } else {
                        "failed"
                    }
                } else {
                    match event.payload.get("status").and_then(ValueExt::as_str) {
                        Some("completed" | "ok" | "success") => "completed",
                        Some("failed" | "error" | "cancelled") => "failed",
                        _ if matches!(
                            event.payload.get("phase").and_then(ValueExt::as_str),
                            Some("started")
                        ) =>
                        {
                            "info"
                        }
                        _ => "default",
                    }
                }
            }
            "file.change" => "file",
            TOOL_CALL | SHELL_CALL | MCP_CALL => "info",
            _ => "default",
        }
    }

    fn status_chip(&self, status: &str) -> String {
        match status {
            "idle" => "○ idle".to_string(),
            "claimed" => "◔ claim".to_string(),
            "running" => "▶ run".to_string(),
            "completed" => "● done".to_string(),
            "failed" => "✖ fail".to_string(),
            _ => "• wait".to_string(),
        }
    }

    fn compact_run_id(&self, run_id: Option<&str>) -> Option<String> {
        let normalized = run_id?.trim();
        if normalized.is_empty() || normalized == "-" {
            return None;
        }
        if normalized.len() <= 12 {
            Some(normalized.to_string())
        } else {
            Some(normalized.chars().take(8).collect())
        }
    }

    fn format_run_chip(&self, run_id: Option<&str>) -> Option<String> {
        self.compact_run_id(run_id)
            .map(|compact| format!("#{compact}"))
    }

    fn render_status_line(&self) -> String {
        let Some((chip, tail)) = self.status_line.split_once(" · ") else {
            return self.paint(
                &self.status_line,
                self.status_chip_style(&self.projector.snapshot.status),
            );
        };
        format!(
            "{} · {}",
            self.paint(
                chip,
                self.status_chip_style(&self.projector.snapshot.status)
            ),
            tail
        )
    }

    fn render_palette(&self, kind: &str, category: EventSummaryCategory) -> RenderPalette {
        let (label, body) = match category {
            EventSummaryCategory::Assistant => (ANSI_BOLD_GREEN, ""),
            EventSummaryCategory::Command => (ANSI_BOLD_BLUE, ""),
            EventSummaryCategory::Search => (ANSI_BOLD_CYAN, ""),
            EventSummaryCategory::Subagent => (ANSI_BOLD_MAGENTA, ""),
            EventSummaryCategory::File => (ANSI_BOLD_YELLOW, ""),
            EventSummaryCategory::Todo => (ANSI_BOLD_CYAN, ""),
            EventSummaryCategory::Error => (ANSI_BOLD_RED, ""),
            EventSummaryCategory::Default => {
                (self.kind_label_style(kind), self.kind_body_style(kind))
            }
        };
        RenderPalette {
            badge: match category {
                EventSummaryCategory::Default => self.kind_badge_style(kind),
                EventSummaryCategory::Error => ANSI_BOLD_RED,
                EventSummaryCategory::Assistant => ANSI_BOLD_GREEN,
                EventSummaryCategory::Command => ANSI_BOLD_BLUE,
                EventSummaryCategory::Search => ANSI_BOLD_CYAN,
                EventSummaryCategory::Subagent => ANSI_BOLD_MAGENTA,
                EventSummaryCategory::File => ANSI_BOLD_YELLOW,
                EventSummaryCategory::Todo => ANSI_BOLD_CYAN,
            },
            label,
            body,
            guide: ANSI_DIM,
        }
    }

    fn paint_leading_line(&self, line: &str, palette: RenderPalette) -> String {
        if let Some((prefix, remainder)) = self.split_subagent_prefix(line) {
            let (label_style, body_style) = if palette.label == ANSI_BOLD_MAGENTA {
                ("", "")
            } else if remainder.starts_with(':') {
                ("", "")
            } else {
                (palette.label, palette.body)
            };
            let prefix_style = self.subagent_prefix_style(prefix);
            return format!(
                "{}{}",
                self.paint(prefix, &prefix_style),
                self.paint_remainder(remainder, label_style, body_style)
            );
        }
        self.paint_remainder(line, palette.label, palette.body)
    }

    fn paint_remainder(
        &self,
        line: &str,
        label_style: &'static str,
        body_style: &'static str,
    ) -> String {
        if let Some(split) = line.find(':') {
            let (label, body) = line.split_at(split + 1);
            format!(
                "{}{}",
                self.paint(label, label_style),
                self.paint(body, body_style)
            )
        } else {
            self.paint(line, label_style)
        }
    }

    fn split_subagent_prefix<'a>(&self, line: &'a str) -> Option<(&'a str, &'a str)> {
        if !line.starts_with("subagent[") {
            return None;
        }
        let end = line.find(']')?;
        Some(line.split_at(end + 1))
    }

    fn subagent_prefix_style(&self, prefix: &str) -> String {
        let Some(thread_id) = prefix
            .strip_prefix("subagent[")
            .and_then(|value| value.strip_suffix(']'))
        else {
            return ANSI_BOLD_MAGENTA.to_string();
        };
        self.projector
            .snapshot
            .agents
            .get(thread_id)
            .and_then(|agent| agent.color.as_deref())
            .and_then(hex_to_truecolor_bold)
            .unwrap_or_else(|| ANSI_BOLD_MAGENTA.to_string())
    }

    fn paint(&self, text: &str, style: &str) -> String {
        if !self.ansi_enabled || style.is_empty() || text.is_empty() {
            return text.to_string();
        }
        format!("{style}{text}{ANSI_RESET}")
    }

    fn kind_badge_style(&self, kind: &str) -> &'static str {
        match kind {
            "claim" | "start" | "idle" | "info" | "default" => ANSI_BOLD_CYAN,
            "completed" => ANSI_BOLD_GREEN,
            "failed" => ANSI_BOLD_RED,
            "warning" => ANSI_BOLD_YELLOW,
            "file" => ANSI_BOLD_YELLOW,
            _ => ANSI_BOLD_WHITE,
        }
    }

    fn kind_label_style(&self, kind: &str) -> &'static str {
        match kind {
            "claim" | "start" | "idle" | "info" | "default" => ANSI_BOLD_CYAN,
            "completed" => ANSI_BOLD_GREEN,
            "failed" => ANSI_BOLD_RED,
            "warning" => ANSI_BOLD_YELLOW,
            "file" => ANSI_BOLD_YELLOW,
            _ => ANSI_BOLD_WHITE,
        }
    }

    fn kind_body_style(&self, kind: &str) -> &'static str {
        match kind {
            "claim" | "start" | "idle" | "info" | "default" => ANSI_CYAN,
            "completed" => ANSI_GREEN,
            "failed" => ANSI_RED,
            "warning" => ANSI_YELLOW,
            "file" => ANSI_YELLOW,
            _ => ANSI_WHITE,
        }
    }

    fn status_chip_style(&self, status: &str) -> &'static str {
        match status {
            "idle" => ANSI_BOLD_CYAN,
            "claimed" => ANSI_BOLD_CYAN,
            "running" => ANSI_BOLD_CYAN,
            "completed" => ANSI_BOLD_GREEN,
            "failed" => ANSI_BOLD_RED,
            _ => ANSI_BOLD_WHITE,
        }
    }
}

fn task_title<'a>(task: &'a TaskBlock, fallback_id: &'a str) -> &'a str {
    if task.title.is_empty() {
        fallback_id
    } else {
        &task.title
    }
}

trait ValueExt {
    fn as_i64(&self) -> Option<i64>;
    fn as_str(&self) -> Option<&str>;
}

impl ValueExt for serde_json::Value {
    fn as_i64(&self) -> Option<i64> {
        serde_json::Value::as_i64(self)
    }

    fn as_str(&self) -> Option<&str> {
        serde_json::Value::as_str(self)
    }
}

fn hex_to_truecolor_bold(hex: &str) -> Option<String> {
    let hex = hex.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let red = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let green = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let blue = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(format!("\x1b[1;38;2;{red};{green};{blue}m"))
}
