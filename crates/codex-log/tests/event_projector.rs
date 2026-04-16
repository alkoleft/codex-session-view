use codex_log::events::projector::{summarize_event, EventProjector};
use codex_log::EventRecord;
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
}
