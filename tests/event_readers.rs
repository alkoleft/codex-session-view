use std::collections::{HashMap, HashSet};
use std::path::Path;

use codex_worker_rs::events::payloads::PayloadObject;
use codex_worker_rs::events::readers::{EventLogFileReader, JsonOutputEventReader, RunEventContext};

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
        assert_eq!(event.event_type, "agent.message");
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
        assert_eq!(event.event_type, "tool.result");
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
        payload.text_links.as_ref().and_then(|v| v.scheme.as_deref()),
        Some("wss")
    );
    assert_eq!(
        payload.text_links.as_ref().and_then(|v| v.host.as_deref()),
        Some("chatgpt.com")
    );
    assert_eq!(
        payload.text_links.as_ref().and_then(|v| v.cf_ray.as_deref()),
        Some("9e0ef7775a834d74-FRA")
    );
}

#[test]
fn reader_extracts_text_links_from_turn_failed() {
    let mut reader = make_reader();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let mut subagent_threads = HashSet::new();
    let (_, event) = reader.parse_main_output_line(
        0,
        r#"{"type":"turn.failed","error":{"message":"stream disconnected before completion: An error occurred while processing your request. Please include the request ID e5bb5dbd-7494-4d6b-b105-f00335db8b6f in your message."}}"#,
        &mut tool_counts,
        &mut subagent_counts,
        &mut subagent_threads,
    );

    assert_eq!(event.event_type, "agent.turn.failed");
    let PayloadObject::AgentTurn(payload) = event.payload_obj() else {
        panic!("expected turn payload");
    };
    assert_eq!(
        payload
            .text_links
            .as_ref()
            .and_then(|v| v.request_id.as_deref()),
        Some("e5bb5dbd-7494-4d6b-b105-f00335db8b6f")
    );
    assert_eq!(
        payload
            .text_links
            .as_ref()
            .and_then(|v| v.disconnect_reason.as_deref()),
        Some("stream disconnected before completion")
    );
}

#[test]
fn reader_preserves_agent_message_id_and_text_links() {
    let mut reader = make_reader();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let mut subagent_threads = HashSet::new();
    let (_, event) = reader.parse_main_output_line(
        0,
        r#"{"type":"item.completed","item":{"id":"item_42","type":"agent_message","text":"See failure request ID 01234567-89ab-cdef-0123-456789abcdef."}}"#,
        &mut tool_counts,
        &mut subagent_counts,
        &mut subagent_threads,
    );

    let PayloadObject::AgentMessage(payload) = event.payload_obj() else {
        panic!("expected message payload");
    };
    assert_eq!(payload.item_id.as_deref(), Some("item_42"));
    assert_eq!(
        payload
            .text_links
            .as_ref()
            .and_then(|v| v.request_id.as_deref()),
        Some("01234567-89ab-cdef-0123-456789abcdef")
    );
}

#[test]
fn event_log_reader_restores_payload_model_and_nested_text_links() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let path = tmp.path().join("events.jsonl");
    std::fs::write(
        &path,
        concat!(
            "{\"schema_version\":1,\"ts\":\"2026-03-23T00:00:00Z\",\"task_id\":\"demo\",\"run_id\":\"run-1\",\"seq\":1,\"event_type\":\"tool.result\",\"raw_type\":\"item.completed\",\"parse_status\":\"parsed\",\"payload\":{\"tool_name\":\"command_execution\"}}\n",
            "{\"schema_version\":1,\"ts\":\"2026-03-23T00:00:00Z\",\"task_id\":\"demo\",\"run_id\":\"run-1\",\"seq\":2,\"event_type\":\"error\",\"raw_type\":\"error\",\"parse_status\":\"parsed\",\"payload\":{\"message\":\"msg\",\"text_links\":{\"request_id\":\"8c25d52e-8391-4ef8-8ffc-ef0fdcc2b3ec\",\"status_code\":403}}}\n"
        ),
    )
    .expect("events should be written");

    let events = EventLogFileReader::load_events(&path).expect("events should load");
    assert_eq!(events.len(), 2);
    assert!(matches!(events[0].payload_obj(), PayloadObject::ToolResult(_)));
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
fn mcp_web_search_todo_are_parsed_without_raw_unparsed() {
    let mut reader = make_reader();
    let mut seq = 0u64;
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let mut subagent_threads = HashSet::new();

    let inputs = [
        r#"{"type":"item.completed","item":{"type":"mcp_tool_call","id":"mcp-1","tool":"fetch_docs","status":"completed","server":"docs","arguments":{"q":"a"},"result":{"ok":true}}}"#,
        r#"{"type":"item.completed","item":{"type":"web_search","id":"web-1","query":"rust","action":"search"}}"#,
        r#"{"type":"item.completed","item":{"type":"todo_list","id":"todo-1","status":"completed","items":[{"text":"a","completed":true},{"text":"b","completed":false}]}}"#,
    ];

    for line in inputs {
        let (next, event) = reader.parse_main_output_line(
            seq,
            line,
            &mut tool_counts,
            &mut subagent_counts,
            &mut subagent_threads,
        );
        assert_ne!(event.event_type, "raw.unparsed");
        seq = next;
    }
}

#[test]
fn nested_item_wrapper_is_unwrapped_recursively() {
    let mut reader = make_reader();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let mut subagent_threads = HashSet::new();

    let (_, event) = reader.parse_main_output_line(
        0,
        r#"{"type":"item.completed","item":{"type":"item.updated","item":{"type":"item.started","item":{"type":"command_execution","id":"cmd-nested","command":"echo ok","status":"running"}}}}"#,
        &mut tool_counts,
        &mut subagent_counts,
        &mut subagent_threads,
    );

    assert_eq!(event.event_type, "tool.call");
    assert_eq!(event.payload["tool_name"].as_str(), Some("command_execution"));
    assert_eq!(event.payload["tool_use_id"].as_str(), Some("cmd-nested"));
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
fn collab_tool_call_is_normalized_and_updates_subagent_counts() {
    let mut reader = make_reader();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let mut subagent_threads = HashSet::new();
    let (_, event) = reader.parse_main_output_line(
        0,
        r#"{"type":"item.completed","item":{"type":"collab_tool_call","id":"ct-1","tool":"spawn_agent","status":"completed","sender_thread_id":"root-1","receiver_thread_ids":["sub-1"],"prompt":"go","agents_states":{"sub-1":{"status":"ok","message":"done"}}}}"#,
        &mut tool_counts,
        &mut subagent_counts,
        &mut subagent_threads,
    );
    assert_eq!(event.event_type, "tool.result");
    assert_ne!(event.event_type, "raw.unparsed");
    assert_eq!(event.payload["tool_name"].as_str(), Some("spawn_agent"));
    assert_eq!(subagent_counts.get("spawn_agent").copied(), Some(1));
}

#[test]
fn collab_tool_call_wait_has_high_detection_confidence() {
    let mut reader = make_reader();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let mut subagent_threads = HashSet::new();
    let (_, event) = reader.parse_main_output_line(
        0,
        r#"{"type":"item.completed","item":{"type":"collab_tool_call","id":"ct-2","tool":"wait","status":"completed","sender_thread_id":"root-1","receiver_thread_ids":["sub-1"],"prompt":"wait","agents_states":{"sub-1":{"status":"ok","message":"done"}}}}"#,
        &mut tool_counts,
        &mut subagent_counts,
        &mut subagent_threads,
    );
    assert_eq!(event.event_type, "tool.result");
    assert_eq!(event.payload["tool_name"].as_str(), Some("wait"));
    assert_eq!(
        event.payload["detection_confidence"].as_str(),
        Some("high")
    );
}

#[test]
fn root_tool_use_subagent_tool_has_detection_fields() {
    let mut reader = make_reader();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let mut subagent_threads = HashSet::new();

    let (_, event) = reader.parse_main_output_line(
        0,
        r#"{"type":"item.completed","item":{"type":"tool_use","id":"tu-1","name":"spawn_agent","input":{"goal":"x"}}}"#,
        &mut tool_counts,
        &mut subagent_counts,
        &mut subagent_threads,
    );

    assert_eq!(event.event_type, "tool.call");
    assert_eq!(event.payload["tool_name"].as_str(), Some("spawn_agent"));
    assert_eq!(event.payload["detection_source"].as_str(), Some("tool_name"));
    assert_eq!(event.payload["detection_confidence"].as_str(), Some("high"));
    assert_eq!(event.payload["raw_ref"].as_str(), Some("tu-1"));
    assert_eq!(subagent_counts.get("spawn_agent").copied(), Some(1));
}

#[test]
fn text_link_regexes_are_case_insensitive() {
    let mut reader = make_reader();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let mut subagent_threads = HashSet::new();

    let (_, top_error) = reader.parse_main_output_line(
        0,
        r#"{"type":"error","message":"reconnecting... 2/5 (unexpected status 403 Forbidden, url: wss://chatgpt.com/backend-api/codex/responses, CF-RAY: 9e0ef7775a834d74-FRA). fAlLiNg BaCk FrOm WebSockets to HTTPS transport. Please include the request ID 8c25d52e-8391-4ef8-8ffc-ef0fdcc2b3ec in your message."}"#,
        &mut tool_counts,
        &mut subagent_counts,
        &mut subagent_threads,
    );
    assert_eq!(
        top_error.payload["text_links"]["cf_ray"].as_str(),
        Some("9e0ef7775a834d74-FRA")
    );
    assert_eq!(top_error.payload["is_fallback"].as_bool(), Some(true));
    assert_eq!(
        top_error.payload["fallback_from"].as_str(),
        Some("websocket")
    );
    assert_eq!(top_error.payload["fallback_to"].as_str(), Some("https"));

    let (_, turn_failed) = reader.parse_main_output_line(
        1,
        r#"{"type":"turn.failed","error":{"message":"StReAm DiScOnNeCtEd BeFoRe CoMpLeTiOn: failure happened."}}"#,
        &mut tool_counts,
        &mut subagent_counts,
        &mut subagent_threads,
    );
    assert_eq!(
        turn_failed.payload["text_links"]["disconnect_reason"].as_str(),
        Some("StReAm DiScOnNeCtEd BeFoRe CoMpLeTiOn")
    );
}

#[test]
fn payload_obj_keeps_subagent_tool_fields_after_event_log_roundtrip() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let path = tmp.path().join("events.jsonl");
    std::fs::write(
        &path,
        "{\"schema_version\":1,\"ts\":\"2026-04-05T00:00:00Z\",\"task_id\":\"demo\",\"run_id\":\"run-1\",\"seq\":1,\"event_type\":\"tool.call\",\"raw_type\":\"response_item\",\"parse_status\":\"parsed\",\"payload\":{\"actor_type\":\"subagent\",\"thread_id\":\"thread-1\",\"parent_thread_id\":\"parent-1\",\"session_path\":\"/tmp/subagent.jsonl\",\"tool_name\":\"spawn_agent\",\"tool_use_id\":\"call-1\",\"input\":{\"goal\":\"x\"}}}\n",
    )
    .expect("event log should be written");

    let events = EventLogFileReader::load_events(&path).expect("events should load");
    let PayloadObject::ToolCall(payload) = events[0].payload_obj() else {
        panic!("expected tool.call payload");
    };
    assert_eq!(payload.parent_thread_id.as_deref(), Some("parent-1"));
    assert_eq!(payload.session_path.as_deref(), Some("/tmp/subagent.jsonl"));
}

#[test]
fn payload_obj_keeps_collab_fields_after_event_log_roundtrip() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let path = tmp.path().join("events.jsonl");
    std::fs::write(
        &path,
        "{\"schema_version\":1,\"ts\":\"2026-04-05T00:00:00Z\",\"task_id\":\"demo\",\"run_id\":\"run-1\",\"seq\":2,\"event_type\":\"tool.result\",\"raw_type\":\"item.completed\",\"parse_status\":\"parsed\",\"payload\":{\"actor_type\":\"agent\",\"thread_id\":\"root-1\",\"tool_name\":\"wait\",\"tool_use_id\":\"ct-2\",\"phase\":\"completed\",\"status\":\"completed\",\"sender_thread_id\":\"root-1\",\"receiver_thread_ids\":[\"sub-1\"],\"prompt\":\"wait\",\"agents_states\":{\"sub-1\":{\"status\":\"ok\"}},\"detection_source\":\"tool_payload\",\"detection_confidence\":\"high\",\"raw_ref\":\"ct-2\"}}\n",
    )
    .expect("event log should be written");

    let events = EventLogFileReader::load_events(&path).expect("events should load");
    let PayloadObject::ToolResult(payload) = events[0].payload_obj() else {
        panic!("expected tool.result payload");
    };
    assert_eq!(payload.sender_thread_id.as_deref(), Some("root-1"));
    assert_eq!(
        payload.receiver_thread_ids,
        Some(vec!["sub-1".to_string()])
    );
    assert_eq!(payload.detection_source.as_deref(), Some("tool_payload"));
    assert_eq!(payload.detection_confidence.as_deref(), Some("high"));
    assert_eq!(payload.raw_ref.as_deref(), Some("ct-2"));
}

#[test]
fn subagent_session_meta_own_vs_foreign() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let own: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"session_meta","payload":{"id":"thread-1","forked_from_id":"root","cwd":"/tmp","agent_nickname":"A","agent_role":"worker","source":{"subagent":{"thread_spawn":{"parent_thread_id":"parent-x"}}}}}"#,
    )
    .expect("json should parse");
    let event = reader
        .parse_subagent_session_payload(
            1,
            &own,
            imported,
            "parent-fallback",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("own session meta should produce event");
    assert_eq!(event.event_type, "agent.session");
    assert_eq!(
        event.payload["parent_thread_id"].as_str(),
        Some("parent-x")
    );

    let foreign: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(r#"{"type":"session_meta","payload":{"id":"thread-foreign"}}"#)
            .expect("json should parse");
    let skipped = reader.parse_subagent_session_payload(
        2,
        &foreign,
        imported,
        "parent-fallback",
        "thread-1",
        &mut call_names,
        &mut tool_counts,
        &mut subagent_counts,
    );
    assert!(skipped.is_none());
}

#[test]
fn subagent_function_call_and_output_normalization() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let call_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"function_call","name":"exec_command","call_id":"call-1","arguments":"{\"cmd\":\"echo ok\"}"}}"#,
    )
    .expect("json should parse");
    let call = reader
        .parse_subagent_session_payload(
            10,
            &call_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("function_call should produce event");
    assert_eq!(call.event_type, "tool.call");
    assert_eq!(call.payload["tool_name"].as_str(), Some("exec_command"));
    assert_eq!(call.payload["tool_use_id"].as_str(), Some("call-1"));

    let output_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"function_call_output","call_id":"call-1","output":{"exit_code":0}}}"#,
    )
    .expect("json should parse");
    let output = reader
        .parse_subagent_session_payload(
            11,
            &output_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("function_call_output should produce event");
    assert_eq!(output.event_type, "tool.result");
    assert_eq!(output.payload["tool_name"].as_str(), Some("exec_command"));
    assert_eq!(output.payload["tool_use_id"].as_str(), Some("call-1"));
    assert_eq!(output.payload["output"]["exit_code"].as_i64(), Some(0));
}

#[test]
fn subagent_function_call_updates_subagent_counts_for_spawn_agent() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"function_call","name":"spawn_agent","call_id":"call-spawn","arguments":"{\"goal\":\"x\"}"}}"#,
    )
    .expect("json should parse");

    let event = reader
        .parse_subagent_session_payload(
            14,
            &payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("function_call should produce event");

    assert_eq!(event.event_type, "tool.call");
    assert_eq!(event.payload["tool_name"].as_str(), Some("spawn_agent"));
    assert_eq!(subagent_counts.get("spawn_agent").copied(), Some(1));
}

#[test]
fn subagent_response_item_message_and_reasoning_are_normalized() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let message_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"text":"hello"},{"text":"world"}]}}"#,
    )
    .expect("json should parse");
    let message = reader
        .parse_subagent_session_payload(
            12,
            &message_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("message should produce event");
    assert_eq!(message.event_type, "agent.message");
    assert_eq!(message.payload["text"].as_str(), Some("hello\nworld"));

    let reasoning_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"reasoning","summary":[{"text":"step1"},{"text":"step2"}]}}"#,
    )
    .expect("json should parse");
    let reasoning = reader
        .parse_subagent_session_payload(
            13,
            &reasoning_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("reasoning should produce event");
    assert_eq!(reasoning.event_type, "agent.reasoning");
    assert_eq!(reasoning.payload["text"].as_str(), Some("step1\nstep2"));
}

#[test]
fn subagent_event_msg_agent_message_and_meta_normalization() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let agent_message: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"event_msg","payload":{"type":"agent_message","message":"Please include the request ID 01234567-89ab-cdef-0123-456789abcdef in your message.","phase":"completed"}}"#,
    )
    .expect("json should parse");
    let event = reader
        .parse_subagent_session_payload(
            20,
            &agent_message,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("event_msg agent_message should produce event");
    assert_eq!(event.event_type, "agent.message");
    assert_eq!(
        event.payload["text_links"]["request_id"].as_str(),
        Some("01234567-89ab-cdef-0123-456789abcdef")
    );

    let meta_message: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"total_tokens":123}}}}"#,
    )
    .expect("json should parse");
    let meta = reader
        .parse_subagent_session_payload(
            21,
            &meta_message,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("event_msg meta should produce event");
    assert_eq!(meta.event_type, "agent.meta");
    assert_eq!(meta.payload["meta_type"].as_str(), Some("token_count"));
    assert_eq!(meta.payload["total_tokens"].as_i64(), Some(123));
    assert!(meta.payload.get("raw").is_none());

    let complete_message: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-1","last_agent_message":"done"}}"#,
    )
    .expect("json should parse");
    let complete = reader
        .parse_subagent_session_payload(
            22,
            &complete_message,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("event_msg task_complete should produce event");
    assert_eq!(complete.event_type, "agent.meta");
    assert_eq!(complete.payload["meta_type"].as_str(), Some("task_complete"));
    assert_eq!(complete.payload["turn_id"].as_str(), Some("turn-1"));
    assert_eq!(complete.payload["last_agent_message"].as_str(), Some("done"));
    assert!(complete.payload.get("raw").is_none());

    let unknown_meta_message: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"event_msg","payload":{"type":"custom_meta","x":1}}"#,
    )
    .expect("json should parse");
    let unknown_meta = reader
        .parse_subagent_session_payload(
            23,
            &unknown_meta_message,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("event_msg unknown meta should produce event");
    assert_eq!(unknown_meta.event_type, "agent.meta");
    assert_eq!(unknown_meta.payload["meta_type"].as_str(), Some("custom_meta"));
    assert_eq!(unknown_meta.payload["raw"]["type"].as_str(), Some("custom_meta"));
}

#[test]
fn subagent_turn_context_normalization() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let turn_context: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"turn_context","payload":{"turn_id":"t-1","cwd":"/workspace","current_date":"2026-04-05","timezone":"UTC","approval_policy":"never","sandbox_policy":{"type":"workspace-write"},"model":"gpt-5","effort":"medium","summary":"ok","collaboration_mode":{"mode":"default"}}}"#,
    )
    .expect("json should parse");
    let event = reader
        .parse_subagent_session_payload(
            30,
            &turn_context,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("turn_context should produce event");
    assert_eq!(event.event_type, "agent.meta");
    assert_eq!(event.payload["meta_type"].as_str(), Some("turn_context"));
    assert_eq!(event.payload["turn_id"].as_str(), Some("t-1"));
    assert_eq!(event.payload["cwd"].as_str(), Some("/workspace"));
    assert_eq!(event.payload["sandbox_policy_type"].as_str(), Some("workspace-write"));
    assert_eq!(event.payload["collaboration_mode_kind"].as_str(), Some("default"));
}
