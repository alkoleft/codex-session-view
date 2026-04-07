use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use codex_worker_rs::events::payloads::PayloadObject;
use codex_worker_rs::events::readers::{
    EventLogFileReader, JsonOutputEventReader, RunEventContext,
};
use serde_json::json;

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

    assert_eq!(event.event_type, "agent.failed");
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
fn mcp_tool_call_is_normalized_as_dedicated_event_types() {
    let mut reader = make_reader();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let mut subagent_threads = HashSet::new();

    let (_, call) = reader.parse_main_output_line(
        0,
        r#"{"type":"item.started","item":{"type":"mcp_tool_call","id":"mcp-1","tool":"fetch_docs","status":"in_progress","server":"docs","arguments":{"q":"a"}}}"#,
        &mut tool_counts,
        &mut subagent_counts,
        &mut subagent_threads,
    );
    assert_eq!(call.event_type, "mcp.call");
    assert_eq!(call.payload["tool"].as_str(), Some("fetch_docs"));
    assert_eq!(call.payload["server"].as_str(), Some("docs"));
    assert_eq!(call.payload["arguments"]["q"].as_str(), Some("a"));

    let (_, result) = reader.parse_main_output_line(
        1,
        r#"{"type":"item.completed","item":{"type":"mcp_tool_call","id":"mcp-1","tool":"fetch_docs","status":"completed","server":"docs","arguments":{"q":"a"},"result":{"ok":true}}}"#,
        &mut tool_counts,
        &mut subagent_counts,
        &mut subagent_threads,
    );
    assert_eq!(result.event_type, "mcp.result");
    assert_eq!(result.payload["tool"].as_str(), Some("fetch_docs"));
    assert_eq!(result.payload["server"].as_str(), Some("docs"));
    assert_eq!(result.payload["arguments"]["q"].as_str(), Some("a"));
    assert_eq!(result.payload["result"]["ok"].as_bool(), Some(true));
    assert_eq!(tool_counts.get("fetch_docs").copied(), Some(2));
}

#[test]
fn web_search_is_normalized_as_dedicated_event_type() {
    let mut reader = make_reader();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let mut subagent_threads = HashSet::new();

    let (_, event) = reader.parse_main_output_line(
        0,
        r#"{"type":"item.started","item":{"type":"web_search","id":"web-1","query":"","action":{"type":"other"}}}"#,
        &mut tool_counts,
        &mut subagent_counts,
        &mut subagent_threads,
    );

    assert_eq!(event.event_type, "web.search");
    assert_eq!(event.payload["tool_name"].as_str(), Some("web_search"));
    assert_eq!(event.payload["phase"].as_str(), Some("started"));
}

#[test]
fn web_open_is_normalized_as_dedicated_event_type() {
    let mut reader = make_reader();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let mut subagent_threads = HashSet::new();

    let (_, event) = reader.parse_main_output_line(
        0,
        r#"{"type":"item.completed","item":{"type":"web_search","id":"web-1","query":"https://developers.openai.com/codex/cli","action":{"type":"open_page","url":"https://developers.openai.com/codex/cli"}}}"#,
        &mut tool_counts,
        &mut subagent_counts,
        &mut subagent_threads,
    );

    assert_eq!(event.event_type, "web.open");
    assert_eq!(event.payload["tool_name"].as_str(), Some("web_search"));
    assert_eq!(event.payload["phase"].as_str(), Some("completed"));
    assert_eq!(
        event.payload["output"]["action"]["type"].as_str(),
        Some("open_page")
    );
    assert_eq!(
        event.payload["output"]["action"]["url"].as_str(),
        Some("https://developers.openai.com/codex/cli")
    );
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

    assert_eq!(event.event_type, "shell.call");
    assert_eq!(
        event.payload["tool_name"].as_str(),
        Some("command_execution")
    );
    assert_eq!(event.payload["tool_use_id"].as_str(), Some("cmd-nested"));
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
    assert_eq!(event.event_type, "collab.spawn_agent");
    assert_ne!(event.event_type, "raw.unparsed");
    assert_eq!(event.payload["tool_name"].as_str(), Some("spawn_agent"));
    assert_eq!(subagent_counts.get("spawn_agent").copied(), Some(1));
}

#[test]
fn collab_tool_call_started_is_normalized_as_tool_call() {
    let mut reader = make_reader();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let mut subagent_threads = HashSet::new();
    let (_, event) = reader.parse_main_output_line(
        0,
        r#"{"type":"item.started","item":{"type":"collab_tool_call","id":"ct-1","tool":"spawn_agent","status":"in_progress","sender_thread_id":"root-1","receiver_thread_ids":[],"prompt":"go","agents_states":{}}}"#,
        &mut tool_counts,
        &mut subagent_counts,
        &mut subagent_threads,
    );

    assert_eq!(event.event_type, "collab.spawn_agent");
    assert_eq!(event.payload["tool_name"].as_str(), Some("spawn_agent"));
    assert_eq!(event.payload["phase"].as_str(), Some("started"));
    assert_eq!(event.payload["status"].as_str(), Some("in_progress"));
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
    assert_eq!(event.event_type, "collab.wait");
    assert_eq!(event.payload["tool_name"].as_str(), Some("wait"));
    assert_eq!(event.payload["detection_confidence"].as_str(), Some("high"));
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

    assert_eq!(event.event_type, "collab.spawn_agent");
    assert_eq!(event.payload["tool_name"].as_str(), Some("spawn_agent"));
    assert_eq!(
        event.payload["detection_source"].as_str(),
        Some("tool_name")
    );
    assert_eq!(event.payload["detection_confidence"].as_str(), Some("high"));
    assert_eq!(event.payload["raw_ref"].as_str(), Some("tu-1"));
    assert_eq!(event.payload["phase"].as_str(), Some("started"));
    assert_eq!(subagent_counts.get("spawn_agent").copied(), Some(1));
}

#[test]
fn root_tool_use_write_stdin_is_normalized_as_dedicated_event_type() {
    let mut reader = make_reader();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let mut subagent_threads = HashSet::new();

    let (_, started) = reader.parse_main_output_line(
        0,
        r#"{"type":"item.completed","item":{"type":"tool_use","id":"stdin-1","name":"write_stdin","input":{"session_id":42,"chars":"ls\n"}}}"#,
        &mut tool_counts,
        &mut subagent_counts,
        &mut subagent_threads,
    );
    assert_eq!(started.event_type, "stdin.write");
    assert_eq!(started.payload["phase"].as_str(), Some("started"));
    assert_eq!(started.payload["input"]["session_id"].as_i64(), Some(42));
    assert_eq!(started.payload["input"]["chars"].as_str(), Some("ls\n"));

    let (_, completed) = reader.parse_main_output_line(
        1,
        r#"{"type":"item.completed","item":{"type":"tool_result","id":"stdin-out-1","tool_use_id":"stdin-1","name":"write_stdin","output":{"stdout":"ok"}}}"#,
        &mut tool_counts,
        &mut subagent_counts,
        &mut subagent_threads,
    );
    assert_eq!(completed.event_type, "stdin.write");
    assert_eq!(completed.payload["phase"].as_str(), Some("completed"));
    assert_eq!(completed.payload["tool_use_id"].as_str(), Some("stdin-1"));
    assert_eq!(completed.payload["output"]["stdout"].as_str(), Some("ok"));
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
        "{\"schema_version\":1,\"ts\":\"2026-04-05T00:00:00Z\",\"task_id\":\"demo\",\"run_id\":\"run-1\",\"seq\":1,\"event_type\":\"collab.spawn_agent\",\"raw_type\":\"response_item\",\"parse_status\":\"parsed\",\"payload\":{\"actor_type\":\"subagent\",\"thread_id\":\"thread-1\",\"parent_thread_id\":\"parent-1\",\"session_path\":\"/tmp/subagent.jsonl\",\"tool_name\":\"spawn_agent\",\"tool_use_id\":\"call-1\",\"phase\":\"started\",\"input\":{\"goal\":\"x\"}}}\n",
    )
    .expect("event log should be written");

    let events = EventLogFileReader::load_events(&path).expect("events should load");
    let PayloadObject::ToolResult(payload) = events[0].payload_obj() else {
        panic!("expected collab payload");
    };
    assert_eq!(payload.parent_thread_id.as_deref(), Some("parent-1"));
    assert_eq!(payload.session_path.as_deref(), Some("/tmp/subagent.jsonl"));
    assert_eq!(payload.phase.as_deref(), Some("started"));
}

#[test]
fn payload_obj_keeps_collab_fields_after_event_log_roundtrip() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let path = tmp.path().join("events.jsonl");
    std::fs::write(
        &path,
        "{\"schema_version\":1,\"ts\":\"2026-04-05T00:00:00Z\",\"task_id\":\"demo\",\"run_id\":\"run-1\",\"seq\":2,\"event_type\":\"collab.wait\",\"raw_type\":\"item.completed\",\"parse_status\":\"parsed\",\"payload\":{\"actor_type\":\"agent\",\"thread_id\":\"root-1\",\"tool_name\":\"wait\",\"tool_use_id\":\"ct-2\",\"phase\":\"completed\",\"status\":\"completed\",\"sender_thread_id\":\"root-1\",\"receiver_thread_ids\":[\"sub-1\"],\"prompt\":\"wait\",\"agents_states\":{\"sub-1\":{\"status\":\"ok\"}},\"detection_source\":\"tool_payload\",\"detection_confidence\":\"high\",\"raw_ref\":\"ct-2\"}}\n",
    )
    .expect("event log should be written");

    let events = EventLogFileReader::load_events(&path).expect("events should load");
    let PayloadObject::ToolResult(payload) = events[0].payload_obj() else {
        panic!("expected tool.result payload");
    };
    assert_eq!(payload.sender_thread_id.as_deref(), Some("root-1"));
    assert_eq!(payload.receiver_thread_ids, Some(vec!["sub-1".to_string()]));
    assert_eq!(payload.detection_source.as_deref(), Some("tool_payload"));
    assert_eq!(payload.detection_confidence.as_deref(), Some("high"));
    assert_eq!(payload.raw_ref.as_deref(), Some("ct-2"));
}

#[test]
fn payload_obj_keeps_mcp_fields_after_event_log_roundtrip() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let path = tmp.path().join("events.jsonl");
    std::fs::write(
        &path,
        "{\"schema_version\":1,\"ts\":\"2026-04-05T00:00:00Z\",\"task_id\":\"demo\",\"run_id\":\"run-1\",\"seq\":3,\"event_type\":\"mcp.result\",\"raw_type\":\"item.completed\",\"parse_status\":\"parsed\",\"payload\":{\"actor_type\":\"agent\",\"thread_id\":\"root-1\",\"tool_use_id\":\"mcp-1\",\"status\":\"completed\",\"phase\":\"completed\",\"arguments\":{\"q\":\"a\"},\"result\":{\"ok\":true},\"server\":\"docs\",\"tool\":\"fetch_docs\"}}\n",
    )
    .expect("event log should be written");

    let events = EventLogFileReader::load_events(&path).expect("events should load");
    let PayloadObject::McpResult(payload) = events[0].payload_obj() else {
        panic!("expected mcp.result payload");
    };
    assert_eq!(payload.tool.as_deref(), Some("fetch_docs"));
    assert_eq!(payload.server.as_deref(), Some("docs"));
    assert_eq!(
        payload
            .arguments
            .as_ref()
            .and_then(|v| v.get("q"))
            .and_then(|v| v.as_str()),
        Some("a")
    );
    assert_eq!(
        payload
            .result
            .as_ref()
            .and_then(|v| v.get("ok"))
            .and_then(|v| v.as_bool()),
        Some(true)
    );
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
            "parent-x",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("own session meta should produce event");
    assert_eq!(event.event_type, "agent.session");
    assert_eq!(event.payload["parent_thread_id"].as_str(), Some("parent-x"));

    let foreign: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(r#"{"type":"session_meta","payload":{"id":"thread-foreign"}}"#)
            .expect("json should parse");
    let foreign_event = reader
        .parse_subagent_session_payload(
            2,
            &foreign,
            imported,
            "parent-fallback",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("foreign session meta should produce event");
    assert_eq!(foreign_event.event_type, "agent.session.foreign");
    assert_eq!(
        foreign_event.payload["thread_id"].as_str(),
        Some("thread-1")
    );
    assert_eq!(
        foreign_event.payload["foreign_thread_id"].as_str(),
        Some("thread-foreign")
    );
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
    assert_eq!(call.event_type, "shell.call");
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
    assert_eq!(output.event_type, "shell.result");
    assert_eq!(output.payload["tool_name"].as_str(), Some("exec_command"));
    assert_eq!(output.payload["tool_use_id"].as_str(), Some("call-1"));
    assert_eq!(output.payload["output"]["exit_code"].as_i64(), Some(0));
}

#[test]
fn subagent_update_plan_function_call_and_output_normalization() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let call_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"function_call","name":"update_plan","call_id":"plan-1","arguments":"{\"explanation\":\"sync state\",\"plan\":[{\"step\":\"Inspect\",\"status\":\"completed\"},{\"step\":\"Patch\",\"status\":\"in_progress\"}]}"}}"#,
    )
    .expect("json should parse");
    let call = reader
        .parse_subagent_session_payload(
            12,
            &call_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("update_plan function_call should produce event");
    assert_eq!(call.event_type, "plan.update");
    assert_eq!(call.payload["tool_name"].as_str(), Some("update_plan"));
    assert_eq!(call.payload["tool_use_id"].as_str(), Some("plan-1"));
    assert_eq!(call.payload["phase"].as_str(), Some("started"));

    let output_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"function_call_output","call_id":"plan-1","output":{"explanation":"done","plan":[{"step":"Inspect","status":"completed"},{"step":"Patch","status":"completed"}]}}}"#,
    )
    .expect("json should parse");
    let output = reader
        .parse_subagent_session_payload(
            13,
            &output_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("update_plan function_call_output should produce event");
    assert_eq!(output.event_type, "plan.update");
    assert_eq!(output.payload["tool_use_id"].as_str(), Some("plan-1"));
    assert_eq!(output.payload["phase"].as_str(), Some("completed"));
    assert_eq!(
        output.payload["output"]["plan"].as_array().map(|v| v.len()),
        Some(2)
    );
}

#[test]
fn subagent_update_plan_function_call_sample_is_normalized() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let call_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"timestamp":"2026-04-06T19:42:26.171Z","type":"response_item","payload":{"type":"function_call","name":"update_plan","arguments":"{\"explanation\":\"Analysis approved; moving to branch decision and executable checklist before code changes.\",\"plan\":[{\"step\":\"Verify branch decision and working tree isolation for implementation branch\", \"status\":\"in_progress\"},{\"step\":\"Build approved executable TODO checklist from the plan\", \"status\":\"pending\"},{\"step\":\"Implement standalone session loading and metadata propagation for events_tree_html\", \"status\":\"pending\"},{\"step\":\"Add/adjust tests for standalone session input, routing, and regressions\", \"status\":\"pending\"},{\"step\":\"Run required checks, review diff, and prepare commit(s)\", \"status\":\"pending\"}]}","call_id":"call_MsxRurOW415xdvzIsTdask9e"}}"#,
    )
    .expect("json should parse");
    let call = reader
        .parse_subagent_session_payload(
            12,
            &call_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("update_plan sample function_call should produce event");

    assert_eq!(call.event_type, "plan.update");
    assert_eq!(call.payload["tool_name"].as_str(), Some("update_plan"));
    assert_eq!(
        call.payload["tool_use_id"].as_str(),
        Some("call_MsxRurOW415xdvzIsTdask9e")
    );
    assert_eq!(call.payload["phase"].as_str(), Some("started"));
    assert_eq!(
        call.payload["input"]["explanation"].as_str(),
        Some("Analysis approved; moving to branch decision and executable checklist before code changes.")
    );
    assert_eq!(
        call.payload["input"]["plan"].as_array().map(|v| v.len()),
        Some(5)
    );
    assert_eq!(
        call.payload["input"]["plan"][0]["status"].as_str(),
        Some("in_progress")
    );
}

#[test]
fn subagent_request_user_input_function_call_and_output_normalization() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let call_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"timestamp":"2026-04-06T19:21:22.374Z","type":"response_item","payload":{"type":"function_call","name":"request_user_input","arguments":"{\"questions\":[{\"header\":\"Поиск детей\",\"id\":\"child_lookup\",\"question\":\"Как искать дочерние session-файлы для HTML-дерева, когда вход — один session-файл?\",\"options\":[{\"label\":\"Тот же каталог (Recommended)\",\"description\":\"Искать только рядом с исходным файлом по `receiver_thread_ids`, без глобального сканирования `CODEX_HOME`.\"},{\"label\":\"Весь sessions root\",\"description\":\"Разрешить рекурсивный поиск по всему `.../sessions`, чтобы собрать дерево даже при разнесённых файлах.\"},{\"label\":\"Оба режима\",\"description\":\"По умолчанию искать рядом, но поддержать отдельный флаг для расширенного рекурсивного поиска.\"}]}]}","call_id":"call_4Bs6MMfml19vjtweSvGzdQrW"}}"#,
    )
    .expect("json should parse");
    let call = reader
        .parse_subagent_session_payload(
            13,
            &call_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("request_user_input function_call should produce event");
    assert_eq!(call.event_type, "user.input.request");
    assert_eq!(
        call.payload["tool_name"].as_str(),
        Some("request_user_input")
    );
    assert_eq!(call.payload["phase"].as_str(), Some("started"));
    assert_eq!(
        call.payload["input"]["questions"][0]["header"].as_str(),
        Some("Поиск детей")
    );
    assert_eq!(
        call.payload["input"]["questions"][0]["id"].as_str(),
        Some("child_lookup")
    );
    assert_eq!(
        call.payload["input"]["questions"][0]["options"][0]["label"].as_str(),
        Some("Тот же каталог (Recommended)")
    );

    let output_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"timestamp":"2026-04-06T19:21:46.805Z","type":"response_item","payload":{"type":"function_call_output","call_id":"call_4Bs6MMfml19vjtweSvGzdQrW","output":"{\"answers\":{\"child_lookup\":{\"answers\":[\"Тот же каталог (Recommended)\"]}}}"}}"#,
    )
    .expect("json should parse");
    let output = reader
        .parse_subagent_session_payload(
            14,
            &output_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("request_user_input function_call_output should produce event");
    assert_eq!(output.event_type, "user.input.request");
    assert_eq!(output.payload["phase"].as_str(), Some("completed"));
    assert_eq!(
        output.payload["tool_name"].as_str(),
        Some("request_user_input")
    );
    assert_eq!(
        output.payload["output"]["answers"]["child_lookup"]["answers"][0].as_str(),
        Some("Тот же каталог (Recommended)")
    );
}

#[test]
fn subagent_write_stdin_function_call_and_output_normalization() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let call_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"function_call","name":"write_stdin","call_id":"stdin-1","arguments":"{\"session_id\":42,\"chars\":\"ls\\n\"}"}}"#,
    )
    .expect("json should parse");
    let call = reader
        .parse_subagent_session_payload(
            14,
            &call_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("write_stdin function_call should produce event");
    assert_eq!(call.event_type, "stdin.write");
    assert_eq!(call.payload["tool_name"].as_str(), Some("write_stdin"));
    assert_eq!(call.payload["tool_use_id"].as_str(), Some("stdin-1"));
    assert_eq!(call.payload["phase"].as_str(), Some("started"));

    let output_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"function_call_output","call_id":"stdin-1","output":{"stdout":"ok"}}}"#,
    )
    .expect("json should parse");
    let output = reader
        .parse_subagent_session_payload(
            15,
            &output_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("write_stdin function_call_output should produce event");
    assert_eq!(output.event_type, "stdin.write");
    assert_eq!(output.payload["tool_name"].as_str(), Some("write_stdin"));
    assert_eq!(output.payload["tool_use_id"].as_str(), Some("stdin-1"));
    assert_eq!(output.payload["phase"].as_str(), Some("completed"));
    assert_eq!(output.payload["output"]["stdout"].as_str(), Some("ok"));
}

#[test]
fn subagent_mcp_function_calls_are_normalized_as_mcp_events() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let resources_call_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"function_call","name":"list_mcp_resources","call_id":"mcp-1","arguments":"{}"}}"#,
    )
    .expect("json should parse");
    let resources_call = reader
        .parse_subagent_session_payload(
            16,
            &resources_call_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("list_mcp_resources function_call should produce event");
    assert_eq!(resources_call.event_type, "mcp.call");
    assert_eq!(
        resources_call.payload["tool_use_id"].as_str(),
        Some("mcp-1")
    );
    assert_eq!(resources_call.payload["server"].as_str(), Some("codex"));
    assert_eq!(
        resources_call.payload["tool"].as_str(),
        Some("list_mcp_resources")
    );
    assert_eq!(resources_call.payload["arguments"], json!({}));

    let resources_output_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"function_call_output","call_id":"mcp-1","output":"{\"resources\":[]}"}}"#,
    )
    .expect("json should parse");
    let resources_output = reader
        .parse_subagent_session_payload(
            17,
            &resources_output_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("list_mcp_resources function_call_output should produce event");
    assert_eq!(resources_output.event_type, "mcp.result");
    assert_eq!(resources_output.payload["server"].as_str(), Some("codex"));
    assert_eq!(
        resources_output.payload["tool"].as_str(),
        Some("list_mcp_resources")
    );
    assert_eq!(
        resources_output.payload["result"]["resources"]
            .as_array()
            .map(|values| values.len()),
        Some(0)
    );

    let templates_call_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"function_call","name":"list_mcp_resource_templates","call_id":"mcp-2","arguments":"{}"}}"#,
    )
    .expect("json should parse");
    let templates_call = reader
        .parse_subagent_session_payload(
            18,
            &templates_call_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("list_mcp_resource_templates function_call should produce event");
    assert_eq!(templates_call.event_type, "mcp.call");
    assert_eq!(templates_call.payload["server"].as_str(), Some("codex"));
    assert_eq!(
        templates_call.payload["tool"].as_str(),
        Some("list_mcp_resource_templates")
    );

    let templates_output_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"function_call_output","call_id":"mcp-2","output":"{\"resourceTemplates\":[]}"}}"#,
    )
    .expect("json should parse");
    let templates_output = reader
        .parse_subagent_session_payload(
            19,
            &templates_output_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("list_mcp_resource_templates function_call_output should produce event");
    assert_eq!(templates_output.event_type, "mcp.result");
    assert_eq!(templates_output.payload["server"].as_str(), Some("codex"));
    assert_eq!(
        templates_output.payload["tool"].as_str(),
        Some("list_mcp_resource_templates")
    );
    assert_eq!(
        templates_output.payload["result"]["resourceTemplates"]
            .as_array()
            .map(|values| values.len()),
        Some(0)
    );

    let read_call_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"function_call","name":"read_mcp_resource","call_id":"mcp-3","arguments":"{\"server\":\"docs\",\"uri\":\"docs://intro\"}"}}"#,
    )
    .expect("json should parse");
    let read_call = reader
        .parse_subagent_session_payload(
            20,
            &read_call_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("read_mcp_resource function_call should produce event");
    assert_eq!(read_call.event_type, "mcp.call");
    assert_eq!(read_call.payload["server"].as_str(), Some("codex"));
    assert_eq!(
        read_call.payload["tool"].as_str(),
        Some("read_mcp_resource")
    );
    assert_eq!(
        read_call.payload["arguments"]["server"].as_str(),
        Some("docs")
    );
    assert_eq!(
        read_call.payload["arguments"]["uri"].as_str(),
        Some("docs://intro")
    );

    let read_output_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"function_call_output","call_id":"mcp-3","output":"{\"contents\":[{\"text\":\"ok\"}]}"}}"#,
    )
    .expect("json should parse");
    let read_output = reader
        .parse_subagent_session_payload(
            21,
            &read_output_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("read_mcp_resource function_call_output should produce event");
    assert_eq!(read_output.event_type, "mcp.result");
    assert_eq!(read_output.payload["server"].as_str(), Some("codex"));
    assert_eq!(
        read_output.payload["tool"].as_str(),
        Some("read_mcp_resource")
    );
    assert_eq!(
        read_output.payload["result"]["contents"][0]["text"].as_str(),
        Some("ok")
    );
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

    assert_eq!(event.event_type, "collab.spawn_agent");
    assert_eq!(event.payload["tool_name"].as_str(), Some("spawn_agent"));
    assert_eq!(event.payload["phase"].as_str(), Some("started"));
    assert_eq!(subagent_counts.get("spawn_agent").copied(), Some(1));
}

#[test]
fn subagent_spawn_agent_function_call_and_output_extract_rich_fields() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let call_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"function_call","name":"spawn_agent","call_id":"call-spawn","arguments":"{\"agent_type\":\"reviewer\",\"message\":\"review this implementation plan\",\"model\":\"gpt-5.3-codex\",\"reasoning_effort\":\"high\"}"}}"#,
    )
    .expect("json should parse");
    let call = reader
        .parse_subagent_session_payload(
            15,
            &call_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("spawn_agent function_call should produce event");
    assert_eq!(call.event_type, "collab.spawn_agent");
    assert_eq!(
        call.payload["prompt"].as_str(),
        Some("review this implementation plan")
    );
    assert_eq!(
        call.payload["requested_agent_type"].as_str(),
        Some("reviewer")
    );
    assert_eq!(call.payload["model"].as_str(), Some("gpt-5.3-codex"));
    assert_eq!(call.payload["reasoning_effort"].as_str(), Some("high"));

    let output_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"function_call_output","call_id":"call-spawn","output":"{\"agent_id\":\"sub-1\",\"nickname\":\"Halley\"}"}}"#,
    )
    .expect("json should parse");
    let output = reader
        .parse_subagent_session_payload(
            16,
            &output_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("spawn_agent function_call_output should produce event");
    assert_eq!(output.event_type, "collab.spawn_agent");
    assert_eq!(output.payload["new_thread_id"].as_str(), Some("sub-1"));
    assert_eq!(
        output.payload["new_agent_nickname"].as_str(),
        Some("Halley")
    );
    assert_eq!(
        output.payload["receiver_thread_ids"][0].as_str(),
        Some("sub-1")
    );
    assert_eq!(
        output.payload["agents_states"]["sub-1"]["agent_nickname"].as_str(),
        Some("Halley")
    );
}

#[test]
fn subagent_function_call_send_input_is_normalized_as_collab_event() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"function_call","name":"send_input","call_id":"call-send","arguments":"{\"target\":\"sub-1\",\"message\":\"continue\"}"}}"#,
    )
    .expect("json should parse");

    let event = reader
        .parse_subagent_session_payload(
            15,
            &payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("function_call should produce event");

    assert_eq!(event.event_type, "collab.send_input");
    assert_eq!(event.payload["tool_name"].as_str(), Some("send_input"));
    assert_eq!(event.payload["phase"].as_str(), Some("started"));
}

#[test]
fn subagent_response_item_message_and_reasoning_are_normalized() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let message_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"hello"},{"type":"output_text","text":"world"}]}}"#,
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
    assert_eq!(message.event_type, "message.assistant");
    assert_eq!(message.payload["role"].as_str(), Some("assistant"));
    assert_eq!(message.payload["direction"].as_str(), Some("output_text"));
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
    assert_eq!(event.event_type, "message.assistant");
    assert_eq!(event.payload["role"].as_str(), Some("assistant"));
    assert_eq!(event.payload["direction"].as_str(), Some("output_text"));
    assert_eq!(event.payload["phase"].as_str(), Some("completed"));
    assert_eq!(
        event.payload["text_links"]["request_id"].as_str(),
        Some("01234567-89ab-cdef-0123-456789abcdef")
    );

    let meta_message: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":70,"cached_input_tokens":10,"output_tokens":30,"reasoning_output_tokens":13,"total_tokens":123}},"rate_limits":{"primary":{"used_percent":4.0}}}}"#,
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
    assert_eq!(meta.event_type, "info.tokens");
    assert_eq!(meta.payload["input_tokens"].as_i64(), Some(70));
    assert_eq!(meta.payload["cached_input_tokens"].as_i64(), Some(10));
    assert_eq!(meta.payload["output_tokens"].as_i64(), Some(30));
    assert_eq!(meta.payload["reasoning_output_tokens"].as_i64(), Some(13));
    assert_eq!(meta.payload["total_tokens"].as_i64(), Some(123));
    assert_eq!(
        meta.payload["total_token_usage"]["reasoning_output_tokens"].as_i64(),
        Some(13)
    );
    assert!(meta.payload.get("meta_type").is_none());
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
    assert_eq!(complete.event_type, "task.completed");
    assert_eq!(complete.payload["turn_id"].as_str(), Some("turn-1"));
    assert_eq!(
        complete.payload["last_agent_message"].as_str(),
        Some("done")
    );
    assert!(complete.payload.get("raw").is_none());

    let unknown_meta_message: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(r#"{"type":"event_msg","payload":{"type":"custom_meta","x":1}}"#)
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
    assert_eq!(unknown_meta.event_type, "raw.unparsed");
    assert_eq!(
        unknown_meta.payload["reason"].as_str(),
        Some("unsupported event_msg.type=custom_meta")
    );
    assert_eq!(
        unknown_meta.payload["raw"]["payload"]["type"].as_str(),
        Some("custom_meta")
    );
}

#[test]
fn subagent_event_msg_user_message_is_normalized_with_role_and_direction() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let user_message: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"event_msg","payload":{"type":"user_message","message":"возможность просмотра дерева событий по файлу сессии","images":[],"local_images":[],"text_elements":[]}}"#,
    )
    .expect("json should parse");
    let event = reader
        .parse_subagent_session_payload(
            22,
            &user_message,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("event_msg user_message should produce event");
    assert_eq!(event.event_type, "message.user");
    assert_eq!(event.payload["role"].as_str(), Some("user"));
    assert_eq!(event.payload["direction"].as_str(), Some("input_text"));
    assert_eq!(
        event.payload["text"].as_str(),
        Some("возможность просмотра дерева событий по файлу сессии")
    );
}

#[test]
fn subagent_legacy_event_msg_exec_command_end_is_normalized() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let started_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"function_call","name":"exec_command","call_id":"exec-1","arguments":"{\"cmd\":\"pwd\"}"}}"#,
    )
    .expect("json should parse");
    reader
        .parse_subagent_session_payload(
            24,
            &started_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("function_call should produce event");

    let legacy_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"event_msg","payload":{"type":"exec_command_end","call_id":"exec-1","command":["/bin/bash","-lc","pwd"],"cwd":"/repo","status":"completed","aggregated_output":"/repo\n","stderr":"","exit_code":0,"process_id":"123","source":"unified_exec","duration":{"secs":0,"nanos":1},"parsed_cmd":[{"type":"search","cmd":"pwd"}],"formatted_output":"formatted","turn_id":"turn-1"}}"#,
    )
    .expect("json should parse");
    let event = reader
        .parse_subagent_session_payload(
            25,
            &legacy_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("exec_command_end should produce event");

    assert_eq!(event.event_type, "shell.result");
    assert_eq!(event.payload["tool_name"].as_str(), Some("exec_command"));
    assert_eq!(event.payload["tool_use_id"].as_str(), Some("exec-1"));
    assert_eq!(event.payload["phase"].as_str(), Some("completed"));
    assert_eq!(event.payload["status"].as_str(), Some("completed"));
    assert_eq!(event.payload["output"].as_str(), Some("/repo\n"));
    assert_eq!(event.payload["exit_code"].as_i64(), Some(0));
    assert_eq!(
        event.payload["duplicate_of"].as_str(),
        Some("response_item.function_call_output")
    );
}

#[test]
fn subagent_legacy_event_msg_collab_end_events_are_normalized() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let started_payloads = [
        r#"{"type":"response_item","payload":{"type":"function_call","name":"spawn_agent","call_id":"spawn-1","arguments":"{\"goal\":\"review\"}"}}"#,
        r#"{"type":"response_item","payload":{"type":"function_call","name":"wait_agent","call_id":"wait-1","arguments":"{\"targets\":[\"sub-1\"]}"}}"#,
        r#"{"type":"response_item","payload":{"type":"function_call","name":"close_agent","call_id":"close-1","arguments":"{\"target\":\"sub-1\"}"}}"#,
        r#"{"type":"response_item","payload":{"type":"function_call","name":"send_input","call_id":"send-1","arguments":"{\"target\":\"sub-1\",\"message\":\"continue\"}"}}"#,
    ];

    for (index, payload) in started_payloads.iter().enumerate() {
        let parsed: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(payload).expect("json should parse");
        reader
            .parse_subagent_session_payload(
                30 + index as u64,
                &parsed,
                imported,
                "parent-1",
                "thread-1",
                &mut call_names,
                &mut tool_counts,
                &mut subagent_counts,
            )
            .expect("function_call should produce event");
    }

    let spawn_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"event_msg","payload":{"type":"collab_agent_spawn_end","call_id":"spawn-1","sender_thread_id":"thread-1","new_thread_id":"sub-1","new_agent_nickname":"Ada","new_agent_role":"reviewer","prompt":"review this","status":"pending_init","model":"gpt-5","reasoning_effort":"high"}}"#,
    )
    .expect("json should parse");
    let spawn = reader
        .parse_subagent_session_payload(
            40,
            &spawn_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("collab_agent_spawn_end should produce event");
    assert_eq!(spawn.event_type, "collab.spawn_agent");
    assert_eq!(
        spawn.payload["receiver_thread_ids"][0].as_str(),
        Some("sub-1")
    );
    assert_eq!(
        spawn.payload["detection_source"].as_str(),
        Some("legacy_event_msg")
    );
    assert_eq!(spawn.payload["status"].as_str(), Some("pending_init"));

    let wait_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"event_msg","payload":{"type":"collab_waiting_end","call_id":"wait-1","sender_thread_id":"thread-1","agent_statuses":[{"thread_id":"sub-1","agent_nickname":"Ada","agent_role":"reviewer","status":{"completed":"done"}}],"statuses":{"sub-1":{"completed":"done"}}}}"#,
    )
    .expect("json should parse");
    let wait = reader
        .parse_subagent_session_payload(
            41,
            &wait_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("collab_waiting_end should produce event");
    assert_eq!(wait.event_type, "collab.wait");
    assert_eq!(
        wait.payload["receiver_thread_ids"][0].as_str(),
        Some("sub-1")
    );
    assert_eq!(
        wait.payload["agents_states"]["sub-1"]["completed"].as_str(),
        Some("done")
    );
    assert_eq!(wait.payload["status"].as_str(), Some("completed"));

    let close_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"event_msg","payload":{"type":"collab_close_end","call_id":"close-1","sender_thread_id":"thread-1","receiver_thread_id":"sub-1","receiver_agent_nickname":"Ada","receiver_agent_role":"reviewer","status":{"completed":"closed"}}}"#,
    )
    .expect("json should parse");
    let close = reader
        .parse_subagent_session_payload(
            42,
            &close_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("collab_close_end should produce event");
    assert_eq!(close.event_type, "collab.close_agent");
    assert_eq!(
        close.payload["receiver_thread_ids"][0].as_str(),
        Some("sub-1")
    );
    assert_eq!(close.payload["status"].as_str(), Some("completed"));
    assert_eq!(
        close.payload["duplicate_of"].as_str(),
        Some("response_item.function_call_output")
    );

    let send_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"event_msg","payload":{"type":"collab_agent_interaction_end","call_id":"send-1","sender_thread_id":"thread-1","receiver_thread_id":"sub-1","receiver_agent_nickname":"Ada","receiver_agent_role":"reviewer","prompt":"continue","status":{"completed":"accepted"}}}"#,
    )
    .expect("json should parse");
    let send = reader
        .parse_subagent_session_payload(
            43,
            &send_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("collab_agent_interaction_end should produce event");
    assert_eq!(send.event_type, "collab.send_input");
    assert_eq!(
        send.payload["receiver_thread_ids"][0].as_str(),
        Some("sub-1")
    );
    assert_eq!(send.payload["prompt"].as_str(), Some("continue"));
    assert_eq!(send.payload["status"].as_str(), Some("completed"));
}

#[test]
fn subagent_legacy_event_msg_item_completed_and_web_search_end_are_normalized() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let item_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r##"{"type":"event_msg","payload":{"type":"item_completed","thread_id":"thread-1","turn_id":"turn-1","item":{"type":"Plan","id":"turn-1-plan","text":"# Plan\n\n1. Inspect\n2. Patch"}}}"##,
    )
    .expect("json should parse");
    let item_event = reader
        .parse_subagent_session_payload(
            50,
            &item_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("item_completed should produce event");
    assert_eq!(item_event.event_type, "plan.update");
    assert_eq!(
        item_event.payload["tool_name"].as_str(),
        Some("update_plan")
    );
    assert_eq!(
        item_event.payload["tool_use_id"].as_str(),
        Some("turn-1-plan")
    );
    assert_eq!(
        item_event.payload["output"]["text"].as_str(),
        Some("# Plan\n\n1. Inspect\n2. Patch")
    );
    assert_eq!(
        item_event.payload["duplicate_of"].as_str(),
        Some("response_item.message")
    );

    let web_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"event_msg","payload":{"type":"web_search_end","call_id":"web-1","query":"","action":{"type":"other"}}}"#,
    )
    .expect("json should parse");
    let web_event = reader
        .parse_subagent_session_payload(
            51,
            &web_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("web_search_end should produce event");
    assert_eq!(web_event.event_type, "web.search");
    assert_eq!(
        web_event.payload["tool_name"].as_str(),
        Some("web_search_call")
    );
    assert_eq!(web_event.payload["status"].as_str(), Some("completed"));
    assert_eq!(
        web_event.payload["duplicate_of"].as_str(),
        Some("response_item.web_search_call")
    );
}

#[test]
fn subagent_session_example_from_env_has_no_raw_unparsed_when_configured() {
    let Some(path) = std::env::var_os("CODEX_SESSION_EXAMPLE").map(PathBuf::from) else {
        eprintln!("skip: CODEX_SESSION_EXAMPLE is not set");
        return;
    };

    let file = std::fs::File::open(&path).expect("session example should be readable");
    let parsed_lines: Vec<serde_json::Map<String, serde_json::Value>> = BufReader::new(file)
        .lines()
        .enumerate()
        .map(|(index, line)| {
            let line =
                line.unwrap_or_else(|err| panic!("line {} should be readable: {err}", index + 1));
            serde_json::from_str(&line)
                .unwrap_or_else(|err| panic!("line {} should be valid json: {err}", index + 1))
        })
        .collect();

    let Some(first) = parsed_lines.first() else {
        panic!("session example should not be empty");
    };
    assert_eq!(
        first.get("type").and_then(|value| value.as_str()),
        Some("session_meta")
    );
    let thread_id = first
        .get("payload")
        .and_then(|value| value.as_object())
        .and_then(|payload| payload.get("id"))
        .and_then(|value| value.as_str())
        .expect("session_meta.payload.id should be present")
        .to_string();
    let parent_thread_id = first
        .get("payload")
        .and_then(|value| value.as_object())
        .and_then(|payload| payload.get("source"))
        .and_then(|value| value.as_object())
        .and_then(|payload| payload.get("subagent"))
        .and_then(|value| value.as_object())
        .and_then(|payload| payload.get("thread_spawn"))
        .and_then(|value| value.as_object())
        .and_then(|payload| payload.get("parent_thread_id"))
        .and_then(|value| value.as_str())
        .unwrap_or("root-parent")
        .to_string();

    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let mut unparsed = Vec::new();

    for (index, parsed) in parsed_lines.iter().enumerate() {
        let event = reader
            .parse_subagent_session_payload(
                (index + 1) as u64,
                parsed,
                &path,
                &parent_thread_id,
                &thread_id,
                &mut call_names,
                &mut tool_counts,
                &mut subagent_counts,
            )
            .expect("session line should produce event");
        if event.event_type == "raw.unparsed" {
            unparsed.push(format!(
                "line {}: {}",
                index + 1,
                event.payload["reason"].as_str().unwrap_or("unknown reason")
            ));
        }
    }

    assert!(
        unparsed.is_empty(),
        "session example produced raw.unparsed events: {}",
        unparsed.join("; ")
    );
}

#[test]
fn subagent_response_item_commentary_message_is_normalized() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let commentary_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"message","role":"assistant","phase":"commentary","content":[{"text":"thinking aloud"}]}}"#,
    )
    .expect("json should parse");
    let commentary = reader
        .parse_subagent_session_payload(
            14,
            &commentary_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("commentary message should produce event");

    assert_eq!(commentary.event_type, "message.assistant");
    assert_eq!(commentary.payload["phase"].as_str(), Some("commentary"));
    assert_eq!(commentary.payload["text"].as_str(), Some("thinking aloud"));
}

#[test]
fn subagent_event_msg_agent_message_is_deduplicated_after_matching_response_item_message() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let response_item_message: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"message","role":"assistant","phase":"commentary","content":[{"text":"thinking aloud"}]}}"#,
    )
    .expect("json should parse");
    let normalized = reader
        .parse_subagent_session_payload(
            30,
            &response_item_message,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("response_item message should produce event");
    assert_eq!(normalized.event_type, "message.assistant");
    assert_eq!(normalized.payload["text"].as_str(), Some("thinking aloud"));

    let duplicate_event_msg: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"event_msg","payload":{"type":"agent_message","message":"thinking aloud","phase":"commentary"}}"#,
    )
    .expect("json should parse");
    let duplicate = reader.parse_subagent_session_payload(
        31,
        &duplicate_event_msg,
        imported,
        "parent-1",
        "thread-1",
        &mut call_names,
        &mut tool_counts,
        &mut subagent_counts,
    );
    assert!(duplicate.is_none(), "duplicate event_msg should be skipped");
}

#[test]
fn subagent_response_item_message_is_deduplicated_after_matching_event_msg_agent_message() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let event_msg: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"event_msg","payload":{"type":"agent_message","message":"thinking aloud","phase":"commentary"}}"#,
    )
    .expect("json should parse");
    let normalized = reader
        .parse_subagent_session_payload(
            31,
            &event_msg,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("event_msg should produce event");
    assert_eq!(normalized.event_type, "message.assistant");
    assert_eq!(normalized.payload["text"].as_str(), Some("thinking aloud"));

    let duplicate_response_item: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(
            r#"{"type":"response_item","payload":{"type":"message","role":"assistant","phase":"commentary","content":[{"type":"output_text","text":"thinking aloud"}]}}"#,
        )
        .expect("json should parse");
    let duplicate = reader.parse_subagent_session_payload(
        32,
        &duplicate_response_item,
        imported,
        "parent-1",
        "thread-1",
        &mut call_names,
        &mut tool_counts,
        &mut subagent_counts,
    );
    assert!(
        duplicate.is_none(),
        "duplicate response_item should be skipped"
    );
}

#[test]
fn subagent_event_msg_user_message_is_deduplicated_after_matching_response_item_message() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let response_item_message: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"возможность просмотра дерева событий по файлу сессии"}]}}"#,
    )
    .expect("json should parse");
    let normalized = reader
        .parse_subagent_session_payload(
            32,
            &response_item_message,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("response_item user message should produce event");
    assert_eq!(normalized.event_type, "message.user");
    assert_eq!(normalized.payload["role"].as_str(), Some("user"));
    assert_eq!(normalized.payload["direction"].as_str(), Some("input_text"));
    assert_eq!(
        normalized.payload["text"].as_str(),
        Some("возможность просмотра дерева событий по файлу сессии")
    );

    let duplicate_event_msg: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"event_msg","payload":{"type":"user_message","message":"возможность просмотра дерева событий по файлу сессии","images":[],"local_images":[],"text_elements":[]}}"#,
    )
    .expect("json should parse");
    let duplicate = reader.parse_subagent_session_payload(
        33,
        &duplicate_event_msg,
        imported,
        "parent-1",
        "thread-1",
        &mut call_names,
        &mut tool_counts,
        &mut subagent_counts,
    );
    assert!(
        duplicate.is_none(),
        "duplicate user_message should be skipped"
    );
}

#[test]
fn subagent_response_item_user_message_is_deduplicated_after_matching_event_msg_user_message() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let event_msg: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"event_msg","payload":{"type":"user_message","message":"возможность просмотра дерева событий по файлу сессии","images":[],"local_images":[],"text_elements":[]}}"#,
    )
    .expect("json should parse");
    let normalized = reader
        .parse_subagent_session_payload(
            33,
            &event_msg,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("event_msg user_message should produce event");
    assert_eq!(normalized.event_type, "message.user");
    assert_eq!(
        normalized.payload["text"].as_str(),
        Some("возможность просмотра дерева событий по файлу сессии")
    );

    let duplicate_response_item: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(
            r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"возможность просмотра дерева событий по файлу сессии"}]}}"#,
        )
        .expect("json should parse");
    let duplicate = reader.parse_subagent_session_payload(
        34,
        &duplicate_response_item,
        imported,
        "parent-1",
        "thread-1",
        &mut call_names,
        &mut tool_counts,
        &mut subagent_counts,
    );
    assert!(
        duplicate.is_none(),
        "duplicate response_item user message should be skipped"
    );
}

#[test]
fn subagent_messages_with_different_phase_are_not_deduplicated() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let event_msg: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"event_msg","payload":{"type":"agent_message","message":"thinking aloud","phase":"commentary"}}"#,
    )
    .expect("json should parse");
    let normalized = reader
        .parse_subagent_session_payload(
            35,
            &event_msg,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("event_msg should produce event");
    assert_eq!(normalized.event_type, "message.assistant");

    let response_item: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"message","role":"assistant","phase":"completed","content":[{"type":"output_text","text":"thinking aloud"}]}}"#,
    )
    .expect("json should parse");
    let not_duplicate = reader
        .parse_subagent_session_payload(
            36,
            &response_item,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("different phase should keep second message");
    assert_eq!(not_duplicate.event_type, "message.assistant");
    assert_eq!(not_duplicate.payload["phase"].as_str(), Some("completed"));
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
    assert_eq!(event.event_type, "runtime.context");
    assert!(event.payload.get("turn_id").is_none());
    assert_eq!(event.payload["cwd"].as_str(), Some("/workspace"));
    assert_eq!(
        event.payload["sandbox_policy"]["type"].as_str(),
        Some("workspace-write")
    );
    assert_eq!(
        event.payload["collaboration_mode"]["mode"].as_str(),
        Some("default")
    );
}

#[test]
fn subagent_response_item_web_search_call_open_page_is_normalized() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"timestamp":"2026-04-06T15:01:01.125Z","type":"response_item","payload":{"type":"web_search_call","status":"completed","action":{"type":"open_page","url":"https://iana.org/domains/example"}}}"#,
    )
    .expect("json should parse");
    let event = reader
        .parse_subagent_session_payload(
            31,
            &payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("web_search_call should produce event");

    assert_eq!(event.raw_type, "response_item");
    assert_eq!(event.event_type, "web.open");
    assert_eq!(event.payload["tool_name"].as_str(), Some("web_search_call"));
    assert_eq!(event.payload["phase"].as_str(), Some("completed"));
    assert_eq!(event.payload["status"].as_str(), Some("completed"));
    assert_eq!(
        event.payload["output"]["action"]["type"].as_str(),
        Some("open_page")
    );
    assert_eq!(
        event.payload["output"]["action"]["url"].as_str(),
        Some("https://iana.org/domains/example")
    );
}

#[test]
fn subagent_custom_apply_patch_is_normalized_as_patch_apply() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let started_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"custom_tool_call","call_id":"call-1","name":"apply_patch","status":"completed","input":"*** Begin Patch\n*** Update File: src/lib.rs\n*** End Patch\n"}}"#,
    )
    .expect("json should parse");
    let started = reader
        .parse_subagent_session_payload(
            40,
            &started_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("custom_tool_call should produce event");
    assert_eq!(started.event_type, "patch.apply");
    assert_eq!(started.payload["phase"].as_str(), Some("started"));
    assert_eq!(started.payload["tool_use_id"].as_str(), Some("call-1"));

    let completed_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"event_msg","payload":{"type":"patch_apply_end","call_id":"call-1","success":true,"stdout":"Success","stderr":"","changes":{"src/lib.rs":{"type":"update"}}}}"#,
    )
    .expect("json should parse");
    let completed = reader
        .parse_subagent_session_payload(
            41,
            &completed_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("patch_apply_end should produce event");
    assert_eq!(completed.event_type, "patch.apply");
    assert_eq!(completed.payload["phase"].as_str(), Some("completed"));
    assert_eq!(completed.payload["status"].as_str(), Some("completed"));
    assert_eq!(completed.payload["success"].as_bool(), Some(true));
    assert_eq!(
        completed.payload["changes"]["src/lib.rs"]["type"].as_str(),
        Some("update")
    );

    let duplicate_output_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"response_item","payload":{"type":"custom_tool_call_output","call_id":"call-1","output":"Success"}}"#,
    )
    .expect("json should parse");
    let duplicate = reader
        .parse_subagent_session_payload(
            42,
            &duplicate_output_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("duplicate custom_tool_call_output should produce event");
    assert_eq!(duplicate.event_type, "patch.apply.duplicate");
    assert_eq!(duplicate.payload["phase"].as_str(), Some("completed"));
    assert_eq!(
        duplicate.payload["duplicate_of"].as_str(),
        Some("event_msg.patch_apply_end")
    );
}

#[test]
fn compacted_event_emits_follow_up_context_compacted_duplicate() {
    let mut reader = make_reader();
    let mut call_names = HashMap::new();
    let mut tool_counts = HashMap::new();
    let mut subagent_counts = HashMap::new();
    let imported = Path::new("/tmp/subagent.jsonl");

    let compacted_payload: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
        r#"{"type":"compacted","payload":{"message":"trimmed","replacement_history":[{"type":"message"}]}}"#,
    )
    .expect("json should parse");
    let compacted = reader
        .parse_subagent_session_payload(
            50,
            &compacted_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("compacted should produce event");
    assert_eq!(compacted.event_type, "context.compacted");
    assert_eq!(compacted.payload["message"].as_str(), Some("trimmed"));

    let duplicate_payload: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(r#"{"type":"event_msg","payload":{"type":"context_compacted"}}"#)
            .expect("json should parse");
    let duplicate = reader
        .parse_subagent_session_payload(
            51,
            &duplicate_payload,
            imported,
            "parent-1",
            "thread-1",
            &mut call_names,
            &mut tool_counts,
            &mut subagent_counts,
        )
        .expect("duplicate context_compacted should produce event");
    assert_eq!(duplicate.event_type, "context.compacted.duplicate");
    assert_eq!(
        duplicate.payload["duplicate_of"].as_str(),
        Some("compacted")
    );
}
