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

fn make_colored_console() -> WorkerConsole<Cursor<Vec<u8>>> {
    WorkerConsole::with_writer_options(worker_config(), Cursor::new(Vec::new()), false, true)
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
        .on_result(
            "task-1",
            "Починить оформление завершения",
            "run-123",
            "completed",
            None,
        )
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
        .on_claim(
            &make_task("task-1", "Починить оформление начала"),
            "run-123",
        )
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
        "message.agent",
        json!({"actor_type":"agent","thread_id":"root","text":"Сообщение ассистента"}),
    );
    assert_eq!(console.format_event_line(&event), "○ Сообщение ассистента");
}

#[test]
fn tool_events_render_as_timeline_rows() {
    let console = make_console();
    let event = make_event(
        "shell.call",
        json!({"actor_type":"agent","thread_id":"root","tool_name":"exec_command"}),
    );
    assert_eq!(
        console.format_event_line(&event),
        "○ command [exec_command]"
    );
}

#[test]
fn web_search_event_renders_structured_prefix() {
    let console = make_console();
    let event = make_event(
        "web.search",
        json!({
            "actor_type":"agent",
            "thread_id":"root",
            "tool_name":"web_search",
            "phase":"started",
            "input":{"query":"","action":{"type":"other"}}
        }),
    );
    assert_eq!(
        console.format_event_line(&event),
        "○ search [web_search]: query=<pending> action=other"
    );
}

#[test]
fn web_open_event_renders_structured_prefix() {
    let console = make_console();
    let event = make_event(
        "web.open",
        json!({
            "actor_type":"agent",
            "thread_id":"root",
            "tool_name":"web_search",
            "phase":"completed",
            "output":{
                "query":"https://developers.openai.com/codex/cli",
                "action":{"type":"open_page","url":"https://developers.openai.com/codex/cli"}
            }
        }),
    );
    assert_eq!(
        console.format_event_line(&event),
        "○ open page result [web_search]: url=https://developers.openai.com/codex/cli"
    );
}

#[test]
fn stdin_write_event_renders_structured_prefix() {
    let console = make_console();
    let event = make_event(
        "stdin.write",
        json!({
            "actor_type":"agent",
            "thread_id":"root",
            "phase":"completed",
            "status":"completed",
            "input":{"session_id":42,"chars":"ls\n"},
            "output":{"stdout":"ok"}
        }),
    );
    assert_eq!(
        console.format_event_line(&event),
        "○ stdin write: phase=completed status=completed session_id=42 chars=ls output=ok"
    );
}

#[test]
fn info_tokens_event_renders_as_info_line() {
    let console = make_console();
    let event = make_event(
        "info.tokens",
        json!({
            "actor_type":"subagent",
            "thread_id":"sub-1",
            "input_tokens":700,
            "cached_input_tokens":120,
            "output_tokens":340,
            "reasoning_output_tokens":74,
            "total_tokens":1234,
            "rate_limits":{"primary":{"used_percent":4.0}}
        }),
    );
    assert_eq!(
        console.format_event_line(&event),
        "○ tokens: input: 700, cached input: 120, output: 340, reasoning output: 74, total: 1 234"
    );
}

#[test]
fn multiline_tool_commands_render_with_timeline_continuation() {
    let console = make_console();
    let event = make_event(
        "shell.call",
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
        "○ command [command_execution]: /bin/bash -lc \"python3 - <<'PY'\n│   from pathlib import Path\n│   print(Path.cwd())\n│   PY\""
    );
}

#[test]
fn tool_results_render_as_success_timeline_rows() {
    let console = make_console();
    let event = make_event(
        "shell.result",
        json!({"actor_type":"agent","thread_id":"root","tool_name":"exec_command","exit_code":0}),
    );
    assert_eq!(
        console.format_event_line(&event),
        "○ command ok [exec_command]: exit=0"
    );
}

#[test]
fn tool_results_include_failed_exit_diagnostic() {
    let console = make_console();
    let event = make_event(
        "shell.result",
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
        "○ command fail [exec_command]: exit=1 -> permission denied"
    );
}

#[test]
fn subagent_launch_events_are_highlighted_in_stdout() {
    let console = make_console();
    let event = make_event(
        "collab.spawn_agent",
        json!({
            "actor_type":"agent",
            "thread_id":"root",
            "tool_name":"spawn_agent",
            "phase":"completed",
            "status":"completed",
            "prompt":"review tree",
            "receiver_thread_ids":["sub-1"],
            "agents_states":{"sub-1":{"status":"pending_init"}}
        }),
    );
    assert_eq!(
        console.format_event_line(&event),
        "○ subagent launch [spawn_agent]: phase=completed status=completed prompt=review tree agents=sub-1 states=sub-1:pending_init"
    );
}

#[test]
fn subagent_session_events_are_highlighted_in_stdout() {
    let console = make_console();
    let event = make_event(
        "agent.session",
        json!({
            "actor_type":"subagent",
            "thread_id":"sub-1",
            "agent_role":"reviewer",
            "agent_nickname":"Lovelace",
            "cwd":"/repo"
        }),
    );
    assert_eq!(
        console.format_event_line(&event),
        "○ ready: role=reviewer nickname=Lovelace cwd=/repo"
    );
}

#[test]
fn subagent_tool_command_renders_command_text() {
    let console = make_console();
    let event = make_event(
        "shell.call",
        json!({
            "actor_type":"subagent",
            "thread_id":"sub-1",
            "tool_name":"exec_command",
            "input":{"cmd":"ping -c 1 1.1.1.1"}
        }),
    );

    assert_eq!(
        console.format_event_line(&event),
        "○ tool exec_command: ping -c 1 1.1.1.1"
    );
}

#[test]
fn subagent_plan_update_renders_structured_summary() {
    let console = make_console();
    let event = make_event(
        "todo.update",
        json!({
            "actor_type":"subagent",
            "thread_id":"sub-1",
            "phase":"started",
            "tool_name":"update_plan",
            "input":{
                "explanation":"sync state",
                "plan":[
                    {"step":"Inspect","status":"completed"},
                    {"step":"Patch","status":"in_progress"}
                ]
            }
        }),
    );

    assert_eq!(
        console.format_event_line(&event),
        "○ todo update: phase=started explanation=sync state steps=2"
    );
}

#[test]
fn mcp_result_renders_structured_summary() {
    let console = make_console();
    let event = make_event(
        "mcp.result",
        json!({
            "phase":"completed",
            "status":"completed",
            "arguments":{"q":"a"},
            "result":{"ok":true},
            "server":"docs",
            "tool":"fetch_docs"
        }),
    );

    assert_eq!(
        console.format_event_line(&event),
        "○ mcp result: phase=completed status=completed arguments={\"q\":\"a\"} result={\"ok\":true} server=docs tool=fetch_docs"
    );
}

#[test]
fn subagent_stream_messages_get_readable_prefix() {
    let mut console = make_console();
    console
        .on_emit("subagent", "session imported thread_id=sub-1")
        .expect("subagent message should print");

    let output = String::from_utf8(console.into_inner().into_inner()).expect("utf-8 output");
    assert!(output.contains("○ subagent: session imported thread_id=sub-1"));
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

#[test]
fn command_call_and_result_share_category_color() {
    let console = make_colored_console();
    let call = make_event(
        "shell.call",
        json!({
            "actor_type":"agent",
            "thread_id":"root",
            "tool_name":"command_execution",
            "input":{"command":"/bin/bash -lc 'ping -c 1 1.1.1.1'"}
        }),
    );
    let result = make_event(
        "shell.result",
        json!({
            "actor_type":"agent",
            "thread_id":"root",
            "tool_name":"command_execution",
            "input":{"command":"/bin/bash -lc 'ping -c 1 1.1.1.1'"},
            "exit_code":0
        }),
    );

    let call_line = console.format_event_line(&call);
    let result_line = console.format_event_line(&result);

    assert!(call_line.contains("\u{1b}[1;34m○\u{1b}[0m"));
    assert!(call_line.contains(
        "\u{1b}[1;34mcommand [command_execution]:\u{1b}[0m /bin/bash -lc 'ping -c 1 1.1.1.1'"
    ));
    assert!(result_line.contains("\u{1b}[1;34m○\u{1b}[0m"));
    assert!(result_line.contains(
        "\u{1b}[1;34mcommand ok [command_execution]:\u{1b}[0m /bin/bash -lc 'ping -c 1 1.1.1.1' exit=0"
    ));
}

#[test]
fn command_failure_keeps_plain_body_after_colored_prefix() {
    let console = make_colored_console();
    let event = make_event(
        "shell.result",
        json!({
            "actor_type":"agent",
            "thread_id":"root",
            "tool_name":"command_execution",
            "input":{"command":"/bin/bash -lc 'ping -c 1 1.1.1.1'"},
            "exit_code":1,
            "stderr":"permission denied"
        }),
    );

    let line = console.format_event_line(&event);

    assert!(line.contains("\u{1b}[1;34m○\u{1b}[0m"));
    assert!(line.contains(
        "\u{1b}[1;34mcommand fail [command_execution]:\u{1b}[0m /bin/bash -lc 'ping -c 1 1.1.1.1' exit=1 -> permission denied"
    ));
}

#[test]
fn subagent_launch_and_ready_use_palette_color_for_thread_prefix() {
    let mut console = make_colored_console();
    let launch = make_event(
        "collab.spawn_agent",
        json!({
            "actor_type":"agent",
            "thread_id":"root",
            "tool_name":"spawn_agent",
            "phase":"completed",
            "status":"completed",
            "receiver_thread_ids":["sub-1"],
            "agents_states":{"sub-1":{"status":"pending_init"}}
        }),
    );
    let ready = make_event(
        "agent.session",
        json!({
            "actor_type":"subagent",
            "thread_id":"sub-1",
            "agent_role":"reviewer",
            "agent_nickname":"Lovelace"
        }),
    );

    console.projector.apply_event(&launch);
    let ready_line = console.format_event_line(&ready);

    assert!(ready_line.contains("\u{1b}[1;35m○\u{1b}[0m"));
    assert!(ready_line.contains("\u{1b}[1;35mready:\u{1b}[0m"));
}

#[test]
fn subagent_command_uses_palette_prefix_and_plain_body_after_blue_label() {
    let mut console = make_colored_console();
    let event = make_event(
        "shell.call",
        json!({
            "actor_type":"subagent",
            "thread_id":"sub-1",
            "tool_name":"exec_command",
            "input":{"cmd":"ping -c 1 1.1.1.1"}
        }),
    );

    console.projector.apply_event(&event);
    let line = console.format_event_line(&event);

    assert!(line.contains("\u{1b}[1;34m○\u{1b}[0m"));
    assert!(line.contains("\u{1b}[1;34mtool exec_command:\u{1b}[0m"));
    assert!(line.contains("\u{1b}[1;34mtool exec_command:\u{1b}[0m ping -c 1 1.1.1.1"));
}

#[test]
fn assistant_message_after_command_inherits_command_color() {
    let mut console = make_colored_console();
    let tool_result = make_event(
        "shell.result",
        json!({
            "actor_type":"agent",
            "thread_id":"root",
            "tool_name":"command_execution",
            "input":{"command":"/bin/bash -lc 'ping -c 1 1.1.1.1'"},
            "exit_code":0
        }),
    );
    let message = make_event(
        "message.agent",
        json!({
            "actor_type":"agent",
            "thread_id":"root",
            "text":"Проверочный прогон выполнен"
        }),
    );

    console.projector.apply_event(&tool_result);
    let line = console.format_event_line(&message);

    assert!(line.contains("\u{1b}[1;34m○\u{1b}[0m"));
    assert!(line.contains("\u{1b}[1;34mПроверочный прогон выполнен\u{1b}[0m"));
}

#[test]
fn subagent_message_after_command_inherits_command_color() {
    let mut console = make_colored_console();
    let tool_result = make_event(
        "shell.result",
        json!({
            "actor_type":"subagent",
            "thread_id":"sub-1",
            "tool_name":"exec_command",
            "output":{"exit_code":0}
        }),
    );
    let message = make_event(
        "message.agent",
        json!({
            "actor_type":"subagent",
            "thread_id":"sub-1",
            "text":"Команда выполнена успешно"
        }),
    );

    console.projector.apply_event(&tool_result);
    let line = console.format_event_line(&message);

    assert!(line.contains("\u{1b}[1;34m○\u{1b}[0m"));
    assert!(line.contains("\u{1b}[1;34mКоманда выполнена успешно\u{1b}[0m"));
}

#[test]
fn subagent_result_after_command_uses_single_category_color() {
    let mut console = make_colored_console();
    let event = make_event(
        "shell.result",
        json!({
            "actor_type":"subagent",
            "thread_id":"sub-1",
            "tool_name":"exec_command",
            "output":"Command: /bin/bash -lc 'ping -c 1 1.1.1.1'"
        }),
    );

    console.projector.apply_event(&event);
    let line = console.format_event_line(&event);

    assert!(line.contains("\u{1b}[1;34m○\u{1b}[0m"));
    assert!(line.contains("\u{1b}[1;34mresult exec_command:\u{1b}[0m"));
    assert!(line.contains(
        "\u{1b}[1;34mresult exec_command:\u{1b}[0m Command: /bin/bash -lc 'ping -c 1 1.1.1.1'"
    ));
}
