use super::*;

impl JsonOutputEventReader {
    pub fn parse_subagent_session_payload(
        &mut self,
        seq: u64,
        parsed: &Map<String, Value>,
        imported_path: &Path,
        parent_thread_id: &str,
        thread_id: &str,
        call_names: &mut HashMap<String, String>,
        tool_counts: &mut HashMap<String, u64>,
        subagent_counts: &mut HashMap<String, u64>,
    ) -> Option<CodexEvent> {
        let ts = parsed
            .get("timestamp")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(utc_now_iso);
        let raw_type = parsed
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();

        if raw_type != "event_msg" && raw_type != "compacted" {
            self.state.pending_context_compacted_duplicate = false;
        }

        let mut event_type = "raw.unparsed".to_string();
        let mut payload = payload_to_value(&RawPayload {
            actor_type: Some("subagent".to_string()),
            thread_id: Some(thread_id.to_string()),
            raw: Some(Value::Object(parsed.clone())),
            ..RawPayload::default()
        });
        let mut parse_status = "best_effort".to_string();

        match raw_type.as_str() {
            "session_meta" => {
                let Some(meta) = parsed.get("payload").and_then(Value::as_object) else {
                    return Some(self.build_subagent_event(
                        seq,
                        &ts,
                        event_type,
                        raw_type,
                        parse_status,
                        payload,
                    ));
                };
                let meta_thread_id = meta
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or(thread_id)
                    .to_string();
                if meta_thread_id != thread_id {
                    return None;
                }
                let session_meta =
                    imported_subagent_session_meta(parsed, thread_id).unwrap_or_default();

                event_type = "agent.session".to_string();
                parse_status = "parsed".to_string();
                payload = Value::Object(Map::from_iter([
                    ("actor_type".to_string(), Value::from("subagent")),
                    ("thread_id".to_string(), Value::from(meta_thread_id)),
                    (
                        "parent_thread_id".to_string(),
                        Value::from(parent_thread_id.to_string()),
                    ),
                    (
                        "forked_from_id".to_string(),
                        session_meta
                            .forked_from_id
                            .map(Value::from)
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "cwd".to_string(),
                        meta.get("cwd").cloned().unwrap_or(Value::Null),
                    ),
                    (
                        "agent_nickname".to_string(),
                        meta.get("agent_nickname").cloned().unwrap_or(Value::Null),
                    ),
                    (
                        "agent_role".to_string(),
                        meta.get("agent_role").cloned().unwrap_or(Value::Null),
                    ),
                    (
                        "session_path".to_string(),
                        Value::from(imported_path.display().to_string()),
                    ),
                ]));
            }
            "response_item" => {
                let Some(item) = parsed.get("payload").and_then(Value::as_object) else {
                    return Some(self.build_subagent_event(
                        seq,
                        &ts,
                        event_type,
                        raw_type,
                        parse_status,
                        payload,
                    ));
                };
                let item_type = item
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                match item_type.as_str() {
                    "function_call" => {
                        let name = item
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown")
                            .to_string();
                        let call_id = item
                            .get("call_id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        if !call_id.is_empty() {
                            call_names.insert(call_id.clone(), name.clone());
                        }
                        increment(tool_counts, &name);
                        if is_subagent_tool(&name) {
                            increment(subagent_counts, &name);
                        }

                        let parsed_arguments = item
                            .get("arguments")
                            .and_then(Value::as_str)
                            .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
                            .or_else(|| item.get("arguments").cloned())
                            .unwrap_or(Value::Null);

                        if let Some(server) = codex_mcp_server(&name) {
                            event_type = MCP_CALL.to_string();
                            parse_status = "parsed".to_string();
                            payload = payload_to_value(&McpCallPayload {
                                actor_type: Some("subagent".to_string()),
                                thread_id: Some(thread_id.to_string()),
                                tool_use_id: Some(call_id),
                                phase: Some("started".to_string()),
                                arguments: Some(parsed_arguments),
                                server: Some(server.to_string()),
                                tool: Some(name),
                                ..McpCallPayload::default()
                            });
                        } else {
                            event_type = normalized_tool_event_type(&name, false);
                            parse_status = "parsed".to_string();
                            payload = if is_singleton_tool_event_type(&event_type) {
                                payload_to_value(&ToolResultPayload {
                                    actor_type: Some("subagent".to_string()),
                                    thread_id: Some(thread_id.to_string()),
                                    parent_thread_id: Some(parent_thread_id.to_string()),
                                    session_path: Some(imported_path.display().to_string()),
                                    tool_name: name,
                                    tool_use_id: Some(call_id),
                                    phase: Some("started".to_string()),
                                    input: Some(parsed_arguments),
                                    ..ToolResultPayload::default()
                                })
                            } else {
                                Value::Object(Map::from_iter([
                                    ("actor_type".to_string(), Value::from("subagent")),
                                    ("thread_id".to_string(), Value::from(thread_id)),
                                    (
                                        "parent_thread_id".to_string(),
                                        Value::from(parent_thread_id),
                                    ),
                                    ("tool_name".to_string(), Value::from(name)),
                                    ("tool_use_id".to_string(), Value::from(call_id)),
                                    ("input".to_string(), parsed_arguments),
                                    (
                                        "session_path".to_string(),
                                        Value::from(imported_path.display().to_string()),
                                    ),
                                ]))
                            };
                        }
                    }
                    "function_call_output" => {
                        let call_id = item
                            .get("call_id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        let name = call_names
                            .get(&call_id)
                            .cloned()
                            .unwrap_or_else(|| "unknown".to_string());
                        increment(tool_counts, &name);
                        if let Some(server) = codex_mcp_server(&name) {
                            event_type = MCP_RESULT.to_string();
                            parse_status = "parsed".to_string();
                            payload = payload_to_value(&McpResultPayload {
                                actor_type: Some("subagent".to_string()),
                                thread_id: Some(thread_id.to_string()),
                                tool_use_id: Some(call_id),
                                phase: Some("completed".to_string()),
                                status: Some("completed".to_string()),
                                result: Some(parse_embedded_json_value(item.get("output"))),
                                server: Some(server.to_string()),
                                tool: Some(name),
                                ..McpResultPayload::default()
                            });
                        } else {
                            event_type = normalized_tool_event_type(&name, true);
                            parse_status = "parsed".to_string();
                            payload = if is_singleton_tool_event_type(&event_type) {
                                payload_to_value(&ToolResultPayload {
                                    actor_type: Some("subagent".to_string()),
                                    thread_id: Some(thread_id.to_string()),
                                    parent_thread_id: Some(parent_thread_id.to_string()),
                                    session_path: Some(imported_path.display().to_string()),
                                    tool_name: name,
                                    tool_use_id: Some(call_id),
                                    phase: Some("completed".to_string()),
                                    output: item.get("output").cloned(),
                                    ..ToolResultPayload::default()
                                })
                            } else {
                                Value::Object(Map::from_iter([
                                    ("actor_type".to_string(), Value::from("subagent")),
                                    ("thread_id".to_string(), Value::from(thread_id)),
                                    (
                                        "parent_thread_id".to_string(),
                                        Value::from(parent_thread_id),
                                    ),
                                    ("tool_name".to_string(), Value::from(name)),
                                    ("tool_use_id".to_string(), Value::from(call_id)),
                                    (
                                        "output".to_string(),
                                        item.get("output").cloned().unwrap_or(Value::Null),
                                    ),
                                    (
                                        "session_path".to_string(),
                                        Value::from(imported_path.display().to_string()),
                                    ),
                                ]))
                            };
                        }
                    }
                    "custom_tool_call" => {
                        let name = item
                            .get("name")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown")
                            .to_string();
                        let call_id = item
                            .get("call_id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        if !call_id.is_empty() {
                            call_names.insert(call_id.clone(), name.clone());
                        }
                        increment(tool_counts, &name);
                        if name == "apply_patch" {
                            event_type = PATCH_APPLY.to_string();
                            parse_status = "parsed".to_string();
                            payload = payload_to_value(&ToolResultPayload {
                                actor_type: Some("subagent".to_string()),
                                thread_id: Some(thread_id.to_string()),
                                parent_thread_id: Some(parent_thread_id.to_string()),
                                session_path: Some(imported_path.display().to_string()),
                                tool_name: name,
                                tool_use_id: Some(call_id),
                                phase: Some("started".to_string()),
                                status: item
                                    .get("status")
                                    .and_then(Value::as_str)
                                    .map(str::to_string),
                                input: item.get("input").cloned(),
                                ..ToolResultPayload::default()
                            });
                        } else {
                            event_type = TOOL_CALL.to_string();
                            parse_status = "parsed".to_string();
                            payload = payload_to_value(&ToolCallPayload {
                                actor_type: Some("subagent".to_string()),
                                thread_id: Some(thread_id.to_string()),
                                parent_thread_id: Some(parent_thread_id.to_string()),
                                session_path: Some(imported_path.display().to_string()),
                                tool_name: name,
                                tool_use_id: Some(call_id),
                                status: item
                                    .get("status")
                                    .and_then(Value::as_str)
                                    .map(str::to_string),
                                input: item.get("input").cloned(),
                                ..ToolCallPayload::default()
                            });
                        }
                    }
                    "custom_tool_call_output" => {
                        let call_id = item
                            .get("call_id")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string();
                        let name = call_names
                            .get(&call_id)
                            .cloned()
                            .unwrap_or_else(|| "unknown".to_string());
                        increment(tool_counts, &name);
                        if name == "apply_patch" {
                            if self.state.patch_apply_end_call_ids.contains(&call_id) {
                                return None;
                            }
                            event_type = PATCH_APPLY.to_string();
                            parse_status = "parsed".to_string();
                            payload = payload_to_value(&ToolResultPayload {
                                actor_type: Some("subagent".to_string()),
                                thread_id: Some(thread_id.to_string()),
                                parent_thread_id: Some(parent_thread_id.to_string()),
                                session_path: Some(imported_path.display().to_string()),
                                tool_name: name,
                                tool_use_id: Some(call_id),
                                phase: Some("completed".to_string()),
                                status: custom_tool_output_status(item.get("output")),
                                output: item.get("output").cloned(),
                                ..ToolResultPayload::default()
                            });
                        } else {
                            event_type = TOOL_RESULT.to_string();
                            parse_status = "parsed".to_string();
                            payload = payload_to_value(&ToolResultPayload {
                                actor_type: Some("subagent".to_string()),
                                thread_id: Some(thread_id.to_string()),
                                parent_thread_id: Some(parent_thread_id.to_string()),
                                session_path: Some(imported_path.display().to_string()),
                                tool_name: name,
                                tool_use_id: Some(call_id),
                                phase: Some("completed".to_string()),
                                output: item.get("output").cloned(),
                                ..ToolResultPayload::default()
                            });
                        }
                    }
                    "message" => {
                        let text = self.extract_message_text(item);
                        event_type =
                            if item.get("phase").and_then(Value::as_str) == Some("commentary") {
                                MESSAGE_COMMENTARY.to_string()
                            } else {
                                MESSAGE_AGENT.to_string()
                            };
                        parse_status = "parsed".to_string();
                        payload = Value::Object(Map::from_iter([
                            ("actor_type".to_string(), Value::from("subagent")),
                            ("thread_id".to_string(), Value::from(thread_id)),
                            (
                                "parent_thread_id".to_string(),
                                Value::from(parent_thread_id),
                            ),
                            (
                                "role".to_string(),
                                item.get("role").cloned().unwrap_or(Value::Null),
                            ),
                            (
                                "phase".to_string(),
                                item.get("phase").cloned().unwrap_or(Value::Null),
                            ),
                            ("text".to_string(), Value::from(text.clone())),
                            (
                                "text_links".to_string(),
                                self.extract_text_links(&text)
                                    .map(|v| payload_to_value(&v))
                                    .unwrap_or(Value::Null),
                            ),
                            (
                                "session_path".to_string(),
                                Value::from(imported_path.display().to_string()),
                            ),
                        ]));
                    }
                    "reasoning" => {
                        let text = self.extract_reasoning_text(item);
                        event_type = "agent.reasoning".to_string();
                        parse_status = "parsed".to_string();
                        payload = Value::Object(Map::from_iter([
                            ("actor_type".to_string(), Value::from("subagent")),
                            ("thread_id".to_string(), Value::from(thread_id)),
                            (
                                "parent_thread_id".to_string(),
                                Value::from(parent_thread_id),
                            ),
                            (
                                "role".to_string(),
                                item.get("role").cloned().unwrap_or(Value::Null),
                            ),
                            (
                                "phase".to_string(),
                                item.get("phase").cloned().unwrap_or(Value::Null),
                            ),
                            ("text".to_string(), Value::from(text.clone())),
                            (
                                "text_links".to_string(),
                                self.extract_text_links(&text)
                                    .map(|v| payload_to_value(&v))
                                    .unwrap_or(Value::Null),
                            ),
                            (
                                "session_path".to_string(),
                                Value::from(imported_path.display().to_string()),
                            ),
                        ]));
                    }
                    _ => {}
                }
            }
            "compacted" => {
                self.state.pending_context_compacted_duplicate = true;
                event_type = CONTEXT_COMPACTED.to_string();
                parse_status = "parsed".to_string();
                let compacted = parsed.get("payload").and_then(Value::as_object);
                payload = Value::Object(Map::from_iter([
                    ("actor_type".to_string(), Value::from("subagent")),
                    ("thread_id".to_string(), Value::from(thread_id)),
                    (
                        "parent_thread_id".to_string(),
                        Value::from(parent_thread_id),
                    ),
                    (
                        "session_path".to_string(),
                        Value::from(imported_path.display().to_string()),
                    ),
                    (
                        "message".to_string(),
                        compacted
                            .and_then(|ctx| ctx.get("message"))
                            .cloned()
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "replacement_history".to_string(),
                        compacted
                            .and_then(|ctx| ctx.get("replacement_history"))
                            .cloned()
                            .unwrap_or(Value::Null),
                    ),
                ]));
            }
            "event_msg" => {
                let Some(item) = parsed.get("payload").and_then(Value::as_object) else {
                    return Some(self.build_subagent_event(
                        seq,
                        &ts,
                        event_type,
                        raw_type,
                        parse_status,
                        payload,
                    ));
                };
                let msg_type = item
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                if msg_type != "context_compacted" {
                    self.state.pending_context_compacted_duplicate = false;
                }
                if msg_type == "agent_message" {
                    let text = item
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    event_type = MESSAGE_COMMENTARY.to_string();
                    parse_status = "parsed".to_string();
                    payload = Value::Object(Map::from_iter([
                        ("actor_type".to_string(), Value::from("subagent")),
                        ("thread_id".to_string(), Value::from(thread_id)),
                        (
                            "parent_thread_id".to_string(),
                            Value::from(parent_thread_id),
                        ),
                        ("text".to_string(), Value::from(text.clone())),
                        (
                            "phase".to_string(),
                            item.get("phase").cloned().unwrap_or(Value::Null),
                        ),
                        (
                            "text_links".to_string(),
                            self.extract_text_links(&text)
                                .map(|v| payload_to_value(&v))
                                .unwrap_or(Value::Null),
                        ),
                        (
                            "session_path".to_string(),
                            Value::from(imported_path.display().to_string()),
                        ),
                    ]));
                } else if msg_type == "token_count" {
                    parse_status = "parsed".to_string();
                    let total_token_usage = item
                        .get("info")
                        .and_then(Value::as_object)
                        .and_then(|v| v.get("total_token_usage"))
                        .cloned();
                    let total_token_usage_obj =
                        total_token_usage.as_ref().and_then(Value::as_object);
                    event_type = INFO_TOKENS.to_string();
                    payload = payload_to_value(&InfoTokensPayload {
                        actor_type: Some("subagent".to_string()),
                        thread_id: Some(thread_id.to_string()),
                        parent_thread_id: Some(parent_thread_id.to_string()),
                        session_path: Some(imported_path.display().to_string()),
                        input_tokens: total_token_usage_obj
                            .and_then(|v| v.get("input_tokens"))
                            .and_then(Value::as_u64),
                        cached_input_tokens: total_token_usage_obj
                            .and_then(|v| v.get("cached_input_tokens"))
                            .and_then(Value::as_u64),
                        output_tokens: total_token_usage_obj
                            .and_then(|v| v.get("output_tokens"))
                            .and_then(Value::as_u64),
                        reasoning_output_tokens: total_token_usage_obj
                            .and_then(|v| v.get("reasoning_output_tokens"))
                            .and_then(Value::as_u64),
                        total_tokens: total_token_usage_obj
                            .and_then(|v| v.get("total_tokens"))
                            .and_then(Value::as_u64),
                        total_token_usage,
                        rate_limits: item.get("rate_limits").cloned(),
                    });
                } else {
                    parse_status = "parsed".to_string();
                    match msg_type.as_str() {
                        "task_started" => {
                            event_type = TASK_STARTED.to_string();
                            payload = Value::Object(Map::from_iter([
                                ("actor_type".to_string(), Value::from("subagent")),
                                ("thread_id".to_string(), Value::from(thread_id)),
                                (
                                    "parent_thread_id".to_string(),
                                    Value::from(parent_thread_id),
                                ),
                                (
                                    "session_path".to_string(),
                                    Value::from(imported_path.display().to_string()),
                                ),
                                (
                                    "turn_id".to_string(),
                                    item.get("turn_id").cloned().unwrap_or(Value::Null),
                                ),
                                (
                                    "model_context_window".to_string(),
                                    item.get("model_context_window")
                                        .cloned()
                                        .unwrap_or(Value::Null),
                                ),
                                (
                                    "collaboration_mode_kind".to_string(),
                                    item.get("collaboration_mode_kind")
                                        .cloned()
                                        .unwrap_or(Value::Null),
                                ),
                            ]));
                        }
                        "task_complete" => {
                            event_type = TASK_COMPLETED.to_string();
                            payload = Value::Object(Map::from_iter([
                                ("actor_type".to_string(), Value::from("subagent")),
                                ("thread_id".to_string(), Value::from(thread_id)),
                                (
                                    "parent_thread_id".to_string(),
                                    Value::from(parent_thread_id),
                                ),
                                (
                                    "session_path".to_string(),
                                    Value::from(imported_path.display().to_string()),
                                ),
                                (
                                    "turn_id".to_string(),
                                    item.get("turn_id").cloned().unwrap_or(Value::Null),
                                ),
                                (
                                    "last_agent_message".to_string(),
                                    item.get("last_agent_message")
                                        .cloned()
                                        .unwrap_or(Value::Null),
                                ),
                            ]));
                        }
                        "user_message" => {
                            event_type = MESSAGE_USER.to_string();
                            payload = Value::Object(Map::from_iter([
                                ("actor_type".to_string(), Value::from("subagent")),
                                ("thread_id".to_string(), Value::from(thread_id)),
                                (
                                    "parent_thread_id".to_string(),
                                    Value::from(parent_thread_id),
                                ),
                                (
                                    "session_path".to_string(),
                                    Value::from(imported_path.display().to_string()),
                                ),
                                (
                                    "text".to_string(),
                                    item.get("message").cloned().unwrap_or(Value::Null),
                                ),
                                (
                                    "images_count".to_string(),
                                    Value::from(
                                        item.get("images")
                                            .and_then(Value::as_array)
                                            .map(|v| v.len() as u64)
                                            .unwrap_or(0),
                                    ),
                                ),
                                (
                                    "local_images_count".to_string(),
                                    Value::from(
                                        item.get("local_images")
                                            .and_then(Value::as_array)
                                            .map(|v| v.len() as u64)
                                            .unwrap_or(0),
                                    ),
                                ),
                                (
                                    "text_elements_count".to_string(),
                                    Value::from(
                                        item.get("text_elements")
                                            .and_then(Value::as_array)
                                            .map(|v| v.len() as u64)
                                            .unwrap_or(0),
                                    ),
                                ),
                            ]));
                        }
                        "context_compacted" => {
                            if self.state.pending_context_compacted_duplicate {
                                self.state.pending_context_compacted_duplicate = false;
                                return None;
                            }
                            event_type = CONTEXT_COMPACTED.to_string();
                            payload = Value::Object(Map::from_iter([
                                ("actor_type".to_string(), Value::from("subagent")),
                                ("thread_id".to_string(), Value::from(thread_id)),
                                (
                                    "parent_thread_id".to_string(),
                                    Value::from(parent_thread_id),
                                ),
                                (
                                    "session_path".to_string(),
                                    Value::from(imported_path.display().to_string()),
                                ),
                            ]));
                        }
                        "turn_aborted" => {
                            event_type = AGENT_ABORTED.to_string();
                            payload = Value::Object(Map::from_iter([
                                ("actor_type".to_string(), Value::from("subagent")),
                                ("thread_id".to_string(), Value::from(thread_id)),
                                (
                                    "parent_thread_id".to_string(),
                                    Value::from(parent_thread_id),
                                ),
                                (
                                    "session_path".to_string(),
                                    Value::from(imported_path.display().to_string()),
                                ),
                                (
                                    "turn_id".to_string(),
                                    item.get("turn_id").cloned().unwrap_or(Value::Null),
                                ),
                                (
                                    "reason".to_string(),
                                    item.get("reason").cloned().unwrap_or(Value::Null),
                                ),
                            ]));
                        }
                        "patch_apply_end" => {
                            let call_id = item
                                .get("call_id")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            if !call_id.is_empty() {
                                self.state.patch_apply_end_call_ids.insert(call_id.clone());
                            }
                            event_type = PATCH_APPLY.to_string();
                            payload = payload_to_value(&ToolResultPayload {
                                actor_type: Some("subagent".to_string()),
                                thread_id: Some(thread_id.to_string()),
                                parent_thread_id: Some(parent_thread_id.to_string()),
                                session_path: Some(imported_path.display().to_string()),
                                tool_name: "apply_patch".to_string(),
                                tool_use_id: (!call_id.is_empty()).then_some(call_id),
                                phase: Some("completed".to_string()),
                                status: item.get("success").and_then(Value::as_bool).map(
                                    |success| {
                                        if success { "completed" } else { "failed" }.to_string()
                                    },
                                ),
                                output: item.get("stdout").cloned(),
                                stderr: item.get("stderr").cloned(),
                                ..ToolResultPayload::default()
                            });
                            if let Some(obj) = payload.as_object_mut() {
                                obj.insert(
                                    "success".to_string(),
                                    item.get("success").cloned().unwrap_or(Value::Null),
                                );
                                obj.insert(
                                    "changes".to_string(),
                                    item.get("changes").cloned().unwrap_or(Value::Null),
                                );
                            }
                        }
                        _ => {
                            event_type = "agent.meta".to_string();
                            let mut payload_map = Map::from_iter([
                                ("actor_type".to_string(), Value::from("subagent")),
                                ("thread_id".to_string(), Value::from(thread_id)),
                                (
                                    "parent_thread_id".to_string(),
                                    Value::from(parent_thread_id),
                                ),
                                ("meta_type".to_string(), Value::from(msg_type.clone())),
                                (
                                    "session_path".to_string(),
                                    Value::from(imported_path.display().to_string()),
                                ),
                            ]);
                            payload_map.insert("raw".to_string(), Value::Object(item.clone()));
                            payload = Value::Object(payload_map);
                        }
                    }
                }
            }
            "turn_context" => {
                event_type = RUNTIME_CONTEXT.to_string();
                parse_status = "parsed".to_string();
                let context = parsed.get("payload").and_then(Value::as_object);
                let sandbox_policy_type = context
                    .and_then(|ctx| ctx.get("sandbox_policy"))
                    .and_then(Value::as_object)
                    .and_then(|v| v.get("type"))
                    .cloned()
                    .unwrap_or(Value::Null);
                let collaboration_mode_kind = context
                    .and_then(|ctx| ctx.get("collaboration_mode"))
                    .and_then(Value::as_object)
                    .and_then(|v| v.get("mode"))
                    .cloned()
                    .unwrap_or(Value::Null);

                payload = Value::Object(Map::from_iter([
                    ("actor_type".to_string(), Value::from("subagent")),
                    ("thread_id".to_string(), Value::from(thread_id)),
                    (
                        "parent_thread_id".to_string(),
                        Value::from(parent_thread_id),
                    ),
                    (
                        "session_path".to_string(),
                        Value::from(imported_path.display().to_string()),
                    ),
                    (
                        "turn_id".to_string(),
                        context
                            .and_then(|ctx| ctx.get("turn_id"))
                            .cloned()
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "cwd".to_string(),
                        context
                            .and_then(|ctx| ctx.get("cwd"))
                            .cloned()
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "current_date".to_string(),
                        context
                            .and_then(|ctx| ctx.get("current_date"))
                            .cloned()
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "timezone".to_string(),
                        context
                            .and_then(|ctx| ctx.get("timezone"))
                            .cloned()
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "approval_policy".to_string(),
                        context
                            .and_then(|ctx| ctx.get("approval_policy"))
                            .cloned()
                            .unwrap_or(Value::Null),
                    ),
                    ("sandbox_policy_type".to_string(), sandbox_policy_type),
                    (
                        "model".to_string(),
                        context
                            .and_then(|ctx| ctx.get("model"))
                            .cloned()
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "effort".to_string(),
                        context
                            .and_then(|ctx| ctx.get("effort"))
                            .cloned()
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "summary".to_string(),
                        context
                            .and_then(|ctx| ctx.get("summary"))
                            .cloned()
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "collaboration_mode_kind".to_string(),
                        collaboration_mode_kind,
                    ),
                ]));
            }
            _ => {}
        }

        Some(self.build_subagent_event(seq, &ts, event_type, raw_type, parse_status, payload))
    }

    fn build_subagent_event(
        &self,
        seq: u64,
        ts: &str,
        event_type: String,
        raw_type: String,
        parse_status: String,
        payload: Value,
    ) -> CodexEvent {
        CodexEvent {
            schema_version: 1,
            ts: ts.to_string(),
            task_id: self.context.task_id.clone(),
            run_id: self.context.run_id.clone(),
            seq,
            event_type,
            raw_type,
            parse_status,
            payload,
            source: "subagent_session".to_string(),
        }
    }

    fn extract_message_text(&self, item: &Map<String, Value>) -> String {
        let Some(content) = item.get("content").and_then(Value::as_array) else {
            return String::new();
        };
        let mut parts: Vec<String> = Vec::new();
        for chunk in content {
            if let Some(chunk_obj) = chunk.as_object() {
                if let Some(text) = chunk_obj.get("text").and_then(Value::as_str) {
                    if !text.is_empty() {
                        parts.push(text.to_string());
                    }
                }
            }
        }
        parts.join("\n").trim().to_string()
    }

    fn extract_reasoning_text(&self, item: &Map<String, Value>) -> String {
        let Some(summary) = item.get("summary").and_then(Value::as_array) else {
            return String::new();
        };
        let mut parts: Vec<String> = Vec::new();
        for entry in summary {
            if let Some(entry_obj) = entry.as_object() {
                if let Some(text) = entry_obj.get("text").and_then(Value::as_str) {
                    if !text.is_empty() {
                        parts.push(text.to_string());
                    }
                }
            }
        }
        parts.join("\n").trim().to_string()
    }
}

pub(crate) fn imported_subagent_session_meta(
    parsed: &Map<String, Value>,
    thread_id: &str,
) -> Option<ImportedSubagentSessionMeta> {
    if parsed.get("type").and_then(Value::as_str) != Some("session_meta") {
        return None;
    }

    let payload = parsed.get("payload").and_then(Value::as_object)?;
    let meta_thread_id = payload
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or(thread_id);
    if meta_thread_id != thread_id {
        return None;
    }

    Some(ImportedSubagentSessionMeta {
        parent_thread_id: payload
            .get("source")
            .and_then(Value::as_object)
            .and_then(|source| source.get("subagent"))
            .and_then(Value::as_object)
            .and_then(|subagent| subagent.get("thread_spawn"))
            .and_then(Value::as_object)
            .and_then(|spawn| spawn.get("parent_thread_id"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        forked_from_id: payload
            .get("forked_from_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    })
}
