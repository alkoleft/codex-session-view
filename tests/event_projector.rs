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
        "tool.result",
        json!({
            "tool_name": "spawn_agent",
            "phase": "completed",
            "status": "completed",
            "thread_id": "parent",
            "sender_thread_id": "parent",
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
        "agent.message",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "parent_thread_id": "parent",
            "text": "checking"
        }),
    ));
    projector.apply_event(&make_event(
        "tool.call",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "tool_name": "exec_command"
        }),
    ));
    projector.apply_event(&make_event(
        "tool.result",
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
        snapshot.agents.get("sub-1").and_then(|agent| agent.parent_thread_id.as_deref()),
        Some("parent")
    );
    assert_eq!(
        snapshot.agents.get("sub-1").and_then(|agent| agent.nickname.as_deref()),
        Some("Lovelace")
    );
    assert_eq!(
        snapshot.agents.get("sub-1").and_then(|agent| agent.role.as_deref()),
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
        .any(|entry| entry.label.starts_with("subagent: spawn_agent")));
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
fn summarize_event_formats_subagent_state() {
    let summary = summarize_event(&make_event(
        "tool.result",
        json!({
            "tool_name": "wait",
            "phase": "completed",
            "status": "completed",
            "receiver_thread_ids": ["sub-1"],
            "agents_states": {"sub-1": {"status": "completed", "message": "ok"}}
        }),
    ));
    assert!(summary.contains("subagent: wait"));
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
        "tool.call",
        json!({
            "tool_name": "command_execution",
            "input": {"command": "git status --short"}
        }),
    ));
    let result_summary = summarize_event(&make_event(
        "tool.result",
        json!({
            "tool_name": "command_execution",
            "input": {"command": "git status --short"},
            "exit_code": 0
        }),
    ));
    assert_eq!(call_summary, "tool: command_execution git status --short");
    assert_eq!(
        result_summary,
        "tool result: command_execution git status --short exit=0"
    );
}

#[test]
fn summarize_event_truncates_overlong_command_lines() {
    let summary = summarize_event(&make_event(
        "tool.call",
        json!({
            "tool_name": "command_execution",
            "input": {"command": format!("python -c '{}'", "x".repeat(200))}
        }),
    ));
    assert!(summary.len() < 150);
    assert!(summary.contains("..."));
}

#[test]
fn summarize_event_formats_web_search_result_with_query() {
    let summary = summarize_event(&make_event(
        "tool.result",
        json!({
            "tool_name": "web_search",
            "output": {
                "query": "https://developers.openai.com/codex/cli",
                "action": {
                    "type": "open_page",
                    "url": "https://developers.openai.com/codex/cli"
                }
            }
        }),
    ));
    assert!(summary.contains("tool result: web_search"));
    assert!(summary.contains("query=https://developers.openai.com/codex/cli"));
    assert!(summary.contains("action=open_page"));
    assert!(summary.contains("url=https://developers.openai.com/codex/cli"));
}

#[test]
fn summarize_event_formats_web_search_call_without_query() {
    let summary = summarize_event(&make_event(
        "tool.call",
        json!({
            "tool_name": "web_search",
            "input": {
                "query": "",
                "action": {"type": "other"}
            }
        }),
    ));
    assert_eq!(summary, "tool: web_search query=<pending> action=other");
}

#[test]
fn summarize_event_formats_subagent_meta_variants() {
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
        "agent.meta",
        json!({
            "actor_type": "subagent",
            "thread_id": "sub-1",
            "meta_type": "token_count",
            "total_tokens": 1234,
            "rate_limits": {"primary": {"used_percent": 4.0}}
        }),
    ));
    assert_eq!(
        user_summary,
        "subagent[sub-1] user: Inspect docs/technical-documentation.md"
    );
    assert_eq!(token_summary, "subagent[sub-1] tokens total=1234 primary=4.0%");
}

#[test]
fn summarize_event_formats_error_event() {
    let summary = summarize_event(&make_event(
        "error",
        json!({
            "message": "Falling back from WebSockets to HTTPS transport."
        }),
    ));
    assert_eq!(summary, "error: Falling back from WebSockets to HTTPS transport.");
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
        "agent.turn.failed",
        json!({
            "error": {"message": "stream disconnected before completion"},
            "text_links": {
                "request_id": "e5bb5dbd-7494-4d6b-b105-f00335db8b6f",
                "host": "chatgpt.com"
            }
        }),
    ));
    assert!(summary.contains("agent turn failed"));
    assert!(summary.contains("stream disconnected before completion"));
    assert!(summary.contains("request_id=e5bb5dbd-7494-4d6b-b105-f00335db8b6f"));
}
