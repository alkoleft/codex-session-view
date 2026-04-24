use codex_log::events::payloads::PayloadObject;
use codex_log::events::readers::{EventLogFileReader, JsonOutputEventReader, RunEventContext};
use std::collections::{HashMap, HashSet};

fn make_reader() -> JsonOutputEventReader {
    JsonOutputEventReader::new(
        RunEventContext {
            task_id: "demo".to_string(),
            run_id: "run-1".to_string(),
        },
        None,
    )
}

#[test]
fn reader_emits_agent_tool_and_file_event_types() {
    let mut reader = make_reader();
    let seq = 0u64;
    let mut tool_counts: HashMap<String, u64> = HashMap::new();
    let mut subagent_counts: HashMap<String, u64> = HashMap::new();
    let mut subagent_threads: HashSet<String> = HashSet::new();

    (seq, {
        let (_, event) = reader.parse_main_output_line(
            seq,
            r#"{"type":"item.completed","item":{"type":"agent_message","text":"ok"}}"#,
            &mut tool_counts,
            &mut subagent_counts,
            &mut subagent_threads,
        );
        assert_eq!(event.event_type, "message.agent");
        (event.seq, ())
    })
        .0;

    (seq, {
        let (_, event) = reader.parse_main_output_line(
            seq,
            r#"{"type":"item.completed","item":{"type":"command_execution","id":"cmd-1","command":"git status --short","status":"completed","exit_code":0}}"#,
            &mut tool_counts,
            &mut subagent_counts,
            &mut subagent_threads,
        );
        assert_eq!(event.event_type, "shell.result");
        (event.seq, ())
    })
        .0;

    let (_, event) = reader.parse_main_output_line(
        seq,
        r#"{"type":"item.completed","item":{"type":"file_change","id":"fc-1","changes":[{"kind":"update","path":"/tmp/demo.py"}]}}"#,
        &mut tool_counts,
        &mut subagent_counts,
        &mut subagent_threads,
    );
    assert_eq!(event.event_type, "file.change");
}

#[test]
fn reader_emits_error_event_type_for_item_error() {
    let mut reader = make_reader();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let mut subagent_threads = HashSet::new();
    let (_, event) = reader.parse_main_output_line(
        0,
        r#"{"type":"item.completed","item":{"id":"item_0","type":"error","message":"Falling back from WebSockets to HTTPS transport."}}"#,
        &mut tool_counts,
        &mut subagent_counts,
        &mut subagent_threads,
    );

    assert_eq!(event.event_type, "error");
    assert_eq!(
        reader.state.item_errors[0]["message"].as_str(),
        Some("Falling back from WebSockets to HTTPS transport.")
    );
    assert!(matches!(event.payload_obj(), PayloadObject::Error(_)));
}

#[test]
fn reader_emits_error_event_type_for_top_level_error() {
    let mut reader = make_reader();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let mut subagent_threads = HashSet::new();
    let (_, event) = reader.parse_main_output_line(
        0,
        r#"{"type":"error","message":"Reconnecting... 2/5 (stream disconnected before completion)"}"#,
        &mut tool_counts,
        &mut subagent_counts,
        &mut subagent_threads,
    );

    assert_eq!(event.event_type, "error");
    assert_eq!(event.parse_status, "parsed");
}

#[test]
fn reader_extracts_text_links_from_item_error() {
    let mut reader = make_reader();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let mut subagent_threads = HashSet::new();
    let (_, event) = reader.parse_main_output_line(
        0,
        r#"{"type":"item.completed","item":{"id":"item_0","type":"error","message":"Falling back from WebSockets to HTTPS transport. Please include the request ID ce2d4bff-3a69-481f-9219-7deb60d3f722 in your message."}}"#,
        &mut tool_counts,
        &mut subagent_counts,
        &mut subagent_threads,
    );

    let PayloadObject::Error(payload) = event.payload_obj() else {
        panic!("expected error payload");
    };
    assert_eq!(payload.is_fallback, Some(true));
    assert_eq!(payload.fallback_from.as_deref(), Some("websocket"));
    assert_eq!(payload.fallback_to.as_deref(), Some("https"));
    assert_eq!(
        payload.request_id.as_deref(),
        Some("ce2d4bff-3a69-481f-9219-7deb60d3f722")
    );
}

#[test]
fn reader_extracts_text_links_from_top_level_error() {
    let mut reader = make_reader();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let mut subagent_threads = HashSet::new();
    let (_, event) = reader.parse_main_output_line(
        0,
        r#"{"type":"error","message":"Reconnecting... 2/5 (unexpected status 403 Forbidden: 154c, url: wss://chatgpt.com/backend-api/codex/responses, cf-ray: 9e0ef7775a834d74-FRA). Please include the request ID 8c25d52e-8391-4ef8-8ffc-ef0fdcc2b3ec in your message."}"#,
        &mut tool_counts,
        &mut subagent_counts,
        &mut subagent_threads,
    );

    let PayloadObject::Error(payload) = event.payload_obj() else {
        panic!("expected error payload");
    };
    assert_eq!(
        payload.request_id.as_deref(),
        Some("8c25d52e-8391-4ef8-8ffc-ef0fdcc2b3ec")
    );
    assert_eq!(payload.reconnect_attempt, Some(2));
    assert_eq!(payload.reconnect_limit, Some(5));
    assert_eq!(
        payload
            .text_links
            .as_ref()
            .and_then(|v| v.status_code)
            .map(u32::from),
        Some(403)
    );
    assert_eq!(
        payload
            .text_links
            .as_ref()
            .and_then(|v| v.status_text.as_deref()),
        Some("Forbidden")
    );
    assert_eq!(
        payload.text_links.as_ref().and_then(|v| v.url.as_deref()),
        Some("wss://chatgpt.com/backend-api/codex/responses")
    );
    assert_eq!(
        payload
            .text_links
            .as_ref()
            .and_then(|v| v.scheme.as_deref()),
        Some("wss")
    );
    assert_eq!(
        payload.text_links.as_ref().and_then(|v| v.host.as_deref()),
        Some("chatgpt.com")
    );
    assert_eq!(
        payload
            .text_links
            .as_ref()
            .and_then(|v| v.cf_ray.as_deref()),
        Some("9e0ef7775a834d74-FRA")
    );
}

#[test]
fn event_log_reader_restores_payload_model_and_nested_text_links() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let path = tmp.path().join("events.jsonl");
    std::fs::write(
        &path,
        concat!(
            "{\"schema_version\":1,\"ts\":\"2026-03-23T00:00:00Z\",\"task_id\":\"demo\",\"run_id\":\"run-1\",\"seq\":1,\"event_type\":\"shell.result\",\"raw_type\":\"item.completed\",\"parse_status\":\"parsed\",\"payload\":{\"tool_name\":\"command_execution\"}}\n",
            "{\"schema_version\":1,\"ts\":\"2026-03-23T00:00:00Z\",\"task_id\":\"demo\",\"run_id\":\"run-1\",\"seq\":2,\"event_type\":\"error\",\"raw_type\":\"error\",\"parse_status\":\"parsed\",\"payload\":{\"message\":\"msg\",\"text_links\":{\"request_id\":\"8c25d52e-8391-4ef8-8ffc-ef0fdcc2b3ec\",\"status_code\":403}}}\n"
        ),
    )
    .expect("events should be written");

    let events = EventLogFileReader::load_events(&path).expect("events should load");
    assert_eq!(events.len(), 2);
    assert!(matches!(
        events[0].payload_obj(),
        PayloadObject::ToolResult(_)
    ));
    let PayloadObject::Error(payload) = events[1].payload_obj() else {
        panic!("expected error payload");
    };
    assert_eq!(
        payload
            .text_links
            .as_ref()
            .and_then(|v| v.request_id.as_deref()),
        Some("8c25d52e-8391-4ef8-8ffc-ef0fdcc2b3ec")
    );
    assert_eq!(
        payload
            .text_links
            .as_ref()
            .and_then(|v| v.status_code)
            .map(u32::from),
        Some(403)
    );
}

#[test]
fn invalid_json_is_fail_closed() {
    let mut reader = make_reader();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let mut subagent_threads = HashSet::new();
    let (_, event) = reader.parse_main_output_line(
        0,
        "{invalid",
        &mut tool_counts,
        &mut subagent_counts,
        &mut subagent_threads,
    );

    assert!(!reader.state.valid_json);
    assert_eq!(event.event_type, "raw.unparsed");
    assert_eq!(event.raw_type, "invalid_json");
}

#[test]
fn command_execution_completed_preserves_aggregated_output() {
    let mut reader = make_reader();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let mut subagent_threads = HashSet::new();

    let (_, event) = reader.parse_main_output_line(
        0,
        r#"{"type":"item.completed","item":{"type":"command_execution","id":"cmd-1","command":"printf 'a\nb\n'","status":"completed","exit_code":0,"aggregated_output":"a\nb\n"}}"#,
        &mut tool_counts,
        &mut subagent_counts,
        &mut subagent_threads,
    );

    assert_eq!(event.event_type, "shell.result");
    assert_eq!(
        event.payload["tool_name"].as_str(),
        Some("command_execution")
    );
    assert_eq!(event.payload["output"].as_str(), Some("a\nb\n"));
}

#[test]
fn root_reasoning_item_is_normalized_to_agent_reasoning() {
    let mut reader = make_reader();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let mut subagent_threads = HashSet::new();

    let (_, event) = reader.parse_main_output_line(
        0,
        r#"{"type":"item.completed","item":{"type":"reasoning","text":"thinking..."}}"#,
        &mut tool_counts,
        &mut subagent_counts,
        &mut subagent_threads,
    );
    assert_eq!(event.event_type, "agent.reasoning");
    assert_eq!(event.payload["text"].as_str(), Some("thinking..."));
}

#[test]
fn shell_events_capture_skill_identifier_marker_for_skill_file_reads() {
    let mut reader = make_reader();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let mut subagent_threads = HashSet::new();

    let (_, event) = reader.parse_main_output_line(
        0,
        r#"{"type":"item.completed","item":{"type":"command_execution","id":"cmd-1","command":"sed -n '1,120p' /repo/.codex/skills/openspec-apply-change/SKILL.md","status":"completed","exit_code":0,"aggregated_output":"..."}}"#,
        &mut tool_counts,
        &mut subagent_counts,
        &mut subagent_threads,
    );

    assert_eq!(event.event_type, "shell.result");
    assert_eq!(
        event.payload["skill_identifiers"][0].as_str(),
        Some("openspec-apply-change")
    );
}
