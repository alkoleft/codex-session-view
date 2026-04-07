use codex_worker_rs::events::projector::{summarize_event, EventProjector};
use codex_worker_rs::models::EventRecord;
use serde_json::json;

fn make_event(event_type: &str, payload: serde_json::Value) -> EventRecord {
    EventRecord {
        schema_version: 1,
        ts: "2026-03-23T00:00:00Z".to_string(),
        task_id: "demo".to_string(),
        run_id: "run-1".to_string(),
        seq: 1,
        event_type: event_type.to_string(),
        raw_type: event_type.to_string(),
        parse_status: "parsed".to_string(),
        payload,
    }
}

#[test]
fn projector_builds_agent_tree_and_timeline() {
    let mut projector = EventProjector::new(10, 4);
    projector.reset_run("demo", "Demo", "run-1", Some("/repo".to_string()));
    projector.apply_event(&make_event("thread.started", json!({"thread_id":"parent"})));
    projector.apply_event(&make_event(
        "collab.spawn_agent",
        json!({
            "tool_name": "spawn_agent",
            "phase": "completed",
            "status": "completed",
            "thread_id": "parent",
            "sender_thread_id": "parent",
            "prompt": "check repo",
            "receiver_thread_ids": ["sub-1"],
            "agents_states": {"sub-1": {"status": "pending_init", "message": "warming up"}}
        }),
    ));
    projector.apply_event(&make_event(
        "agent.session",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "parent_thread_id": "parent",
            "agent_nickname": "Lovelace",
            "agent_role": "reviewer",
            "cwd": "/repo"
        }),
    ));
    projector.apply_event(&make_event(
        "message.agent",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "parent_thread_id": "parent",
            "text": "checking"
        }),
    ));
    projector.apply_event(&make_event(
        "shell.call",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "tool_name": "exec_command"
        }),
    ));
    projector.apply_event(&make_event(
        "shell.result",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "tool_name": "exec_command",
            "output": "14\n"
        }),
    ));

    let snapshot = &projector.snapshot;
    assert_eq!(snapshot.task_id.as_deref(), Some("demo"));
    assert_eq!(snapshot.root_thread_id.as_deref(), Some("parent"));
    assert!(snapshot.agents.contains_key("parent"));
    assert!(snapshot.agents.contains_key("sub-1"));
    assert_eq!(
        snapshot
            .agents
            .get("sub-1")
            .and_then(|agent| agent.parent_thread_id.as_deref()),
        Some("parent")
    );
    assert_eq!(
        snapshot
            .agents
            .get("sub-1")
            .and_then(|agent| agent.nickname.as_deref()),
        Some("Lovelace")
    );
    assert_eq!(
        snapshot
            .agents
            .get("sub-1")
            .and_then(|agent| agent.role.as_deref()),
        Some("reviewer")
    );
    assert!(snapshot
        .agents
        .get("sub-1")
        .and_then(|agent| agent.color.as_ref())
        .is_some());
    assert!(snapshot
        .agents
        .get("sub-1")
        .into_iter()
        .flat_map(|agent| agent.recent_lines.iter())
        .any(|line| line.contains("warming up")));
    assert!(snapshot
        .agents
        .get("sub-1")
        .into_iter()
        .flat_map(|agent| agent.recent_lines.iter())
        .any(|line| line.contains("result exec_command: 14")));
    assert!(snapshot
        .timeline
        .iter()
        .any(|entry| entry.label.starts_with("subagent launch [spawn_agent]")));
}

#[test]
fn projector_releases_subagent_color_after_completion() {
    let mut projector = EventProjector::new(10, 4);
    projector.apply_event(&make_event("thread.started", json!({"thread_id":"parent"})));
    projector.apply_event(&make_event(
        "agent.session",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "parent_thread_id": "parent",
            "agent_nickname": "Lovelace"
        }),
    ));
    let claimed_color = projector
        .snapshot
        .agents
        .get("sub-1")
        .and_then(|agent| agent.color.clone());

    projector.apply_event(&make_event(
        "agent.meta",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "parent_thread_id": "parent",
            "meta_type": "task_complete"
        }),
    ));

    assert!(claimed_color.is_some());
    assert!(projector
        .snapshot
        .agents
        .get("sub-1")
        .and_then(|agent| agent.color.as_ref())
        .is_none());

    projector.apply_event(&make_event(
        "agent.session",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-2",
            "parent_thread_id": "parent",
            "agent_nickname": "Confucius"
        }),
    ));

    assert_eq!(
        projector
            .snapshot
            .agents
            .get("sub-2")
            .and_then(|agent| agent.color.as_deref()),
        claimed_color.as_deref()
    );
}

#[test]
fn projector_prefers_agent_session_parent_for_nested_subagent() {
    let mut projector = EventProjector::new(10, 4);
    projector.apply_event(&make_event("thread.started", json!({"thread_id":"root"})));
    projector.apply_event(&make_event(
        "message.agent",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-2",
            "parent_thread_id": "root",
            "text": "fallback parent"
        }),
    ));
    projector.apply_event(&make_event(
        "agent.session",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-2",
            "parent_thread_id": "sub-1",
            "agent_nickname": "Nested"
        }),
    ));

    assert_eq!(
        projector
            .snapshot
            .agents
            .get("sub-2")
            .and_then(|agent| agent.parent_thread_id.as_deref()),
        Some("sub-1")
    );
}

#[test]
fn projector_does_not_downgrade_terminal_status_after_agent_session() {
    let mut projector = EventProjector::new(10, 4);
    projector.apply_event(&make_event("thread.started", json!({"thread_id":"parent"})));
    projector.apply_event(&make_event(
        "collab.wait",
        json!({
            "tool_name": "wait",
            "phase": "completed",
            "status": "completed",
            "thread_id": "parent",
            "sender_thread_id": "parent",
            "receiver_thread_ids": ["sub-1"],
            "agents_states": {"sub-1": {"status": "completed", "message": "done"}}
        }),
    ));
    projector.apply_event(&make_event(
        "agent.session",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "parent_thread_id": "parent",
            "agent_nickname": "Lovelace"
        }),
    ));

    assert_eq!(
        projector
            .snapshot
            .agents
            .get("sub-1")
            .map(|agent| agent.status.as_str()),
        Some("completed")
    );
}

#[test]
fn summarize_event_formats_subagent_state() {
    let summary = summarize_event(&make_event(
        "collab.wait",
        json!({
            "tool_name": "wait",
            "phase": "completed",
            "status": "completed",
            "receiver_thread_ids": ["sub-1"],
            "agents_states": {"sub-1": {"status": "completed", "message": "ok"}}
        }),
    ));
    assert!(summary.contains("subagent wait [wait]"));
    assert!(summary.contains("agents=sub-1"));
    assert!(summary.contains("states=sub-1:completed"));
}

#[test]
fn summarize_event_formats_file_change() {
    let summary = summarize_event(&make_event(
        "file.change",
        json!({
            "changes": [
                {"kind": "update", "path": "/tmp/demo.py"},
                {"kind": "add", "path": "/tmp/new.py"}
            ]
        }),
    ));
    assert!(summary.contains("files: update"));
    assert!(summary.contains("/tmp/demo.py"));
    assert!(summary.contains("(+1)"));
}

#[test]
fn summarize_event_formats_command_execution_with_command() {
    let call_summary = summarize_event(&make_event(
        "shell.call",
        json!({
            "tool_name": "command_execution",
            "input": {"command": "git status --short"}
        }),
    ));
    let result_summary = summarize_event(&make_event(
        "shell.result",
        json!({
            "tool_name": "command_execution",
            "input": {"command": "git status --short"},
            "exit_code": 0
        }),
    ));
    assert_eq!(
        call_summary,
        "command [command_execution]: git status --short"
    );
    assert_eq!(
        result_summary,
        "command ok [command_execution]: git status --short exit=0"
    );
}

#[test]
fn summarize_event_truncates_overlong_command_lines() {
    let summary = summarize_event(&make_event(
        "shell.call",
        json!({
            "tool_name": "command_execution",
            "input": {"command": format!("python -c '{}'", "x".repeat(200))}
        }),
    ));
    assert!(summary.len() < 150);
    assert!(summary.contains("..."));
}

#[test]
fn summarize_event_formats_web_open_result_with_url() {
    let summary = summarize_event(&make_event(
        "web.open",
        json!({
            "tool_name": "web_search",
            "phase": "completed",
            "output": {
                "query": "https://developers.openai.com/codex/cli",
                "action": {
                    "type": "open_page",
                    "url": "https://developers.openai.com/codex/cli"
                }
            }
        }),
    ));
    assert!(summary.contains("open page result [web_search]"));
    assert!(summary.contains("url=https://developers.openai.com/codex/cli"));
    assert!(!summary.contains("action=open_page"));
    assert!(!summary.contains("query=https://developers.openai.com/codex/cli"));
}

#[test]
fn summarize_event_formats_session_web_open_result_with_web_search_call_label() {
    let summary = summarize_event(&make_event(
        "web.open",
        json!({
            "tool_name": "web_search_call",
            "phase": "completed",
            "output": {
                "action": {
                    "type": "open_page",
                    "url": "https://iana.org/domains/example"
                }
            }
        }),
    ));
    assert_eq!(
        summary,
        "open page result [web_search_call]: url=https://iana.org/domains/example"
    );
}

#[test]
fn summarize_event_formats_web_search_call_without_query() {
    let summary = summarize_event(&make_event(
        "web.search",
        json!({
            "tool_name": "web_search",
            "phase": "started",
            "input": {
                "query": "",
                "action": {"type": "other"}
            }
        }),
    ));
    assert_eq!(summary, "search [web_search]: query=<pending> action=other");
}

#[test]
fn summarize_event_formats_stdin_write() {
    let summary = summarize_event(&make_event(
        "stdin.write",
        json!({
            "phase": "completed",
            "status": "completed",
            "input": {
                "session_id": 42,
                "chars": "ls\n"
            },
            "output": {
                "stdout": "ok"
            }
        }),
    ));
    assert_eq!(
        summary,
        "stdin write: phase=completed status=completed session_id=42 chars=ls output=ok"
    );
}

#[test]
fn summarize_event_uses_plain_message_text() {
    let agent_summary = summarize_event(&make_event(
        "message.assistant",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "role": "system",
            "text": "hello"
        }),
    ));
    let commentary_summary = summarize_event(&make_event(
        "message.assistant",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "role": "assistant",
            "phase": "commentary",
            "text": "thinking aloud"
        }),
    ));

    assert_eq!(agent_summary, "hello");
    assert_eq!(commentary_summary, "thinking aloud");
}

#[test]
fn summarize_event_formats_mcp_result_with_ordered_details() {
    let summary = summarize_event(&make_event(
        "mcp.result",
        json!({
            "phase": "completed",
            "status": "completed",
            "arguments": {"q": "a"},
            "result": {"ok": true},
            "server": "docs",
            "tool": "fetch_docs"
        }),
    ));
    assert_eq!(
        summary,
        "mcp result: phase=completed status=completed arguments={\"q\":\"a\"} result={\"ok\":true} server=docs tool=fetch_docs"
    );
}

#[test]
fn summarize_event_formats_subagent_launch_and_session() {
    let launch_summary = summarize_event(&make_event(
        "collab.spawn_agent",
        json!({
            "tool_name": "spawn_agent",
            "phase": "completed",
            "status": "completed",
            "prompt": "review tree",
            "receiver_thread_ids": ["sub-1"],
            "agents_states": {"sub-1": {"status": "pending_init"}}
        }),
    ));
    let session_summary = summarize_event(&make_event(
        "agent.session",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "agent_nickname": "Lovelace",
            "agent_role": "reviewer",
            "cwd": "/repo"
        }),
    ));

    assert_eq!(
        launch_summary,
        "subagent launch [spawn_agent]: phase=completed status=completed prompt=review tree agents=sub-1 states=sub-1:pending_init"
    );
    assert_eq!(
        session_summary,
        "ready: role=reviewer nickname=Lovelace cwd=/repo"
    );
}

#[test]
fn summarize_event_formats_subagent_task_started() {
    let summary = summarize_event(&make_event(
        "agent.meta",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "meta_type": "task_started",
            "collaboration_mode_kind": "default",
            "turn_id": "turn-1",
            "model_context_window": 256000
        }),
    ));

    assert_eq!(summary, "started: mode=default turn=turn-1 window=256000");
}

#[test]
fn summarize_event_formats_subagent_task_complete() {
    let summary = summarize_event(&make_event(
        "task.completed",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "turn_id": "turn-1",
            "last_agent_message": "**Result** APPROVED"
        }),
    ));

    assert_eq!(summary, "done: **Result** APPROVED");
}

#[test]
fn summarize_event_formats_subagent_command_call() {
    let summary = summarize_event(&make_event(
        "shell.call",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "tool_name": "exec_command",
            "input": {"cmd": "ping -c 1 1.1.1.1"}
        }),
    ));

    assert_eq!(summary, "tool exec_command: ping -c 1 1.1.1.1");
}

#[test]
fn summarize_event_formats_subagent_plan_update() {
    let summary = summarize_event(&make_event(
        "plan.update",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "phase": "started",
            "tool_name": "update_plan",
            "input": {
                "explanation": "sync state",
                "plan": [
                    {"step": "Inspect", "status": "completed"},
                    {"step": "Patch", "status": "in_progress"}
                ]
            }
        }),
    ));

    assert_eq!(
        summary,
        "plan update: phase=started explanation=sync state steps=2"
    );
}

#[test]
fn summarize_event_formats_request_user_input() {
    let summary = summarize_event(&make_event(
        "user.input.request",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "phase": "completed",
            "tool_name": "request_user_input",
            "input": {
                "questions": [
                    {
                        "header": "Поиск детей",
                        "id": "child_lookup",
                        "question": "Как искать дочерние session-файлы?"
                    }
                ]
            },
            "output": {
                "answers": {
                    "child_lookup": {
                        "answers": ["Тот же каталог (Recommended)"]
                    }
                }
            }
        }),
    ));

    assert_eq!(
        summary,
        "user input request: phase=completed questions=1 first=Поиск детей answers=1"
    );
}

#[test]
fn summarize_event_formats_commentary_patch_and_context_events() {
    let commentary = summarize_event(&make_event(
        "message.commentary",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "text": "Проверяю структуру дерева"
        }),
    ));
    let patch = summarize_event(&make_event(
        "patch.apply",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "phase": "completed",
            "status": "completed",
            "input": "*** Begin Patch\n*** Update File: src/events/readers.rs\n*** End Patch\n",
            "changes": {
                "src/events/readers.rs": {"type":"update"}
            }
        }),
    ));
    let runtime_context = summarize_event(&make_event(
        "runtime.context",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "cwd": "/repo",
            "model": "gpt-5.4",
            "collaboration_mode": {
                "mode": "default"
            }
        }),
    ));
    let compacted = summarize_event(&make_event(
        "context.compacted",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "message": "trimmed",
            "replacement_history": [{"type":"message"}, {"type":"message"}]
        }),
    ));

    assert_eq!(commentary, "Проверяю структуру дерева");
    assert_eq!(
        patch,
        "patch apply: phase=completed status=completed file=src/events/readers.rs"
    );
    assert_eq!(
        runtime_context,
        "context: cwd=/repo model=gpt-5.4 mode=default"
    );
    assert_eq!(compacted, "context compacted: items=2 message=trimmed");
}

#[test]
fn summarize_event_formats_subagent_meta_variants() {
    let agent_summary = summarize_event(&make_event(
        "agent.meta",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "meta_type": "message",
            "role": "assistant",
            "text": "Thinking"
        }),
    ));
    let user_summary = summarize_event(&make_event(
        "agent.meta",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "meta_type": "user_message",
            "text": "Inspect docs/technical-documentation.md"
        }),
    ));
    let token_summary = summarize_event(&make_event(
        "info.tokens",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "input_tokens": 700,
            "cached_input_tokens": 120,
            "output_tokens": 340,
            "reasoning_output_tokens": 74,
            "total_tokens": 1234,
            "rate_limits": {"primary": {"used_percent": 4.0}}
        }),
    ));
    assert_eq!(agent_summary, "Thinking");
    assert_eq!(user_summary, "Inspect docs/technical-documentation.md");
    assert_eq!(
        token_summary,
        "tokens: input: 700, cached input: 120, output: 340, reasoning output: 74, total: 1 234"
    );
}

#[test]
fn summarize_event_formats_subagent_import_notes() {
    let foreign_session = summarize_event(&make_event(
        "agent.session.foreign",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "foreign_thread_id": "sub-foreign",
            "agent_role": "reviewer",
            "agent_nickname": "Ada",
            "cwd": "/repo"
        }),
    ));
    let patch_duplicate = summarize_event(&make_event(
        "patch.apply.duplicate",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "tool_name": "apply_patch",
            "phase": "completed",
            "status": "completed",
            "output": "Success"
        }),
    ));
    let compacted_duplicate = summarize_event(&make_event(
        "context.compacted.duplicate",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "duplicate_of": "compacted"
        }),
    ));

    assert_eq!(
        foreign_session,
        "foreign session meta: thread=sub-foreign role=reviewer nickname=Ada cwd=/repo"
    );
    assert_eq!(
        patch_duplicate,
        "patch apply duplicate: phase=completed status=completed output=Success"
    );
    assert_eq!(
        compacted_duplicate,
        "context compacted duplicate: source=compacted"
    );
}

#[test]
fn summarize_event_formats_error_event() {
    let summary = summarize_event(&make_event(
        "error",
        json!({
            "message": "Falling back from WebSockets to HTTPS transport."
        }),
    ));
    assert_eq!(
        summary,
        "error: Falling back from WebSockets to HTTPS transport."
    );
}

#[test]
fn summarize_event_includes_text_link_context() {
    let summary = summarize_event(&make_event(
        "error",
        json!({
            "message": "Reconnecting... 2/5",
            "text_links": {
                "request_id": "8c25d52e-8391-4ef8-8ffc-ef0fdcc2b3ec",
                "status_code": 403,
                "host": "chatgpt.com",
                "cf_ray": "9e0ef7775a834d74-FRA"
            }
        }),
    ));
    assert!(summary.contains("request_id=8c25d52e-8391-4ef8-8ffc-ef0fdcc2b3ec"));
    assert!(summary.contains("status=403"));
    assert!(summary.contains("host=chatgpt.com"));
    assert!(summary.contains("cf_ray=9e0ef7775a834d74-FRA"));
}

#[test]
fn summarize_event_formats_turn_failed_with_text_links() {
    let summary = summarize_event(&make_event(
        "agent.failed",
        json!({
            "error": {"message": "stream disconnected before completion"},
            "text_links": {
                "request_id": "e5bb5dbd-7494-4d6b-b105-f00335db8b6f",
                "host": "chatgpt.com"
            }
        }),
    ));
    assert!(summary.contains("agent failed"));
    assert!(summary.contains("stream disconnected before completion"));
    assert!(summary.contains("request_id=e5bb5dbd-7494-4d6b-b105-f00335db8b6f"));
}
