use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::error::{AppError, AppResult};
use crate::events::readers::{
    imported_subagent_session_meta, JsonOutputEventReader, RunEventContext,
};
use crate::events::record::EventRecord;
use crate::util::utc_now_iso;

#[derive(Debug, Clone, PartialEq)]
pub struct ReplayedRunStream {
    pub task_id: String,
    pub run_id: String,
    pub root_thread_id: Option<String>,
    pub subagent_threads: BTreeSet<String>,
    pub events: Vec<EventRecord>,
    pub event_counts: BTreeMap<String, u64>,
    pub tool_counts: BTreeMap<String, u64>,
    pub subagent_counts: BTreeMap<String, u64>,
}

impl ReplayedRunStream {
    pub fn replay_run_dir(run_dir: &Path) -> AppResult<Self> {
        let context = ReplayRunContext::discover(run_dir)?;
        let mut reader = JsonOutputEventReader::new(
            RunEventContext {
                task_id: context.task_id.clone(),
                run_id: context.run_id.clone(),
            },
            None,
        );

        let mut seq = 0u64;
        let mut events = Vec::new();
        let mut event_counts: HashMap<String, u64> = HashMap::new();
        let mut tool_counts: HashMap<String, u64> = HashMap::new();
        let mut subagent_counts: HashMap<String, u64> = HashMap::new();
        let mut subagent_threads: HashSet<String> = HashSet::new();
        let mut replayed_subagents: HashSet<String> = HashSet::new();

        let stdout = File::open(&context.stdout_path)?;
        let stdout_reader = BufReader::new(stdout);
        for line in stdout_reader.lines() {
            let line = line?;
            let (next_seq, parsed_event) = reader.parse_main_output_line(
                seq,
                &line,
                &mut tool_counts,
                &mut subagent_counts,
                &mut subagent_threads,
            );
            seq = next_seq;
            push_event(&mut events, &mut event_counts, parsed_event.to_record());

            let mut newly_discovered: Vec<String> = subagent_threads
                .difference(&replayed_subagents)
                .cloned()
                .collect();
            newly_discovered.sort();
            for thread_id in newly_discovered {
                seq = replay_subagent_session(
                    &context,
                    seq,
                    &thread_id,
                    reader.state.thread_id.as_deref().unwrap_or(""),
                    &mut events,
                    &mut event_counts,
                    &mut tool_counts,
                    &mut subagent_counts,
                )?;
                replayed_subagents.insert(thread_id);
            }
        }

        if context.stderr_path.exists() {
            let stderr = File::open(&context.stderr_path)?;
            let stderr_reader = BufReader::new(stderr);
            for line in stderr_reader.lines() {
                let line = line?;
                seq += 1;
                push_event(
                    &mut events,
                    &mut event_counts,
                    EventRecord {
                        schema_version: 1,
                        ts: utc_now_iso(),
                        task_id: context.task_id.clone(),
                        run_id: context.run_id.clone(),
                        seq,
                        event_type: "stderr.line".to_string(),
                        raw_type: "stderr".to_string(),
                        parse_status: "parsed".to_string(),
                        payload: Value::Object(Map::from_iter([
                            ("actor_type".to_string(), Value::from("system")),
                            ("text".to_string(), Value::from(line)),
                        ])),
                    },
                );
            }
        }

        Ok(Self {
            task_id: context.task_id,
            run_id: context.run_id,
            root_thread_id: reader.state.thread_id.clone(),
            subagent_threads: subagent_threads.into_iter().collect(),
            events,
            event_counts: to_btree(&event_counts),
            tool_counts: to_btree(&tool_counts),
            subagent_counts: to_btree(&subagent_counts),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReplayRunContext {
    task_id: String,
    run_id: String,
    stdout_path: PathBuf,
    stderr_path: PathBuf,
    subagents_dir: PathBuf,
}

impl ReplayRunContext {
    fn discover(run_dir: &Path) -> AppResult<Self> {
        let run_dir = run_dir.canonicalize()?;
        if !run_dir.is_dir() {
            return Err(AppError::Runner(format!(
                "run dir is not a directory: {}",
                run_dir.display()
            )));
        }

        let Some(task_dir) = run_dir.parent().and_then(Path::parent) else {
            return Err(AppError::Runner(format!(
                "cannot resolve task dir from run dir: {}",
                run_dir.display()
            )));
        };
        let task_json_path = task_dir.join("task.json");
        let task_json_text = std::fs::read_to_string(&task_json_path)?;
        let task_json: Value = serde_json::from_str(&task_json_text).map_err(|err| {
            AppError::Runner(format!(
                "cannot parse task snapshot {}: {err}",
                task_json_path.display()
            ))
        })?;
        let task_id = task_json
            .get("task_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                AppError::Runner(format!(
                    "task_id is missing in {}",
                    task_json_path.display()
                ))
            })?
            .to_string();

        let run_name = run_dir
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| {
                AppError::Runner(format!(
                    "run dir name is not valid utf-8: {}",
                    run_dir.display()
                ))
            })?;
        let (_, run_id) = run_name.split_once("--").ok_or_else(|| {
            AppError::Runner(format!(
                "run dir name does not contain run id suffix: {}",
                run_dir.display()
            ))
        })?;

        Ok(Self {
            task_id,
            run_id: run_id.to_string(),
            stdout_path: run_dir.join("stdout.jsonl"),
            stderr_path: run_dir.join("stderr.log"),
            subagents_dir: run_dir.join("subagents"),
        })
    }
}

fn replay_subagent_session(
    context: &ReplayRunContext,
    mut seq: u64,
    thread_id: &str,
    fallback_parent_thread_id: &str,
    events: &mut Vec<EventRecord>,
    event_counts: &mut HashMap<String, u64>,
    tool_counts: &mut HashMap<String, u64>,
    subagent_counts: &mut HashMap<String, u64>,
) -> AppResult<u64> {
    let session_path = context.subagents_dir.join(format!("{thread_id}.jsonl"));
    if !session_path.exists() {
        return Ok(seq);
    }

    let mut reader = JsonOutputEventReader::new(
        RunEventContext {
            task_id: context.task_id.clone(),
            run_id: context.run_id.clone(),
        },
        None,
    );
    let mut call_names = HashMap::new();
    let mut current_parent_thread_id = fallback_parent_thread_id.to_string();

    let mut source = File::open(&session_path)?;
    let mut chunk = Vec::new();
    source.read_to_end(&mut chunk)?;
    let mut partial_bytes = Vec::new();
    let lines = split_session_chunk_lines(&mut partial_bytes, &chunk, true);

    for line in lines {
        let stripped = trim_line_bytes(&line);
        if stripped.is_empty() {
            continue;
        }

        let next_seq = seq + 1;
        let ts = utc_now_iso();
        let stripped = match std::str::from_utf8(stripped) {
            Ok(text) => text,
            Err(_) => {
                push_event(
                    events,
                    event_counts,
                    subagent_unparsed_event(
                        &context.task_id,
                        &context.run_id,
                        next_seq,
                        &ts,
                        thread_id,
                        "invalid_utf8",
                        String::from_utf8_lossy(stripped).into_owned(),
                    ),
                );
                seq = next_seq;
                continue;
            }
        };

        let parsed = match serde_json::from_str::<Value>(stripped) {
            Ok(Value::Object(obj)) => obj,
            _ => {
                push_event(
                    events,
                    event_counts,
                    subagent_unparsed_event(
                        &context.task_id,
                        &context.run_id,
                        next_seq,
                        &ts,
                        thread_id,
                        "invalid_json",
                        stripped.to_string(),
                    ),
                );
                seq = next_seq;
                continue;
            }
        };

        if let Some(meta) = imported_subagent_session_meta(&parsed, thread_id) {
            if let Some(parent_thread_id) = meta.parent_thread_id {
                if current_parent_thread_id == fallback_parent_thread_id {
                    current_parent_thread_id = parent_thread_id;
                }
            }
        }

        let Some(codex_event) = reader.parse_subagent_session_payload(
            next_seq,
            &parsed,
            &session_path,
            &current_parent_thread_id,
            thread_id,
            &mut call_names,
            tool_counts,
            subagent_counts,
        ) else {
            continue;
        };

        push_event(events, event_counts, codex_event.to_record());
        seq = next_seq;
    }

    Ok(seq)
}

fn subagent_unparsed_event(
    task_id: &str,
    run_id: &str,
    seq: u64,
    ts: &str,
    thread_id: &str,
    raw_type: &str,
    text: String,
) -> EventRecord {
    EventRecord {
        schema_version: 1,
        ts: ts.to_string(),
        task_id: task_id.to_string(),
        run_id: run_id.to_string(),
        seq,
        event_type: "raw.unparsed".to_string(),
        raw_type: raw_type.to_string(),
        parse_status: "unparsed".to_string(),
        payload: Value::Object(Map::from_iter([
            ("actor_type".to_string(), Value::from("subagent")),
            ("thread_id".to_string(), Value::from(thread_id.to_string())),
            ("text".to_string(), Value::from(text)),
        ])),
    }
}

fn split_session_chunk_lines(
    partial_bytes: &mut Vec<u8>,
    chunk: &[u8],
    final_pass: bool,
) -> Vec<Vec<u8>> {
    let mut combined = std::mem::take(partial_bytes);
    combined.extend_from_slice(chunk);

    let mut lines = Vec::new();
    let mut line_start = 0usize;
    for (idx, byte) in combined.iter().enumerate() {
        if *byte == b'\n' {
            lines.push(combined[line_start..idx].to_vec());
            line_start = idx + 1;
        }
    }

    if final_pass {
        if line_start < combined.len() {
            lines.push(combined[line_start..].to_vec());
        }
    } else {
        partial_bytes.extend_from_slice(&combined[line_start..]);
    }

    lines
}

fn trim_line_bytes(line: &[u8]) -> &[u8] {
    line.strip_suffix(b"\r").unwrap_or(line)
}

fn push_event(
    events: &mut Vec<EventRecord>,
    event_counts: &mut HashMap<String, u64>,
    record: EventRecord,
) {
    *event_counts.entry(record.event_type.clone()).or_insert(0) += 1;
    events.push(record);
}

fn to_btree(map: &HashMap<String, u64>) -> BTreeMap<String, u64> {
    map.iter()
        .map(|(key, value)| (key.clone(), *value))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::ReplayedRunStream;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    use serde_json::Value;
    use tempfile::tempdir;

    #[test]
    fn replay_all_operation_emulation_matches_expected_replay_counts() {
        let run_dir = fixture_run_dir(
            "target/manual-smoke/.codex-worker/tasks/all-operation-emulation--07f4203b/runs/20260406T145552Z--18a3cc56f6037e36-1",
        );
        let replayed = ReplayedRunStream::replay_run_dir(&run_dir).expect("replay should succeed");

        assert_common_event_counts(
            &replayed.event_counts,
            &[
                ("message.agent", 9),
                ("agent.completed", 1),
                ("agent.started", 1),
                ("file.change", 4),
                ("stderr.line", 1),
                ("thread.started", 1),
                ("todo.update", 4),
                ("web.search", 32),
                ("mcp.call", 3),
                ("mcp.result", 3),
            ],
            7,
            7,
        );
        assert_eq!(
            replayed.tool_counts,
            summary_counts(&run_dir.join("summary.json"), "tool_counts")
        );
        assert_eq!(
            replayed.subagent_counts,
            summary_counts(&run_dir.join("summary.json"), "subagent_counts")
        );
        assert_eq!(
            replayed.root_thread_id.as_deref(),
            Some("019d634a-ff70-72d2-bc0e-fc1a43c51f85")
        );
        assert!(replayed.subagent_threads.is_empty());
    }

    #[test]
    fn replay_all_operation_emulation_on_sub_agent_matches_expected_replay_counts() {
        let run_dir = fixture_run_dir(
            "target/manual-smoke/.codex-worker/tasks/all-operation-emulation-on-sub-agent--71f2caf3/runs/20260406T145909Z--18a3cc84d99c426e-2",
        );
        let replayed = ReplayedRunStream::replay_run_dir(&run_dir).expect("replay should succeed");

        assert_common_event_counts(
            &replayed.event_counts,
            &[
                ("message.agent", 17),
                ("message.commentary", 12),
                ("message.user", 3),
                ("task.started", 3),
                ("task.completed", 2),
                ("runtime.context", 3),
                ("info.tokens", 11),
                ("agent.reasoning", 14),
                ("agent.completed", 1),
                ("agent.session", 1),
                ("agent.started", 1),
                ("file.change", 4),
                ("thread.started", 1),
                ("todo.update", 3),
                ("web.search", 28),
                ("web.open", 2),
                ("mcp.call", 4),
                ("mcp.result", 4),
            ],
            17,
            17,
        );
        assert_eq!(
            replayed.tool_counts,
            summary_counts(&run_dir.join("summary.json"), "tool_counts")
        );
        assert_eq!(
            replayed.subagent_counts,
            summary_counts(&run_dir.join("summary.json"), "subagent_counts")
        );
        assert_eq!(replayed.subagent_threads.len(), 1);
        assert!(replayed
            .subagent_threads
            .contains("019d634f-b421-7611-832a-1a3b8070d0d3"));
        assert_eq!(replayed.event_counts.get("raw.unparsed").copied(), None);
    }

    #[test]
    fn missing_subagent_session_file_is_best_effort() {
        let temp = tempdir().expect("temp dir should be created");
        let task_dir = temp.path().join("task-missing-subagent");
        let run_dir = task_dir.join("runs").join("20260406T000000Z--run-1");
        let subagents_dir = run_dir.join("subagents");
        fs::create_dir_all(&subagents_dir).expect("run dir should be created");

        fs::write(
            task_dir.join("task.json"),
            r#"{"task_id":"task-missing-subagent"}"#,
        )
        .expect("task.json should be written");
        fs::write(
            run_dir.join("stdout.jsonl"),
            concat!(
                r#"{"type":"thread.started","thread_id":"root-thread"}"#,
                "\n",
                r#"{"type":"item.completed","item":{"type":"collab_tool_call","id":"spawn-1","tool":"spawn_agent","status":"completed","sender_thread_id":"root-thread","receiver_thread_ids":["sub-missing"],"prompt":"demo","agents_states":{"sub-missing":{"status":"running"}}}}"#,
                "\n"
            ),
        )
        .expect("stdout should be written");
        fs::write(run_dir.join("stderr.log"), "").expect("stderr should be written");

        let replayed = ReplayedRunStream::replay_run_dir(&run_dir).expect("replay should succeed");

        assert_eq!(replayed.root_thread_id.as_deref(), Some("root-thread"));
        assert!(replayed.subagent_threads.contains("sub-missing"));
        assert!(replayed
            .events
            .iter()
            .any(|event| event.event_type == "thread.started"));
        assert!(replayed
            .events
            .iter()
            .any(|event| event.event_type == "collab.spawn_agent"));
    }

    fn fixture_run_dir(relative: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
    }

    fn assert_common_event_counts(
        actual: &BTreeMap<String, u64>,
        stable_counts: &[(&str, u64)],
        expected_call_total: u64,
        expected_result_total: u64,
    ) {
        for (event_type, expected) in stable_counts {
            assert_eq!(actual.get(*event_type).copied(), Some(*expected));
        }

        let total_calls = actual.get("tool.call").copied().unwrap_or(0)
            + actual.get("shell.call").copied().unwrap_or(0)
            + actual.get("mcp.call").copied().unwrap_or(0);
        let total_results = actual.get("tool.result").copied().unwrap_or(0)
            + actual.get("shell.result").copied().unwrap_or(0)
            + actual.get("mcp.result").copied().unwrap_or(0);

        assert_eq!(total_calls, expected_call_total);
        assert_eq!(total_results, expected_result_total);
    }

    fn summary_counts(path: &Path, field: &str) -> BTreeMap<String, u64> {
        let payload: Value =
            serde_json::from_str(&std::fs::read_to_string(path).expect("summary")).expect("json");
        payload
            .get(field)
            .and_then(Value::as_object)
            .map(|object| {
                object
                    .iter()
                    .map(|(key, value)| (key.clone(), value.as_u64().unwrap_or_default()))
                    .collect()
            })
            .unwrap_or_default()
    }
}
