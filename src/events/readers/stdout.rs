use super::*;

impl JsonOutputEventReader {
    pub fn parse_main_output_line(
        &mut self,
        seq: u64,
        line: &str,
        tool_counts: &mut HashMap<String, u64>,
        subagent_counts: &mut HashMap<String, u64>,
        subagent_threads: &mut HashSet<String>,
    ) -> (u64, CodexEvent) {
        let next_seq = seq + 1;
        let stripped = line.trim_end_matches('\n');
        let ts = utc_now_iso();
        let parsed = serde_json::from_str::<Value>(stripped);
        let Some(parsed_obj) = parsed.ok().and_then(|v| v.as_object().cloned()) else {
            self.state.valid_json = false;
            let event = CodexEvent {
                schema_version: 1,
                ts,
                task_id: self.context.task_id.clone(),
                run_id: self.context.run_id.clone(),
                seq: next_seq,
                event_type: RAW_UNPARSED.to_string(),
                raw_type: "invalid_json".to_string(),
                parse_status: "unparsed".to_string(),
                payload: payload_to_value(&RawPayload {
                    text: Some(stripped.to_string()),
                    ..RawPayload::default()
                }),
                source: "json_output".to_string(),
            };
            return (next_seq, event);
        };

        let raw_type = parsed_obj
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let mut event_type = "raw.unparsed".to_string();
        let mut payload = payload_to_value(&RawPayload {
            actor_type: Some("agent".to_string()),
            thread_id: self.state.thread_id.clone(),
            raw: Some(Value::Object(parsed_obj.clone())),
            ..RawPayload::default()
        });
        let mut parse_status = "best_effort".to_string();

        match raw_type.as_str() {
            "thread.started" => {
                self.state.saw_thread_started = true;
                self.state.thread_id = parsed_obj
                    .get("thread_id")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                event_type = THREAD_STARTED.to_string();
                parse_status = "parsed".to_string();
                payload = Value::Object(Map::from_iter([
                    (
                        "thread_id".to_string(),
                        parsed_obj.get("thread_id").cloned().unwrap_or(Value::Null),
                    ),
                    (
                        "model".to_string(),
                        parsed_obj.get("model").cloned().unwrap_or(Value::Null),
                    ),
                ]));
            }
            "turn.started" => {
                self.state.turn_open = true;
                self.state.current_turn_messages.clear();
                event_type = AGENT_STARTED.to_string();
                parse_status = "parsed".to_string();
                payload = payload_to_value(&AgentTurnPayload {
                    actor_type: Some("agent".to_string()),
                    thread_id: self.state.thread_id.clone(),
                    ..AgentTurnPayload::default()
                });
            }
            "turn.completed" => {
                self.state.saw_turn_completed = true;
                self.state.last_terminal = Some("turn.completed".to_string());
                self.state.turn_open = false;
                if let Some(last) = self.state.current_turn_messages.last() {
                    self.state.final_agent_message = Some(last.clone());
                }
                event_type = AGENT_COMPLETED.to_string();
                parse_status = "parsed".to_string();
                payload = payload_to_value(&AgentTurnPayload {
                    actor_type: Some("agent".to_string()),
                    thread_id: self.state.thread_id.clone(),
                    usage: parsed_obj.get("usage").cloned(),
                    result: parsed_obj.get("result").cloned(),
                    ..AgentTurnPayload::default()
                });
            }
            "turn.failed" => {
                self.state.saw_turn_failed = true;
                self.state.last_terminal = Some("turn.failed".to_string());
                self.state.turn_open = false;
                let turn_message = parsed_obj.get("message").and_then(Value::as_str);
                let mut error_message = turn_message.map(str::to_string);
                if error_message.is_none() {
                    error_message = parsed_obj
                        .get("error")
                        .and_then(Value::as_object)
                        .and_then(|v| v.get("message"))
                        .and_then(Value::as_str)
                        .map(str::to_string);
                }
                event_type = AGENT_FAILED.to_string();
                parse_status = "parsed".to_string();
                payload = payload_to_value(&AgentTurnPayload {
                    actor_type: Some("agent".to_string()),
                    thread_id: self.state.thread_id.clone(),
                    error: parsed_obj.get("error").cloned(),
                    message: turn_message.map(str::to_string),
                    text_links: error_message.and_then(|msg| self.extract_text_links(&msg)),
                    ..AgentTurnPayload::default()
                });
            }
            "error" => {
                self.state.saw_top_error = true;
                self.state
                    .top_errors
                    .push(Value::Object(parsed_obj.clone()));
                event_type = "error".to_string();
                parse_status = "parsed".to_string();
                payload = payload_to_value(&self.build_error_payload(
                    parsed_obj.get("message").and_then(Value::as_str),
                    parsed_obj.get("type").and_then(Value::as_str),
                ));
            }
            "item.started" | "item.updated" | "item.completed" => {
                if let Some(item) = parsed_obj.get("item").and_then(Value::as_object) {
                    let (parsed_event_type, parsed_payload, status) = self.parse_item_event(
                        item,
                        &raw_type,
                        tool_counts,
                        subagent_counts,
                        subagent_threads,
                    );
                    if parsed_event_type == "error" {
                        self.state.item_errors.push(parsed_payload.clone());
                    }
                    if parsed_event_type == MESSAGE_AGENT {
                        if let Some(text) = parsed_payload.get("text").and_then(Value::as_str) {
                            if !text.trim().is_empty() {
                                if self.state.turn_open {
                                    self.state.current_turn_messages.push(text.to_string());
                                } else if self.state.last_terminal.as_deref()
                                    == Some("turn.completed")
                                {
                                    self.state.final_agent_message = Some(text.to_string());
                                }
                            }
                        }
                    }
                    event_type = parsed_event_type;
                    payload = parsed_payload;
                    parse_status = status;
                }
            }
            _ => {}
        }

        (
            next_seq,
            CodexEvent {
                schema_version: 1,
                ts,
                task_id: self.context.task_id.clone(),
                run_id: self.context.run_id.clone(),
                seq: next_seq,
                event_type,
                raw_type,
                parse_status,
                payload,
                source: "json_output".to_string(),
            },
        )
    }

    fn parse_item_event(
        &mut self,
        item: &Map<String, Value>,
        event_type: &str,
        tool_counts: &mut HashMap<String, u64>,
        subagent_counts: &mut HashMap<String, u64>,
        subagent_threads: &mut HashSet<String>,
    ) -> (String, Value, String) {
        let item_type = item
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        if matches!(
            item_type.as_str(),
            "item.started" | "item.updated" | "item.completed"
        ) {
            if let Some(nested_item) = item.get("item").and_then(Value::as_object) {
                return self.parse_item_event(
                    nested_item,
                    &item_type,
                    tool_counts,
                    subagent_counts,
                    subagent_threads,
                );
            }
        }
        let phase = match event_type {
            "item.started" => "started",
            "item.updated" => "updated",
            _ => "completed",
        }
        .to_string();

        match item_type.as_str() {
            "agent_message" => (
                MESSAGE_AGENT.to_string(),
                payload_to_value(&AgentMessagePayload {
                    actor_type: Some("agent".to_string()),
                    thread_id: self.state.thread_id.clone(),
                    item_id: item.get("id").and_then(Value::as_str).map(str::to_string),
                    text: item
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    text_links: item
                        .get("text")
                        .and_then(Value::as_str)
                        .and_then(|text| self.extract_text_links(text)),
                    ..AgentMessagePayload::default()
                }),
                "parsed".to_string(),
            ),
            "reasoning" => (
                "agent.reasoning".to_string(),
                payload_to_value(&AgentMessagePayload {
                    actor_type: Some("agent".to_string()),
                    thread_id: self.state.thread_id.clone(),
                    text: item
                        .get("text")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    text_links: item
                        .get("text")
                        .and_then(Value::as_str)
                        .and_then(|text| self.extract_text_links(text)),
                    ..AgentMessagePayload::default()
                }),
                "parsed".to_string(),
            ),
            "error" => {
                let mut payload = self.build_error_payload(
                    item.get("message").and_then(Value::as_str),
                    item.get("type").and_then(Value::as_str),
                );
                payload.item_id = item.get("id").and_then(Value::as_str).map(str::to_string);
                payload.status = item
                    .get("status")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                payload.phase = Some(phase);
                (
                    "error".to_string(),
                    payload_to_value(&payload),
                    "parsed".to_string(),
                )
            }
            "command_execution" => {
                increment(tool_counts, "command_execution");
                if phase == "started" {
                    (
                        "shell.call".to_string(),
                        payload_to_value(&ToolCallPayload {
                            actor_type: Some("agent".to_string()),
                            thread_id: self.state.thread_id.clone(),
                            tool_name: "command_execution".to_string(),
                            tool_use_id: item.get("id").and_then(Value::as_str).map(str::to_string),
                            status: item
                                .get("status")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            input: Some(Value::Object(Map::from_iter([(
                                "command".to_string(),
                                item.get("command").cloned().unwrap_or(Value::Null),
                            )]))),
                            ..ToolCallPayload::default()
                        }),
                        "parsed".to_string(),
                    )
                } else {
                    (
                        "shell.result".to_string(),
                        payload_to_value(&ToolResultPayload {
                            actor_type: Some("agent".to_string()),
                            thread_id: self.state.thread_id.clone(),
                            tool_name: "command_execution".to_string(),
                            tool_use_id: item.get("id").and_then(Value::as_str).map(str::to_string),
                            status: item
                                .get("status")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            input: Some(Value::Object(Map::from_iter([(
                                "command".to_string(),
                                item.get("command").cloned().unwrap_or(Value::Null),
                            )]))),
                            exit_code: item
                                .get("exit_code")
                                .and_then(Value::as_i64)
                                .map(|v| v as i32),
                            stderr: item.get("stderr").cloned(),
                            output: item.get("aggregated_output").cloned(),
                            ..ToolResultPayload::default()
                        }),
                        "parsed".to_string(),
                    )
                }
            }
            "mcp_tool_call" => {
                let tool = item
                    .get("tool")
                    .and_then(Value::as_str)
                    .unwrap_or("mcp_tool_call")
                    .to_string();
                increment(tool_counts, &tool);
                if phase == "started" {
                    (
                        MCP_CALL.to_string(),
                        payload_to_value(&McpCallPayload {
                            actor_type: Some("agent".to_string()),
                            thread_id: self.state.thread_id.clone(),
                            tool_use_id: item.get("id").and_then(Value::as_str).map(str::to_string),
                            status: item
                                .get("status")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            phase: Some(phase),
                            arguments: item.get("arguments").cloned(),
                            server: item
                                .get("server")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            tool: Some(tool),
                            ..McpCallPayload::default()
                        }),
                        "parsed".to_string(),
                    )
                } else {
                    (
                        MCP_RESULT.to_string(),
                        payload_to_value(&McpResultPayload {
                            actor_type: Some("agent".to_string()),
                            thread_id: self.state.thread_id.clone(),
                            tool_use_id: item.get("id").and_then(Value::as_str).map(str::to_string),
                            status: item
                                .get("status")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            phase: Some(phase),
                            arguments: item.get("arguments").cloned(),
                            result: item.get("result").cloned(),
                            error: item.get("error").cloned(),
                            server: item
                                .get("server")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            tool: Some(tool),
                            ..McpResultPayload::default()
                        }),
                        "parsed".to_string(),
                    )
                }
            }
            "web_search" => {
                increment(tool_counts, "web_search");
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
                (
                    WEB_SEARCH.to_string(),
                    payload_to_value(&ToolResultPayload {
                        actor_type: Some("agent".to_string()),
                        thread_id: self.state.thread_id.clone(),
                        tool_name: "web_search".to_string(),
                        tool_use_id: item.get("id").and_then(Value::as_str).map(str::to_string),
                        phase: Some(phase.clone()),
                        status: item
                            .get("status")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        input: (phase != "completed").then_some(query_action.clone()),
                        output: (phase == "completed").then_some(query_action),
                        ..ToolResultPayload::default()
                    }),
                    "parsed".to_string(),
                )
            }
            "todo_list" => {
                let mut normalized_items: Vec<TodoItem> = Vec::new();
                let mut completed_count: u32 = 0;
                if let Some(items) = item.get("items").and_then(Value::as_array) {
                    for entry in items {
                        if let Some(entry_obj) = entry.as_object() {
                            let completed = entry_obj
                                .get("completed")
                                .and_then(Value::as_bool)
                                .unwrap_or(false);
                            if completed {
                                completed_count += 1;
                            }
                            normalized_items.push(TodoItem {
                                text: entry_obj
                                    .get("text")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string(),
                                completed,
                            });
                        }
                    }
                }
                (
                    "todo.update".to_string(),
                    payload_to_value(&TodoUpdatePayload {
                        actor_type: Some("agent".to_string()),
                        thread_id: self.state.thread_id.clone(),
                        item_id: item.get("id").and_then(Value::as_str).map(str::to_string),
                        status: item
                            .get("status")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        phase: Some(phase),
                        total_count: normalized_items.len() as u32,
                        completed_count,
                        items: normalized_items,
                    }),
                    "parsed".to_string(),
                )
            }
            "collab_tool_call" => {
                let tool_name = item
                    .get("tool")
                    .and_then(Value::as_str)
                    .unwrap_or("collab_tool_call")
                    .to_string();
                increment(tool_counts, &tool_name);
                increment(subagent_counts, &tool_name);
                let receiver_ids = item
                    .get("receiver_thread_ids")
                    .and_then(Value::as_array)
                    .map(|list| {
                        list.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect::<Vec<String>>()
                    })
                    .unwrap_or_default();
                for receiver in &receiver_ids {
                    subagent_threads.insert(receiver.clone());
                }
                let detection_confidence = if is_collab_high_confidence_tool(&tool_name) {
                    "high"
                } else {
                    "medium"
                }
                .to_string();
                let thread_id = item.get("sender_thread_id").cloned().unwrap_or_else(|| {
                    self.state
                        .thread_id
                        .clone()
                        .map(Value::from)
                        .unwrap_or(Value::Null)
                });
                let collab_event_type = collab_event_type(&tool_name);
                if let Some(collab_event_type) = collab_event_type {
                    (
                        collab_event_type.to_string(),
                        payload_to_value(&ToolResultPayload {
                            actor_type: Some("agent".to_string()),
                            thread_id: thread_id.as_str().map(str::to_string),
                            tool_name: tool_name.clone(),
                            tool_use_id: item.get("id").and_then(Value::as_str).map(str::to_string),
                            phase: Some(phase.clone()),
                            status: item
                                .get("status")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            sender_thread_id: item
                                .get("sender_thread_id")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            receiver_thread_ids: Some(receiver_ids),
                            prompt: item
                                .get("prompt")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            agents_states: item
                                .get("agents_states")
                                .and_then(Value::as_object)
                                .cloned(),
                            detection_source: Some("tool_payload".to_string()),
                            detection_confidence: Some(detection_confidence),
                            raw_ref: item.get("id").and_then(Value::as_str).map(str::to_string),
                            ..ToolResultPayload::default()
                        }),
                        "parsed".to_string(),
                    )
                } else if phase == "completed" {
                    (
                        "tool.result".to_string(),
                        payload_to_value(&ToolResultPayload {
                            actor_type: Some("agent".to_string()),
                            thread_id: thread_id.as_str().map(str::to_string),
                            tool_name: tool_name.clone(),
                            tool_use_id: item.get("id").and_then(Value::as_str).map(str::to_string),
                            phase: Some(phase),
                            status: item
                                .get("status")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            sender_thread_id: item
                                .get("sender_thread_id")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            receiver_thread_ids: Some(receiver_ids),
                            prompt: item
                                .get("prompt")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            agents_states: item
                                .get("agents_states")
                                .and_then(Value::as_object)
                                .cloned(),
                            detection_source: Some("tool_payload".to_string()),
                            detection_confidence: Some(detection_confidence),
                            raw_ref: item.get("id").and_then(Value::as_str).map(str::to_string),
                            ..ToolResultPayload::default()
                        }),
                        "parsed".to_string(),
                    )
                } else {
                    (
                        "tool.call".to_string(),
                        payload_to_value(&ToolCallPayload {
                            actor_type: Some("agent".to_string()),
                            thread_id: thread_id.as_str().map(str::to_string),
                            tool_name: tool_name.clone(),
                            tool_use_id: item.get("id").and_then(Value::as_str).map(str::to_string),
                            phase: Some(phase),
                            status: item
                                .get("status")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            sender_thread_id: item
                                .get("sender_thread_id")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            receiver_thread_ids: Some(receiver_ids),
                            prompt: item
                                .get("prompt")
                                .and_then(Value::as_str)
                                .map(str::to_string),
                            agents_states: item
                                .get("agents_states")
                                .and_then(Value::as_object)
                                .cloned(),
                            detection_source: Some("tool_payload".to_string()),
                            detection_confidence: Some(detection_confidence),
                            raw_ref: item.get("id").and_then(Value::as_str).map(str::to_string),
                            ..ToolCallPayload::default()
                        }),
                        "parsed".to_string(),
                    )
                }
            }
            "tool_use" => {
                let tool_name = item
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                increment(tool_counts, &tool_name);
                let is_subagent_tool = is_subagent_tool(&tool_name);
                if is_subagent_tool {
                    increment(subagent_counts, &tool_name);
                }
                let event_type = normalized_tool_event_type(&tool_name, false);
                if is_singleton_tool_event_type(&event_type) {
                    (
                        event_type,
                        payload_to_value(&ToolResultPayload {
                            actor_type: Some("agent".to_string()),
                            thread_id: self.state.thread_id.clone(),
                            tool_name,
                            tool_use_id: item.get("id").and_then(Value::as_str).map(str::to_string),
                            phase: Some("started".to_string()),
                            input: item.get("input").cloned(),
                            detection_source: is_subagent_tool.then(|| "tool_name".to_string()),
                            detection_confidence: is_subagent_tool.then(|| "high".to_string()),
                            raw_ref: is_subagent_tool
                                .then(|| item.get("id").and_then(Value::as_str).map(str::to_string))
                                .flatten(),
                            ..ToolResultPayload::default()
                        }),
                        "parsed".to_string(),
                    )
                } else {
                    (
                        event_type,
                        payload_to_value(&ToolCallPayload {
                            actor_type: Some("agent".to_string()),
                            thread_id: self.state.thread_id.clone(),
                            tool_name,
                            tool_use_id: item.get("id").and_then(Value::as_str).map(str::to_string),
                            input: item.get("input").cloned(),
                            detection_source: is_subagent_tool.then(|| "tool_name".to_string()),
                            detection_confidence: is_subagent_tool.then(|| "high".to_string()),
                            raw_ref: is_subagent_tool
                                .then(|| item.get("id").and_then(Value::as_str).map(str::to_string))
                                .flatten(),
                            ..ToolCallPayload::default()
                        }),
                        "parsed".to_string(),
                    )
                }
            }
            "tool_result" => {
                let tool_name = item
                    .get("name")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();
                increment(tool_counts, &tool_name);
                let event_type = normalized_tool_event_type(&tool_name, true);
                let is_singleton_event = is_singleton_tool_event_type(&event_type);
                (
                    event_type,
                    payload_to_value(&ToolResultPayload {
                        actor_type: Some("agent".to_string()),
                        thread_id: self.state.thread_id.clone(),
                        tool_name,
                        tool_use_id: item
                            .get("tool_use_id")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                            .or_else(|| item.get("id").and_then(Value::as_str).map(str::to_string)),
                        phase: is_singleton_event.then(|| "completed".to_string()),
                        status: item
                            .get("status")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        stderr: item.get("stderr").cloned(),
                        error: item.get("error").cloned(),
                        output: item
                            .get("output")
                            .cloned()
                            .or_else(|| item.get("content").cloned())
                            .or_else(|| item.get("result").cloned()),
                        ..ToolResultPayload::default()
                    }),
                    "parsed".to_string(),
                )
            }
            "file_change" => {
                let mut normalized_changes: Vec<FileChange> = Vec::new();
                if let Some(changes) = item.get("changes").and_then(Value::as_array) {
                    for change in changes {
                        if let Some(change_obj) = change.as_object() {
                            normalized_changes.push(FileChange {
                                kind: change_obj
                                    .get("kind")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string(),
                                path: change_obj
                                    .get("path")
                                    .and_then(Value::as_str)
                                    .unwrap_or_default()
                                    .to_string(),
                            });
                        }
                    }
                }
                (
                    "file.change".to_string(),
                    payload_to_value(&FileChangePayload {
                        actor_type: Some("agent".to_string()),
                        thread_id: self.state.thread_id.clone(),
                        item_id: item.get("id").and_then(Value::as_str).map(str::to_string),
                        status: item
                            .get("status")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        changes: normalized_changes,
                    }),
                    "parsed".to_string(),
                )
            }
            _ => (
                "raw.unparsed".to_string(),
                payload_to_value(&RawPayload {
                    actor_type: Some("agent".to_string()),
                    thread_id: self.state.thread_id.clone(),
                    raw: Some(Value::Object(item.clone())),
                    ..RawPayload::default()
                }),
                "best_effort".to_string(),
            ),
        }
    }
}
