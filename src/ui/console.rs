use std::io::{self, IsTerminal, Stdout, Write};

use crate::events::projector::{summarize_event, truncate_text, EventProjector};
use crate::models::{EventRecord, TaskBlock, WorkerConfig};

#[derive(Debug)]
pub struct WorkerConsole<W: Write = Stdout> {
    pub config: WorkerConfig,
    pub projector: EventProjector,
    pub started: bool,
    pub status_enabled: bool,
    pub status_line: String,
    writer: W,
}

impl WorkerConsole<Stdout> {
    pub fn new(config: WorkerConfig) -> Self {
        let stdout = io::stdout();
        let status_enabled = stdout.is_terminal();
        Self::with_writer(config, stdout, status_enabled)
    }
}

impl<W: Write> WorkerConsole<W> {
    pub fn with_writer(config: WorkerConfig, writer: W, status_enabled: bool) -> Self {
        Self {
            config,
            projector: EventProjector::new(12, 4),
            started: false,
            status_enabled,
            status_line: String::new(),
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
            writeln!(self.writer, "{}", self.status_line)?;
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
        let label = truncate_text(if task_title.is_empty() { task_id } else { task_title }, 96);
        self.print_stream_line(&self.format_timeline_line(
            &format!("Задача завершена: {label} (id={task_id}, run={run_id}, status={status}{suffix})"),
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
            "subagent" => self.print_stream_line(&self.format_timeline_line(
                &truncate_text(message, 180),
                "info",
            )),
            _ => Ok(()),
        }
    }

    pub fn on_event(&mut self, event: &EventRecord) -> io::Result<()> {
        self.projector.apply_event(event);
        self.update_status_line();
        self.print_stream_line(&self.format_event_line(event))
    }

    pub fn format_event_line(&self, event: &EventRecord) -> String {
        self.format_timeline_line(&summarize_event(event), self.event_timeline_kind(event))
    }

    pub fn format_timeline_line(&self, message: &str, _kind: &str) -> String {
        let lines: Vec<&str> = if message.is_empty() {
            vec![""]
        } else {
            message.lines().collect()
        };
        let mut out = String::new();
        if let Some(first) = lines.first() {
            out.push_str("○ ");
            out.push_str(first);
        }
        for line in lines.iter().skip(1) {
            out.push('\n');
            out.push_str("│   ");
            out.push_str(line);
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
        write!(self.writer, "\r\x1b[2K{}", self.status_line)?;
        self.writer.flush()
    }

    fn event_timeline_kind(&self, event: &EventRecord) -> &'static str {
        match event.event_type.as_str() {
            "agent.turn.failed" | "raw.unparsed" | "stderr.line" | "error" => "failed",
            "agent.turn.completed" => "completed",
            "tool.result" => {
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
                        _ => "default",
                    }
                }
            }
            "file.change" => "file",
            "tool.call" => "info",
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
