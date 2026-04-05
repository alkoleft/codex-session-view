use std::collections::{BTreeMap, BTreeSet};
use std::io::Cursor;
use std::path::PathBuf;

use codex_worker_rs::models::{EventRecord, TaskBlock, WorkerConfig};
use codex_worker_rs::ui::console::WorkerConsole;
use serde_json::json;

const TS: &str = "2026-03-24T09:41:00Z";

fn worker_config() -> WorkerConfig {
    WorkerConfig {
        task_file: PathBuf::from("tasks.md"),
        codex_bin: "codex".to_string(),
        codex_home: None,
        logs_dir: None,
        default_cwd: None,
        model: None,
        sandbox: None,
        approval_policy: None,
        prompt_template: None,
        poll_interval: 1.0,
        stale_after: 30.0,
        dry_run: false,
        log_to_stdout: false,
    }
}

fn make_console() -> WorkerConsole<Cursor<Vec<u8>>> {
    WorkerConsole::with_writer(worker_config(), Cursor::new(Vec::new()), false)
}

fn make_task(id: &str, title: &str) -> TaskBlock {
    let mut metadata = BTreeMap::new();
    metadata.insert("id".to_string(), id.to_string());
    TaskBlock {
        title: title.to_string(),
        status: " ".to_string(),
        metadata,
        explicit_metadata_keys: BTreeSet::new(),
        explicit_metadata_order: Vec::new(),
        body: String::new(),
        raw_text: String::new(),
        start: 0,
        end: 0,
    }
}

fn make_event(event_type: &str, payload: serde_json::Value) -> EventRecord {
    EventRecord {
        schema_version: 1,
        ts: TS.to_string(),
        task_id: "task-1".to_string(),
        run_id: "run-1".to_string(),
        seq: 1,
        event_type: event_type.to_string(),
        raw_type: String::new(),
        parse_status: "parsed".to_string(),
        payload,
    }
}

#[test]
fn status_line_uses_compact_infographic_format() {
    let mut console = make_console();
    console.projector.set_claimed(
        "task".to_string(),
        "Переработай строку состояния".to_string(),
        "f369e241-8c5f-46e7-984b-35f81ee3d0db".to_string(),
    );
    console.update_status_line();

    assert_eq!(
        console.status_line,
        "◔ claim · Переработай строку состояния · #f369e241"
    );
    assert!(!console.status_line.contains("status:"));
    assert!(!console.status_line.contains("task:"));
    assert!(!console.status_line.contains("run:"));
}

#[test]
fn status_line_falls_back_to_status_badge_only() {
    let mut console = make_console();
    console.projector.set_status("idle".to_string(), None);
    console.update_status_line();
    assert_eq!(console.status_line, "○ idle");
}

#[test]
fn on_result_prints_task_title_in_finish_banner() {
    let mut console = make_console();
    console
        .on_result("task-1", "Починить оформление завершения", "run-123", "completed", None)
        .expect("result banner should print");

    let output = String::from_utf8(console.into_inner().into_inner()).expect("utf-8 output");
    assert!(output.contains("○ Задача завершена:"));
    assert!(output.contains("Починить оформление завершения"));
    assert!(output.contains("id=task-1"));
    assert!(output.contains("run=run-123"));
    assert!(output.contains("status=completed"));
}

#[test]
fn on_claim_prints_task_title_in_start_banner() {
    let mut console = make_console();
    console
        .on_claim(&make_task("task-1", "Починить оформление начала"), "run-123")
        .expect("claim banner should print");

    let output = String::from_utf8(console.into_inner().into_inner()).expect("utf-8 output");
    assert!(output.contains("○ Задача взята в работу:"));
    assert!(output.contains("Починить оформление начала"));
    assert!(output.contains("id=task-1"));
    assert!(output.contains("run=run-123"));
}

#[test]
fn assistant_messages_render_as_timeline_rows() {
    let console = make_console();
    let event = make_event(
        "agent.message",
        json!({"actor_type":"agent","thread_id":"root","text":"Сообщение ассистента"}),
    );
    assert_eq!(console.format_event_line(&event), "○ assistant: Сообщение ассистента");
}

#[test]
fn tool_events_render_as_timeline_rows() {
    let console = make_console();
    let event = make_event(
        "tool.call",
        json!({"actor_type":"agent","thread_id":"root","tool_name":"exec_command"}),
    );
    assert_eq!(console.format_event_line(&event), "○ tool: exec_command");
}

#[test]
fn web_search_event_renders_structured_prefix() {
    let console = make_console();
    let event = make_event(
        "tool.call",
        json!({
            "actor_type":"agent",
            "thread_id":"root",
            "tool_name":"web_search",
            "input":{"query":"","action":{"type":"other"}}
        }),
    );
    assert_eq!(
        console.format_event_line(&event),
        "○ tool: web_search query=<pending> action=other"
    );
}

#[test]
fn multiline_tool_commands_render_with_timeline_continuation() {
    let console = make_console();
    let event = make_event(
        "tool.call",
        json!({
            "actor_type":"agent",
            "thread_id":"root",
            "tool_name":"command_execution",
            "input":{
                "command":"/bin/bash -lc \"python3 - <<'PY'\nfrom pathlib import Path\nprint(Path.cwd())\nPY\""
            }
        }),
    );
    assert_eq!(
        console.format_event_line(&event),
        "○ tool: command_execution /bin/bash -lc \"python3 - <<'PY'\n│   from pathlib import Path\n│   print(Path.cwd())\n│   PY\""
    );
}

#[test]
fn tool_results_render_as_success_timeline_rows() {
    let console = make_console();
    let event = make_event(
        "tool.result",
        json!({"actor_type":"agent","thread_id":"root","tool_name":"exec_command","exit_code":0}),
    );
    assert_eq!(console.format_event_line(&event), "○ tool result: exec_command exit=0");
}

#[test]
fn tool_results_include_failed_exit_diagnostic() {
    let console = make_console();
    let event = make_event(
        "tool.result",
        json!({
            "actor_type":"agent",
            "thread_id":"root",
            "tool_name":"exec_command",
            "exit_code":1,
            "stderr":"permission denied"
        }),
    );
    assert_eq!(
        console.format_event_line(&event),
        "○ tool result: exec_command exit=1 -> permission denied"
    );
}

#[test]
fn file_add_changes_render_as_timeline_rows() {
    let console = make_console();
    let event = make_event(
        "file.change",
        json!({
            "actor_type":"agent",
            "thread_id":"root",
            "changes":[{"kind":"add","path":"src/codex_worker/console_ui.py"}]
        }),
    );
    assert_eq!(
        console.format_event_line(&event),
        "○ files: add src/codex_worker/console_ui.py"
    );
}

#[test]
fn file_update_changes_render_as_timeline_rows() {
    let console = make_console();
    let event = make_event(
        "file.change",
        json!({
            "actor_type":"agent",
            "thread_id":"root",
            "changes":[{"kind":"updated","path":"src/codex_worker/console_ui.py"}]
        }),
    );
    assert_eq!(
        console.format_event_line(&event),
        "○ files: updated src/codex_worker/console_ui.py"
    );
}
