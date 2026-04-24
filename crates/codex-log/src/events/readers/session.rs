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
        let mut payload: Value;
        let mut parse_status = "best_effort".to_string();
        let mut current_subagent_message: Option<RecentSubagentMessage> = None;

        match raw_type.as_str() {
            "session_meta" => {
                let Some(meta) = parsed.get("payload").and_then(Value::as_object) else {
                    payload = subagent_raw_payload(
                        thread_id,
                        parent_thread_id,
                        imported_path,
                        Value::Object(parsed.clone()),
                        Some("missing session_meta payload object".to_string()),
                    );
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
                    event_type = AGENT_SESSION_FOREIGN.to_string();
                    parse_status = "parsed".to_string();
                    let mut payload_map =
                        subagent_base_payload_map(thread_id, parent_thread_id, imported_path);
                    payload_map
                        .insert("foreign_thread_id".to_string(), Value::from(meta_thread_id));
                    payload_map.insert(
                        "forked_from_id".to_string(),
                        meta.get("forked_from_id").cloned().unwrap_or(Value::Null),
                    );
                    payload_map.insert(
                        "cwd".to_string(),
                        meta.get("cwd").cloned().unwrap_or(Value::Null),
                    );
                    payload_map.insert(
                        "agent_nickname".to_string(),
                        meta.get("agent_nickname").cloned().unwrap_or(Value::Null),
                    );
                    payload_map.insert(
                        "agent_role".to_string(),
                        meta.get("agent_role").cloned().unwrap_or(Value::Null),
                    );
                    payload_map.insert("raw".to_string(), Value::Object(parsed.clone()));
                    payload = Value::Object(payload_map);
                    return Some(self.build_subagent_event(
                        seq,
                        &ts,
                        event_type,
                        raw_type,
                        parse_status,
                        payload,
                    ));
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
                    payload = subagent_raw_payload(
                        thread_id,
                        parent_thread_id,
                        imported_path,
                        Value::Object(parsed.clone()),
                        Some("missing response_item payload object".to_string()),
                    );
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
                        let is_spawn_agent = name == "spawn_agent";
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
                                    tool_name: name.clone(),
                                    tool_use_id: Some(call_id),
                                    phase: Some("started".to_string()),
                                    input: Some(parsed_arguments.clone()),
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
                                    ("tool_name".to_string(), Value::from(name.clone())),
                                    ("tool_use_id".to_string(), Value::from(call_id)),
                                    ("input".to_string(), parsed_arguments.clone()),
                                    (
                                        "session_path".to_string(),
                                        Value::from(imported_path.display().to_string()),
                                    ),
                                ]))
                            };
                            if is_shell_tool(&name) {
                                enrich_skill_usage_payload(&mut payload);
                            }
                            if is_spawn_agent {
                                enrich_spawn_agent_started_payload(&mut payload, &parsed_arguments);
                            }
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
                        let is_spawn_agent = name == "spawn_agent";
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
                            let parsed_output = parse_embedded_json_value(item.get("output"));
                            payload = if is_singleton_tool_event_type(&event_type) {
                                payload_to_value(&ToolResultPayload {
                                    actor_type: Some("subagent".to_string()),
                                    thread_id: Some(thread_id.to_string()),
                                    parent_thread_id: Some(parent_thread_id.to_string()),
                                    session_path: Some(imported_path.display().to_string()),
                                    tool_name: name.clone(),
                                    tool_use_id: Some(call_id),
                                    phase: Some("completed".to_string()),
                                    output: Some(parsed_output.clone()),
                                    ..ToolResultPayload::default()
                                })
                            } else {
                                payload_to_value(&ToolResultPayload {
                                    actor_type: Some("subagent".to_string()),
                                    thread_id: Some(thread_id.to_string()),
                                    parent_thread_id: Some(parent_thread_id.to_string()),
                                    session_path: Some(imported_path.display().to_string()),
                                    tool_name: name.clone(),
                                    tool_use_id: Some(call_id.clone()),
                                    status: Some("completed".to_string()),
                                    phase: Some("completed".to_string()),
                                    output: Some(parsed_output.clone()),
                                    ..ToolResultPayload::default()
                                })
                            };
                            if is_shell_tool(&name) {
                                enrich_response_item_shell_result_payload(
                                    &mut payload,
                                    item.get("output"),
                                    &parsed_output,
                                );
                                enrich_skill_usage_payload(&mut payload);
                            }
                            if is_spawn_agent {
                                enrich_spawn_agent_completed_payload(&mut payload, &parsed_output);
                            }
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
                                event_type = PATCH_APPLY_DUPLICATE.to_string();
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
                                if let Some(obj) = payload.as_object_mut() {
                                    obj.insert(
                                        "duplicate_of".to_string(),
                                        Value::from("event_msg.patch_apply_end"),
                                    );
                                }
                                return Some(self.build_subagent_event(
                                    seq,
                                    &ts,
                                    event_type,
                                    raw_type,
                                    parse_status,
                                    payload,
                                ));
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
                    "web_search_call" => {
                        increment(tool_counts, "web_search_call");
                        let phase = response_item_phase(item);
                        let query_action = Value::Object(Map::from_iter([
                            (
                                "query".to_string(),
                                item.get("query").cloned().unwrap_or(Value::Null),
                            ),
                            (
                                "action".to_string(),
                                item.get("action").cloned().unwrap_or(Value::Null),
                            ),
                        ]));
                        event_type = if is_open_page_action(item.get("action")) {
                            WEB_OPEN.to_string()
                        } else {
                            WEB_SEARCH.to_string()
                        };
                        parse_status = "parsed".to_string();
                        payload = payload_to_value(&ToolResultPayload {
                            actor_type: Some("subagent".to_string()),
                            thread_id: Some(thread_id.to_string()),
                            parent_thread_id: Some(parent_thread_id.to_string()),
                            session_path: Some(imported_path.display().to_string()),
                            tool_name: "web_search_call".to_string(),
                            tool_use_id: item.get("id").and_then(Value::as_str).map(str::to_string),
                            phase: Some(phase.clone()),
                            status: item
                                .get("status")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            input: (phase != "completed").then_some(query_action.clone()),
                            output: (phase == "completed").then_some(query_action),
                            ..ToolResultPayload::default()
                        });
                    }
                    "message" => {
                        let text = self.extract_message_text(item);
                        let direction = self.extract_message_direction(item);
                        let phase = item
                            .get("phase")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .filter(|value| !value.is_empty())
                            .map(str::to_string);
                        let role = normalized_subagent_message_role(
                            item.get("role").and_then(Value::as_str),
                        );
                        event_type = subagent_message_event_type(&role);
                        parse_status = "parsed".to_string();
                        payload = Value::Object(Map::from_iter([
                            ("actor_type".to_string(), Value::from("subagent")),
                            ("thread_id".to_string(), Value::from(thread_id)),
                            (
                                "parent_thread_id".to_string(),
                                Value::from(parent_thread_id),
                            ),
                            ("role".to_string(), Value::from(role.clone())),
                            (
                                "direction".to_string(),
                                direction
                                    .as_ref()
                                    .map(|value| Value::from(value.clone()))
                                    .unwrap_or(Value::Null),
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
                        current_subagent_message = Some(RecentSubagentMessage {
                            role,
                            phase,
                            text,
                            source: RecentSubagentMessageSource::ResponseItemMessage,
                        });
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
                    _ => {
                        payload = subagent_raw_payload(
                            thread_id,
                            parent_thread_id,
                            imported_path,
                            Value::Object(parsed.clone()),
                            Some(format!("unsupported response_item.type={item_type}")),
                        );
                    }
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
                    payload = subagent_raw_payload(
                        thread_id,
                        parent_thread_id,
                        imported_path,
                        Value::Object(parsed.clone()),
                        Some("missing event_msg payload object".to_string()),
                    );
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
                    let phase = item
                        .get("phase")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string);
                    let role =
                        normalized_subagent_message_role(item.get("role").and_then(Value::as_str));
                    let direction = "output_text".to_string();
                    event_type = subagent_message_event_type(&role);
                    parse_status = "parsed".to_string();
                    payload = Value::Object(Map::from_iter([
                        ("actor_type".to_string(), Value::from("subagent")),
                        ("thread_id".to_string(), Value::from(thread_id)),
                        (
                            "parent_thread_id".to_string(),
                            Value::from(parent_thread_id),
                        ),
                        ("role".to_string(), Value::from(role.clone())),
                        ("direction".to_string(), Value::from(direction)),
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
                    current_subagent_message = Some(RecentSubagentMessage {
                        role,
                        phase,
                        text,
                        source: RecentSubagentMessageSource::EventMsgMessage,
                    });
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
                            let text = item
                                .get("message")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            let direction = "input_text".to_string();
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
                                ("role".to_string(), Value::from("user")),
                                ("direction".to_string(), Value::from(direction)),
                                ("text".to_string(), Value::from(text.clone())),
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
                            current_subagent_message = Some(RecentSubagentMessage {
                                role: "user".to_string(),
                                phase: item
                                    .get("phase")
                                    .and_then(Value::as_str)
                                    .map(str::trim)
                                    .filter(|value| !value.is_empty())
                                    .map(str::to_string),
                                text,
                                source: RecentSubagentMessageSource::EventMsgMessage,
                            });
                        }
                        "context_compacted" => {
                            if self.state.pending_context_compacted_duplicate {
                                self.state.pending_context_compacted_duplicate = false;
                                event_type = CONTEXT_COMPACTED_DUPLICATE.to_string();
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
                                    ("duplicate_of".to_string(), Value::from("compacted")),
                                    ("raw".to_string(), Value::Object(parsed.clone())),
                                ]));
                                return Some(self.build_subagent_event(
                                    seq,
                                    &ts,
                                    event_type,
                                    raw_type,
                                    parse_status,
                                    payload,
                                ));
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
                        "exec_command_end" => {
                            let call_id = item
                                .get("call_id")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            let tool_name =
                                legacy_event_msg_tool_name(&msg_type, &call_id, call_names);
                            event_type = normalized_tool_event_type(&tool_name, true);
                            payload = payload_to_value(&ToolResultPayload {
                                actor_type: Some("subagent".to_string()),
                                thread_id: Some(thread_id.to_string()),
                                parent_thread_id: Some(parent_thread_id.to_string()),
                                session_path: Some(imported_path.display().to_string()),
                                tool_name,
                                tool_use_id: (!call_id.is_empty()).then_some(call_id.clone()),
                                input: Some(Value::Object(Map::from_iter([(
                                    "command".to_string(),
                                    item.get("command").cloned().unwrap_or(Value::Null),
                                )]))),
                                status: item
                                    .get("status")
                                    .and_then(Value::as_str)
                                    .map(str::to_string)
                                    .or_else(|| Some("completed".to_string())),
                                phase: Some("completed".to_string()),
                                output: item
                                    .get("aggregated_output")
                                    .cloned()
                                    .or_else(|| item.get("stdout").cloned()),
                                stderr: item.get("stderr").cloned(),
                                exit_code: item
                                    .get("exit_code")
                                    .and_then(Value::as_i64)
                                    .map(|value| value as i32),
                                ..ToolResultPayload::default()
                            });
                            if let Some(obj) = payload.as_object_mut() {
                                obj.insert(
                                    "cwd".to_string(),
                                    item.get("cwd").cloned().unwrap_or(Value::Null),
                                );
                                obj.insert(
                                    "process_id".to_string(),
                                    item.get("process_id").cloned().unwrap_or(Value::Null),
                                );
                                obj.insert(
                                    "turn_id".to_string(),
                                    item.get("turn_id").cloned().unwrap_or(Value::Null),
                                );
                                obj.insert(
                                    "source".to_string(),
                                    item.get("source").cloned().unwrap_or(Value::Null),
                                );
                                obj.insert(
                                    "duration".to_string(),
                                    item.get("duration").cloned().unwrap_or(Value::Null),
                                );
                                obj.insert(
                                    "parsed_cmd".to_string(),
                                    item.get("parsed_cmd").cloned().unwrap_or(Value::Null),
                                );
                                obj.insert(
                                    "formatted_output".to_string(),
                                    item.get("formatted_output").cloned().unwrap_or(Value::Null),
                                );
                            }
                            enrich_skill_usage_payload(&mut payload);
                            insert_duplicate_of(&mut payload, "response_item.function_call_output");
                        }
                        "web_search_end" => {
                            event_type = if is_open_page_action(item.get("action")) {
                                WEB_OPEN.to_string()
                            } else {
                                WEB_SEARCH.to_string()
                            };
                            payload = payload_to_value(&ToolResultPayload {
                                actor_type: Some("subagent".to_string()),
                                thread_id: Some(thread_id.to_string()),
                                parent_thread_id: Some(parent_thread_id.to_string()),
                                session_path: Some(imported_path.display().to_string()),
                                tool_name: "web_search_call".to_string(),
                                tool_use_id: item
                                    .get("call_id")
                                    .and_then(Value::as_str)
                                    .map(str::to_string),
                                status: Some("completed".to_string()),
                                phase: Some("completed".to_string()),
                                output: Some(Value::Object(Map::from_iter([
                                    (
                                        "query".to_string(),
                                        item.get("query").cloned().unwrap_or(Value::Null),
                                    ),
                                    (
                                        "action".to_string(),
                                        item.get("action").cloned().unwrap_or(Value::Null),
                                    ),
                                ]))),
                                ..ToolResultPayload::default()
                            });
                            insert_duplicate_of(&mut payload, "response_item.web_search_call");
                        }
                        "collab_agent_spawn_end"
                        | "collab_waiting_end"
                        | "collab_close_end"
                        | "collab_agent_interaction_end" => {
                            let call_id = item
                                .get("call_id")
                                .and_then(Value::as_str)
                                .unwrap_or_default()
                                .to_string();
                            let tool_name =
                                legacy_event_msg_tool_name(&msg_type, &call_id, call_names);
                            let receiver_thread_ids =
                                legacy_event_msg_receiver_thread_ids(&msg_type, item);
                            let detection_confidence =
                                if is_collab_high_confidence_tool(&tool_name) {
                                    "high"
                                } else {
                                    "medium"
                                }
                                .to_string();

                            event_type = normalized_tool_event_type(&tool_name, true);
                            payload = payload_to_value(&ToolResultPayload {
                                actor_type: Some("subagent".to_string()),
                                thread_id: Some(thread_id.to_string()),
                                parent_thread_id: Some(parent_thread_id.to_string()),
                                session_path: Some(imported_path.display().to_string()),
                                tool_name,
                                tool_use_id: (!call_id.is_empty()).then_some(call_id.clone()),
                                status: legacy_event_msg_status(item),
                                phase: Some("completed".to_string()),
                                sender_thread_id: item
                                    .get("sender_thread_id")
                                    .and_then(Value::as_str)
                                    .map(str::to_string),
                                receiver_thread_ids: (!receiver_thread_ids.is_empty())
                                    .then_some(receiver_thread_ids),
                                prompt: item
                                    .get("prompt")
                                    .and_then(Value::as_str)
                                    .map(str::to_string),
                                agents_states: legacy_event_msg_agents_states(&msg_type, item),
                                detection_source: Some("legacy_event_msg".to_string()),
                                detection_confidence: Some(detection_confidence),
                                raw_ref: (!call_id.is_empty()).then_some(call_id.clone()),
                                output: Some(Value::Object(item.clone())),
                                ..ToolResultPayload::default()
                            });
                            if let Some(obj) = payload.as_object_mut() {
                                obj.insert(
                                    "new_thread_id".to_string(),
                                    item.get("new_thread_id").cloned().unwrap_or(Value::Null),
                                );
                                obj.insert(
                                    "new_agent_nickname".to_string(),
                                    item.get("new_agent_nickname")
                                        .cloned()
                                        .unwrap_or(Value::Null),
                                );
                                obj.insert(
                                    "new_agent_role".to_string(),
                                    item.get("new_agent_role").cloned().unwrap_or(Value::Null),
                                );
                                obj.insert(
                                    "receiver_agent_nickname".to_string(),
                                    item.get("receiver_agent_nickname")
                                        .cloned()
                                        .unwrap_or(Value::Null),
                                );
                                obj.insert(
                                    "receiver_agent_role".to_string(),
                                    item.get("receiver_agent_role")
                                        .cloned()
                                        .unwrap_or(Value::Null),
                                );
                                obj.insert(
                                    "model".to_string(),
                                    item.get("model").cloned().unwrap_or(Value::Null),
                                );
                                obj.insert(
                                    "reasoning_effort".to_string(),
                                    item.get("reasoning_effort").cloned().unwrap_or(Value::Null),
                                );
                                obj.insert(
                                    "timed_out".to_string(),
                                    item.get("timed_out").cloned().unwrap_or(Value::Null),
                                );
                            }
                            insert_duplicate_of(&mut payload, "response_item.function_call_output");
                        }
                        "item_completed" => {
                            let Some(completed_item) = item.get("item").and_then(Value::as_object)
                            else {
                                event_type = RAW_UNPARSED.to_string();
                                parse_status = "best_effort".to_string();
                                payload = subagent_raw_payload(
                                    thread_id,
                                    parent_thread_id,
                                    imported_path,
                                    Value::Object(parsed.clone()),
                                    Some("missing item_completed item object".to_string()),
                                );
                                return Some(self.build_subagent_event(
                                    seq,
                                    &ts,
                                    event_type,
                                    raw_type,
                                    parse_status,
                                    payload,
                                ));
                            };
                            let item_type = completed_item
                                .get("type")
                                .and_then(Value::as_str)
                                .unwrap_or("unknown");
                            if item_type == "Plan" {
                                let item_id = completed_item
                                    .get("id")
                                    .and_then(Value::as_str)
                                    .map(str::to_string);
                                event_type = TODO_UPDATE.to_string();
                                payload = payload_to_value(&ToolResultPayload {
                                    actor_type: Some("subagent".to_string()),
                                    thread_id: Some(thread_id.to_string()),
                                    parent_thread_id: Some(parent_thread_id.to_string()),
                                    session_path: Some(imported_path.display().to_string()),
                                    tool_name: "update_plan".to_string(),
                                    tool_use_id: item_id.clone(),
                                    status: Some("completed".to_string()),
                                    phase: Some("completed".to_string()),
                                    output: Some(Value::Object(Map::from_iter([
                                        (
                                            "item_type".to_string(),
                                            Value::from(item_type.to_string()),
                                        ),
                                        (
                                            "item_id".to_string(),
                                            item_id.clone().map(Value::from).unwrap_or(Value::Null),
                                        ),
                                        (
                                            "text".to_string(),
                                            completed_item
                                                .get("text")
                                                .cloned()
                                                .unwrap_or(Value::Null),
                                        ),
                                        (
                                            "turn_id".to_string(),
                                            item.get("turn_id").cloned().unwrap_or(Value::Null),
                                        ),
                                    ]))),
                                    raw_ref: item_id,
                                    ..ToolResultPayload::default()
                                });
                                insert_duplicate_of(&mut payload, "response_item.message");
                            } else {
                                event_type = RAW_UNPARSED.to_string();
                                parse_status = "best_effort".to_string();
                                payload = subagent_raw_payload(
                                    thread_id,
                                    parent_thread_id,
                                    imported_path,
                                    Value::Object(parsed.clone()),
                                    Some(format!(
                                        "unsupported item_completed item.type={item_type}"
                                    )),
                                );
                            }
                        }
                        _ => {
                            event_type = RAW_UNPARSED.to_string();
                            parse_status = "best_effort".to_string();
                            payload = subagent_raw_payload(
                                thread_id,
                                parent_thread_id,
                                imported_path,
                                Value::Object(parsed.clone()),
                                Some(format!("unsupported event_msg.type={msg_type}")),
                            );
                        }
                    }
                }
            }
            "turn_context" => {
                event_type = RUNTIME_CONTEXT.to_string();
                parse_status = "parsed".to_string();
                let context = parsed.get("payload").and_then(Value::as_object);
                let mut normalized = Map::from_iter([
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
                ]);
                if let Some(context) = context {
                    for (key, value) in context {
                        if key == "turn_id" {
                            continue;
                        }
                        normalized.insert(key.clone(), value.clone());
                    }
                }
                payload = Value::Object(normalized);
            }
            _ => {
                payload = subagent_raw_payload(
                    thread_id,
                    parent_thread_id,
                    imported_path,
                    Value::Object(parsed.clone()),
                    Some(format!("unsupported subagent record type={raw_type}")),
                );
            }
        }

        if let Some(current_message) = current_subagent_message {
            if should_skip_subagent_message_duplicate(
                self.state.last_subagent_message.as_ref(),
                &current_message,
            ) {
                return None;
            }
            self.state.last_subagent_message = Some(current_message);
        } else {
            self.state.last_subagent_message = None;
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

    fn extract_message_direction(&self, item: &Map<String, Value>) -> Option<String> {
        let content = item.get("content").and_then(Value::as_array)?;
        content
            .iter()
            .filter_map(Value::as_object)
            .filter_map(|chunk| chunk.get("type").and_then(Value::as_str))
            .map(str::trim)
            .find(|content_type| matches!(*content_type, "input_text" | "output_text"))
            .map(str::to_string)
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

fn subagent_base_payload_map(
    thread_id: &str,
    parent_thread_id: &str,
    imported_path: &Path,
) -> Map<String, Value> {
    Map::from_iter([
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
    ])
}

fn subagent_raw_payload(
    thread_id: &str,
    parent_thread_id: &str,
    imported_path: &Path,
    raw: Value,
    reason: Option<String>,
) -> Value {
    let mut payload = subagent_base_payload_map(thread_id, parent_thread_id, imported_path);
    payload.insert("raw".to_string(), raw);
    if let Some(reason) = reason.filter(|reason| !reason.trim().is_empty()) {
        payload.insert("reason".to_string(), Value::from(reason));
    }
    Value::Object(payload)
}

fn enrich_response_item_shell_result_payload(
    payload: &mut Value,
    raw_output: Option<&Value>,
    parsed_output: &Value,
) {
    let Some(object) = payload.as_object_mut() else {
        return;
    };

    object
        .entry("status".to_string())
        .or_insert_with(|| Value::from("completed"));
    object
        .entry("phase".to_string())
        .or_insert_with(|| Value::from("completed"));

    if let Some(exit_code) = response_item_shell_exit_code(raw_output, parsed_output) {
        object.insert("exit_code".to_string(), Value::from(exit_code));
    }

    if let Some(original_token_count) =
        response_item_shell_original_token_count(raw_output, parsed_output)
    {
        object.insert(
            "original_token_count".to_string(),
            Value::from(original_token_count),
        );
    }

    if let Some(formatted_output) = response_item_shell_formatted_output(raw_output) {
        object.insert(
            "formatted_output".to_string(),
            Value::from(formatted_output),
        );
    }
}

fn response_item_shell_exit_code(raw_output: Option<&Value>, parsed_output: &Value) -> Option<i32> {
    parsed_output
        .as_object()
        .and_then(|object| object.get("exit_code"))
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
        .or_else(|| {
            raw_output
                .and_then(Value::as_str)
                .and_then(shell_exit_code_from_formatted_output)
        })
}

fn response_item_shell_original_token_count(
    raw_output: Option<&Value>,
    parsed_output: &Value,
) -> Option<u64> {
    parsed_output
        .as_object()
        .and_then(|object| object.get("original_token_count"))
        .and_then(Value::as_u64)
        .or_else(|| {
            raw_output
                .and_then(Value::as_str)
                .and_then(shell_original_token_count_from_formatted_output)
        })
}

fn response_item_shell_formatted_output(raw_output: Option<&Value>) -> Option<String> {
    raw_output
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .map(str::to_string)
}

fn shell_exit_code_from_formatted_output(formatted_output: &str) -> Option<i32> {
    formatted_output.lines().find_map(|line| {
        line.trim()
            .strip_prefix("Process exited with code ")
            .and_then(|code| code.parse::<i32>().ok())
    })
}

fn shell_original_token_count_from_formatted_output(formatted_output: &str) -> Option<u64> {
    formatted_output.lines().find_map(|line| {
        line.trim()
            .strip_prefix("Original token count: ")
            .map(|count| count.replace(',', ""))
            .and_then(|count| count.parse::<u64>().ok())
    })
}

fn legacy_event_msg_tool_name(
    msg_type: &str,
    call_id: &str,
    call_names: &HashMap<String, String>,
) -> String {
    if let Some(name) = call_names.get(call_id) {
        return name.clone();
    }

    match msg_type {
        "exec_command_end" => "exec_command",
        "collab_agent_spawn_end" => "spawn_agent",
        "collab_waiting_end" => "wait_agent",
        "collab_close_end" => "close_agent",
        "collab_agent_interaction_end" => "send_input",
        _ => "unknown",
    }
    .to_string()
}

fn legacy_event_msg_status(item: &Map<String, Value>) -> Option<String> {
    match item.get("status") {
        Some(Value::String(status)) if !status.trim().is_empty() => Some(status.to_string()),
        Some(Value::Object(statuses)) if statuses.len() == 1 => statuses.keys().next().cloned(),
        Some(Value::Bool(true)) => Some("completed".to_string()),
        Some(Value::Bool(false)) => Some("failed".to_string()),
        _ => Some("completed".to_string()),
    }
}

fn legacy_event_msg_receiver_thread_ids(msg_type: &str, item: &Map<String, Value>) -> Vec<String> {
    match msg_type {
        "collab_agent_spawn_end" => item
            .get("new_thread_id")
            .and_then(Value::as_str)
            .map(|thread_id| vec![thread_id.to_string()])
            .unwrap_or_default(),
        "collab_close_end" | "collab_agent_interaction_end" => item
            .get("receiver_thread_id")
            .and_then(Value::as_str)
            .map(|thread_id| vec![thread_id.to_string()])
            .unwrap_or_default(),
        "collab_waiting_end" => {
            let mut receiver_thread_ids = Vec::new();
            if let Some(agent_statuses) = item.get("agent_statuses").and_then(Value::as_array) {
                for entry in agent_statuses {
                    let Some(entry_obj) = entry.as_object() else {
                        continue;
                    };
                    let Some(thread_id) = entry_obj.get("thread_id").and_then(Value::as_str) else {
                        continue;
                    };
                    if !receiver_thread_ids.iter().any(|known| known == thread_id) {
                        receiver_thread_ids.push(thread_id.to_string());
                    }
                }
            }
            if let Some(statuses) = item.get("statuses").and_then(Value::as_object) {
                for thread_id in statuses.keys() {
                    if !receiver_thread_ids.iter().any(|known| known == thread_id) {
                        receiver_thread_ids.push(thread_id.clone());
                    }
                }
            }
            receiver_thread_ids
        }
        _ => Vec::new(),
    }
}

fn legacy_event_msg_agents_states(
    msg_type: &str,
    item: &Map<String, Value>,
) -> Option<serde_json::Map<String, Value>> {
    match msg_type {
        "collab_waiting_end" => {
            if let Some(statuses) = item.get("statuses").and_then(Value::as_object) {
                return Some(statuses.clone());
            }

            let mut states = Map::new();
            if let Some(agent_statuses) = item.get("agent_statuses").and_then(Value::as_array) {
                for entry in agent_statuses {
                    let Some(entry_obj) = entry.as_object() else {
                        continue;
                    };
                    let Some(thread_id) = entry_obj.get("thread_id").and_then(Value::as_str) else {
                        continue;
                    };
                    let mut state = Map::new();
                    state.insert(
                        "agent_nickname".to_string(),
                        entry_obj
                            .get("agent_nickname")
                            .cloned()
                            .unwrap_or(Value::Null),
                    );
                    state.insert(
                        "agent_role".to_string(),
                        entry_obj.get("agent_role").cloned().unwrap_or(Value::Null),
                    );
                    state.insert(
                        "status".to_string(),
                        entry_obj.get("status").cloned().unwrap_or(Value::Null),
                    );
                    states.insert(thread_id.to_string(), Value::Object(state));
                }
            }
            (!states.is_empty()).then_some(states)
        }
        "collab_agent_spawn_end" => {
            let thread_id = item.get("new_thread_id").and_then(Value::as_str)?;
            let mut states = Map::new();
            states.insert(
                thread_id.to_string(),
                Value::Object(Map::from_iter([
                    (
                        "status".to_string(),
                        item.get("status").cloned().unwrap_or(Value::Null),
                    ),
                    (
                        "agent_nickname".to_string(),
                        item.get("new_agent_nickname")
                            .cloned()
                            .unwrap_or(Value::Null),
                    ),
                    (
                        "agent_role".to_string(),
                        item.get("new_agent_role").cloned().unwrap_or(Value::Null),
                    ),
                    (
                        "model".to_string(),
                        item.get("model").cloned().unwrap_or(Value::Null),
                    ),
                    (
                        "reasoning_effort".to_string(),
                        item.get("reasoning_effort").cloned().unwrap_or(Value::Null),
                    ),
                ])),
            );
            Some(states)
        }
        "collab_close_end" | "collab_agent_interaction_end" => {
            let thread_id = item.get("receiver_thread_id").and_then(Value::as_str)?;
            let status = match item.get("status") {
                Some(Value::Object(value)) => Value::Object(value.clone()),
                Some(value) => {
                    Value::Object(Map::from_iter([("status".to_string(), value.clone())]))
                }
                None => Value::Object(Map::new()),
            };
            Some(Map::from_iter([(thread_id.to_string(), status)]))
        }
        _ => None,
    }
}

fn insert_duplicate_of(payload: &mut Value, duplicate_of: &str) {
    if let Some(obj) = payload.as_object_mut() {
        obj.insert("duplicate_of".to_string(), Value::from(duplicate_of));
    }
}

fn enrich_spawn_agent_started_payload(payload: &mut Value, arguments: &Value) {
    let Some(payload_obj) = payload.as_object_mut() else {
        return;
    };
    let Some(arguments_obj) = arguments.as_object() else {
        return;
    };

    if let Some(prompt) = arguments_obj
        .get("message")
        .or_else(|| arguments_obj.get("prompt"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        payload_obj.insert("prompt".to_string(), Value::from(prompt.to_string()));
    }
    if let Some(agent_type) = arguments_obj
        .get("agent_type")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        payload_obj.insert(
            "requested_agent_type".to_string(),
            Value::from(agent_type.to_string()),
        );
    }
    if let Some(model) = arguments_obj
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        payload_obj.insert("model".to_string(), Value::from(model.to_string()));
    }
    if let Some(reasoning_effort) = arguments_obj
        .get("reasoning_effort")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        payload_obj.insert(
            "reasoning_effort".to_string(),
            Value::from(reasoning_effort.to_string()),
        );
    }
}

fn enrich_spawn_agent_completed_payload(payload: &mut Value, output: &Value) {
    let Some(payload_obj) = payload.as_object_mut() else {
        return;
    };
    let Some(output_obj) = output.as_object() else {
        return;
    };

    let thread_id = output_obj
        .get("agent_id")
        .or_else(|| output_obj.get("thread_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let nickname = output_obj
        .get("nickname")
        .or_else(|| output_obj.get("agent_nickname"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let role = output_obj
        .get("agent_role")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    if let Some(thread_id) = thread_id.as_deref() {
        payload_obj.insert(
            "new_thread_id".to_string(),
            Value::from(thread_id.to_string()),
        );
        payload_obj.insert(
            "receiver_thread_ids".to_string(),
            Value::Array(vec![Value::from(thread_id.to_string())]),
        );
    }
    if let Some(nickname) = nickname.as_deref() {
        payload_obj.insert(
            "new_agent_nickname".to_string(),
            Value::from(nickname.to_string()),
        );
    }
    if let Some(role) = role.as_deref() {
        payload_obj.insert("new_agent_role".to_string(), Value::from(role.to_string()));
    }

    if let Some(thread_id) = thread_id {
        let state = Value::Object(Map::from_iter([
            (
                "agent_nickname".to_string(),
                nickname.map(Value::from).unwrap_or(Value::Null),
            ),
            (
                "agent_role".to_string(),
                role.map(Value::from).unwrap_or(Value::Null),
            ),
        ]));
        payload_obj.insert(
            "agents_states".to_string(),
            Value::Object(Map::from_iter([(thread_id, state)])),
        );
    }
}

fn normalized_subagent_message_role(role: Option<&str>) -> String {
    role.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_else(|| "assistant".to_string())
}

fn subagent_message_event_type(role: &str) -> String {
    format!("message.{role}")
}

fn should_skip_subagent_message_duplicate(
    previous: Option<&RecentSubagentMessage>,
    current: &RecentSubagentMessage,
) -> bool {
    previous
        .map(|message| {
            message.source != current.source
                && same_subagent_message_role(&message.role, &current.role)
                && same_subagent_message_phase(message.phase.as_deref(), current.phase.as_deref())
                && same_subagent_message_text(&message.text, &current.text)
        })
        .unwrap_or(false)
}

fn same_subagent_message_role(left: &str, right: &str) -> bool {
    left.trim() == right.trim()
}

fn same_subagent_message_text(left: &str, right: &str) -> bool {
    let left = left.trim();
    let right = right.trim();
    !left.is_empty() && left == right
}

fn same_subagent_message_phase(left: Option<&str>, right: Option<&str>) -> bool {
    let left = left.map(str::trim).filter(|value| !value.is_empty());
    let right = right.map(str::trim).filter(|value| !value.is_empty());
    left == right
}

pub fn imported_subagent_session_meta(
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
