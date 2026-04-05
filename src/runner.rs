use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chrono::{Datelike, Duration as ChronoDuration, Utc};
use regex::Regex;
use serde_json::{Map, Value};

use crate::events::readers::{JsonOutputEventReader, RunEventContext};
use crate::events::record::EventRecord;
use crate::error::{AppError, AppResult};
use crate::lockfile::TaskFileLock;
use crate::logs::{append_task_run, update_task_snapshot, write_event, write_summary, RunPaths};
use crate::models::{ClaimedTask, RunSummary, TaskBlock, WorkerConfig};
use crate::task_file::{
    archive_snapshot_if_completed, claim_next_task, finalize_task, load_snapshot, recover_stale_tasks,
    refresh_heartbeat, write_snapshot_atomic, TaskFileError,
};
use crate::util::normalize_path;
use crate::ui::console::WorkerConsole;
use crate::util::utc_now_iso;

static RUN_COUNTER: AtomicU64 = AtomicU64::new(1);
const SUBAGENT_SESSION_LOOKUP_RETRY_SECONDS: f64 = 1.0;
const RECENT_SUBAGENT_SESSION_DAY_WINDOW: usize = 3;

enum StreamEvent {
    StdoutLine(String),
    StderrLine(String),
    StdoutDone,
    StderrDone,
    StdoutError(String),
    StderrError(String),
}

#[derive(Debug, Default)]
struct SubagentSessionTail {
    thread_id: String,
    imported_path: PathBuf,
    session_path: Option<PathBuf>,
    read_cursor: u64,
    partial_line: String,
    announced: bool,
    missing_announced: bool,
    call_names: HashMap<String, String>,
    last_lookup_at: Option<Instant>,
}

#[derive(Debug)]
pub struct CodexWorker {
    pub config: WorkerConfig,
    worker_id: String,
    ui: Option<WorkerConsole>,
}

impl CodexWorker {
    pub fn new(config: WorkerConfig) -> Self {
        let worker_id = format!(
            "worker-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );
        let ui = if config.log_to_stdout {
            Some(WorkerConsole::new(config.clone()))
        } else {
            None
        };
        Self { config, worker_id, ui }
    }

    pub fn run_next(&mut self) -> AppResult<i32> {
        if let Some(ui) = self.ui.as_mut() {
            let _ = ui.start();
        }

        if self.config.dry_run {
            let result = Ok(0);
            if let Some(ui) = self.ui.as_mut() {
                let _ = ui.finish();
            }
            return result;
        }

        let result = (|| {
            let mut processed_any = false;
            loop {
                let claimed = self.claim_next_task()?;
                let Some(claimed) = claimed else {
                    if !processed_any {
                        self.emit("idle", &format!("no pending tasks in {}", self.config.task_file.display()));
                    }
                    return Ok(0);
                };
                processed_any = true;
                let exit_code = self.execute_claimed_task(&claimed)?;
                if exit_code != 0 {
                    return Ok(exit_code);
                }
            }
        })();

        if let Some(ui) = self.ui.as_mut() {
            let _ = ui.finish();
        }
        result
    }

    fn claim_next_task(&mut self) -> AppResult<Option<ClaimedTask>> {
        let task_file = self.config.task_file.clone();
        if !task_file.exists() {
            return Err(AppError::Runner(format!(
                "Task file was not found.\nPath: {}\nPass an existing markdown file via --task-file.",
                task_file.display()
            )));
        }
        if let Some(parent) = task_file.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut lock = TaskFileLock::new(&task_file);
        lock.acquire().map_err(lock_err)?;
        lock.write_payload(&self.worker_id).map_err(lock_err)?;

        let mut snapshot = load_snapshot(&task_file).map_err(task_err)?;
        let (recovered_content, recovered_ids) =
            recover_stale_tasks(&snapshot, self.config.stale_after).map_err(task_err)?;
        if !recovered_ids.is_empty() {
            self.emit(
                "recovery",
                &format!("marked stale tasks as failed: {}", recovered_ids.join(", ")),
            );
            write_snapshot_atomic(&task_file, &recovered_content).map_err(task_err)?;
            snapshot = load_snapshot(&task_file).map_err(task_err)?;
        }

        let run_id = next_run_id();
        let claimed = claim_next_task(&snapshot, &run_id, &self.worker_id).map_err(task_err)?;
        let Some((new_content, task)) = claimed else {
            lock.release().map_err(lock_err)?;
            return Ok(None);
        };
        write_snapshot_atomic(&task_file, &new_content).map_err(task_err)?;

        let latest_snapshot = load_snapshot(&task_file).map_err(task_err)?;
        let fresh_task = latest_snapshot
            .tasks
            .iter()
            .find(|item| item.task_id().ok() == task.task_id().ok())
            .cloned()
            .unwrap_or(task);
        lock.release().map_err(lock_err)?;
        if let Some(ui) = self.ui.as_mut() {
            let _ = ui.on_claim(&fresh_task, &run_id);
        }

        Ok(Some(ClaimedTask {
            task: fresh_task,
            run_id,
            worker_id: self.worker_id.clone(),
            worker_pid: std::process::id(),
            snapshot: latest_snapshot,
        }))
    }

    fn execute_claimed_task(&mut self, claimed: &ClaimedTask) -> AppResult<i32> {
        let started_at = utc_now_iso();
        let logs_root = self
            .config
            .logs_dir
            .clone()
            .unwrap_or_else(|| claimed.snapshot.path.parent().unwrap_or(Path::new(".")).join(".codex-worker"));
        let paths = RunPaths::new(&logs_root, &claimed.task, &claimed.run_id, &started_at);
        paths.ensure()?;

        let prompt = self.build_prompt(&claimed.task)?;
        std::fs::write(&paths.prompt, prompt.as_bytes())?;
        if let Some(ui) = self.ui.as_mut() {
            let _ = ui.on_start(&claimed.task, &claimed.run_id);
        }

        let mut summary = RunSummary::default();
        summary.task_id = claimed.task.task_id().unwrap_or_default().to_string();
        summary.run_id = claimed.run_id.clone();
        summary.started_at = started_at.clone();
        summary.paths = BTreeMap::from([
            ("prompt".to_string(), paths.prompt.display().to_string()),
            ("stdout".to_string(), paths.stdout.display().to_string()),
            ("stderr".to_string(), paths.stderr.display().to_string()),
            ("events".to_string(), paths.events.display().to_string()),
            ("summary".to_string(), paths.summary.display().to_string()),
            (
                "subagents_dir".to_string(),
                paths.subagents_dir.display().to_string(),
            ),
            (
                "raw_unparsed_dir".to_string(),
                paths.raw_unparsed_dir.display().to_string(),
            ),
            (
                "problem_examples_dir".to_string(),
                paths.problem_examples_dir.display().to_string(),
            ),
        ]);

        let mut command = self.build_codex_command(&claimed.task);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(err) => {
                summary.finished_at = utc_now_iso();
                summary.status = "failed".to_string();
                summary.failure_reason = Some("process_spawn_failed".to_string());
                summary.failure_analysis = Value::Object(Map::from_iter([(
                    "spawn_error".to_string(),
                    Value::from(err.to_string()),
                )]));

                write_summary(&paths.summary, &summary)?;
                append_task_run(
                    &paths.runs_jsonl,
                    &Value::Object(Map::from_iter([
                        ("task_id".to_string(), Value::from(summary.task_id.clone())),
                        ("run_id".to_string(), Value::from(summary.run_id.clone())),
                        ("status".to_string(), Value::from(summary.status.clone())),
                        (
                            "failure_reason".to_string(),
                            summary
                                .failure_reason
                                .clone()
                                .map(Value::from)
                                .unwrap_or(Value::Null),
                        ),
                        ("started_at".to_string(), Value::from(summary.started_at.clone())),
                        ("finished_at".to_string(), Value::from(summary.finished_at.clone())),
                        ("summary_path".to_string(), Value::from(paths.summary.display().to_string())),
                    ])),
                )?;

                let task_file_path = self.finalize_task(
                    claimed,
                    "!",
                    "process_spawn_failed",
                    &format!("failed to spawn codex process: {err}"),
                )?;
                let latest = load_snapshot(&task_file_path).map_err(task_err)?;
                if let Some(final_task) = latest
                    .tasks
                    .iter()
                    .find(|item| item.task_id().ok() == Some(summary.task_id.as_str()))
                {
                    update_task_snapshot(
                        &paths.task_json,
                        final_task,
                        &paths.summary.display().to_string(),
                    )?;
                }
                if let Some(ui) = self.ui.as_mut() {
                    let _ = ui.on_result(
                        &summary.task_id,
                        &claimed.task.title,
                        &summary.run_id,
                        "failed",
                        Some("process_spawn_failed"),
                    );
                }

                return Ok(1);
            }
        };

        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(prompt.as_bytes())?;
        }
        drop(child.stdin.take());

        let task_id = claimed.task.task_id().unwrap_or_default().to_string();
        let task_file_path = self.config.task_file.clone();
        let heartbeat_stop = Arc::new(AtomicBool::new(false));
        let heartbeat_error = Arc::new(Mutex::new(None::<String>));
        let heartbeat_thread = self.start_heartbeat_loop(
            task_file_path,
            task_id.clone(),
            self.worker_id.clone(),
            heartbeat_stop.clone(),
            heartbeat_error.clone(),
        );

        let mut event_counts: HashMap<String, u64> = HashMap::new();
        let mut tool_counts: HashMap<String, u64> = HashMap::new();
        let mut subagent_counts: HashMap<String, u64> = HashMap::new();
        let mut subagent_threads: HashSet<String> = HashSet::new();
        let mut session_tails: HashMap<String, SubagentSessionTail> = HashMap::new();

        let mut reader = JsonOutputEventReader::new(
            RunEventContext {
                task_id: summary.task_id.clone(),
                run_id: summary.run_id.clone(),
            },
            None,
        );

        let mut seq = 0u64;
        let mut stdout_handle = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&paths.stdout)?;
        let mut stderr_handle = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&paths.stderr)?;
        let mut stderr_text = String::new();

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| AppError::Runner("child stdout is not piped".to_string()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| AppError::Runner("child stderr is not piped".to_string()))?;

        let (tx, rx) = mpsc::channel::<StreamEvent>();
        let stdout_tx = tx.clone();
        let stdout_thread = thread::spawn(move || {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => {
                        let _ = stdout_tx.send(StreamEvent::StdoutDone);
                        break;
                    }
                    Ok(_) => {
                        if stdout_tx
                            .send(StreamEvent::StdoutLine(line.clone()))
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(err) => {
                        let _ = stdout_tx.send(StreamEvent::StdoutError(err.to_string()));
                        break;
                    }
                }
            }
        });
        let stderr_thread = thread::spawn(move || {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();
            loop {
                line.clear();
                match reader.read_line(&mut line) {
                    Ok(0) => {
                        let _ = tx.send(StreamEvent::StderrDone);
                        break;
                    }
                    Ok(_) => {
                        if tx.send(StreamEvent::StderrLine(line.clone())).is_err() {
                            break;
                        }
                    }
                    Err(err) => {
                        let _ = tx.send(StreamEvent::StderrError(err.to_string()));
                        break;
                    }
                }
            }
        });

        let mut stdout_done = false;
        let mut stderr_done = false;
        let mut stream_error: Option<String> = None;
        while !(stdout_done && stderr_done) {
            let event = match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(event) => event,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    seq = self.sync_subagent_sessions(
                        claimed,
                        &paths,
                        seq,
                        &mut event_counts,
                        &mut tool_counts,
                        &mut subagent_counts,
                        &subagent_threads,
                        reader.state.thread_id.as_deref().unwrap_or(""),
                        &mut session_tails,
                        false,
                    )?;
                    continue;
                }
                Err(err) => {
                    stream_error = Some(format!(
                        "stream reader channel closed unexpectedly: {err}"
                    ));
                    break;
                }
            };
            match event {
                StreamEvent::StdoutLine(line) => {
                    stdout_handle.write_all(line.as_bytes())?;
                    let (next_seq, parsed_event) = reader.parse_main_output_line(
                        seq,
                        &line,
                        &mut tool_counts,
                        &mut subagent_counts,
                        &mut subagent_threads,
                    );
                    seq = next_seq;
                    *event_counts.entry(parsed_event.event_type.clone()).or_insert(0) += 1;
                    let record = parsed_event.to_record();
                    write_event(&paths.events, &record)?;
                    self.maybe_write_problem_examples(&paths, &record)?;
                    self.emit_event(&record);
                    seq = self.sync_subagent_sessions(
                        claimed,
                        &paths,
                        seq,
                        &mut event_counts,
                        &mut tool_counts,
                        &mut subagent_counts,
                        &subagent_threads,
                        reader.state.thread_id.as_deref().unwrap_or(""),
                        &mut session_tails,
                        false,
                    )?;
                }
                StreamEvent::StderrLine(line) => {
                    stderr_handle.write_all(line.as_bytes())?;
                    stderr_text.push_str(&line);
                    seq += 1;
                    let record = EventRecord {
                        schema_version: 1,
                        ts: utc_now_iso(),
                        task_id: summary.task_id.clone(),
                        run_id: summary.run_id.clone(),
                        seq,
                        event_type: "stderr.line".to_string(),
                        raw_type: "stderr".to_string(),
                        parse_status: "parsed".to_string(),
                        payload: Value::Object(Map::from_iter([
                            ("actor_type".to_string(), Value::from("system")),
                            (
                                "text".to_string(),
                                Value::from(line.trim_end_matches('\n').to_string()),
                            ),
                        ])),
                    };
                    *event_counts.entry(record.event_type.clone()).or_insert(0) += 1;
                    write_event(&paths.events, &record)?;
                    self.emit("stderr", line.trim_end_matches('\n'));
                    seq = self.sync_subagent_sessions(
                        claimed,
                        &paths,
                        seq,
                        &mut event_counts,
                        &mut tool_counts,
                        &mut subagent_counts,
                        &subagent_threads,
                        reader.state.thread_id.as_deref().unwrap_or(""),
                        &mut session_tails,
                        false,
                    )?;
                }
                StreamEvent::StdoutDone => {
                    stdout_done = true;
                    seq = self.sync_subagent_sessions(
                        claimed,
                        &paths,
                        seq,
                        &mut event_counts,
                        &mut tool_counts,
                        &mut subagent_counts,
                        &subagent_threads,
                        reader.state.thread_id.as_deref().unwrap_or(""),
                        &mut session_tails,
                        false,
                    )?;
                }
                StreamEvent::StderrDone => {
                    stderr_done = true;
                    seq = self.sync_subagent_sessions(
                        claimed,
                        &paths,
                        seq,
                        &mut event_counts,
                        &mut tool_counts,
                        &mut subagent_counts,
                        &subagent_threads,
                        reader.state.thread_id.as_deref().unwrap_or(""),
                        &mut session_tails,
                        false,
                    )?;
                }
                StreamEvent::StdoutError(err) => {
                    stream_error = Some(format!("stdout read failed: {err}"));
                    break;
                }
                StreamEvent::StderrError(err) => {
                    stream_error = Some(format!("stderr read failed: {err}"));
                    break;
                }
            }
        }
        let _ = stdout_thread.join();
        let _ = stderr_thread.join();

        let proc_status = child.wait()?;
        let exit_code = proc_status.code();
        heartbeat_stop.store(true, Ordering::Relaxed);
        let _ = heartbeat_thread.join();
        self.sync_subagent_sessions(
            claimed,
            &paths,
            seq,
            &mut event_counts,
            &mut tool_counts,
            &mut subagent_counts,
            &subagent_threads,
            reader.state.thread_id.as_deref().unwrap_or(""),
            &mut session_tails,
            true,
        )?;
        let heartbeat_failure = heartbeat_error
            .lock()
            .ok()
            .and_then(|guard| guard.clone());

        summary.finished_at = utc_now_iso();
        summary.exit_code = exit_code;
        summary.event_counts = map_to_btree(&event_counts);
        summary.tool_counts = map_to_btree(&tool_counts);
        summary.subagent_counts = map_to_btree(&subagent_counts);

        let failure_analysis = self.analyze_failure(&stderr_text, &paths.problem_examples_dir.join("main.jsonl"));
        let failure_reason = if let Some(stream_err) = stream_error.clone() {
            summary.failure_analysis = Value::Object(Map::from_iter([(
                "stream_error".to_string(),
                Value::from(stream_err),
            )]));
            Some("process_read_error".to_string())
        } else if let Some(heartbeat_err) = heartbeat_failure {
            summary.failure_analysis = Value::Object(Map::from_iter([(
                "heartbeat_error".to_string(),
                Value::from(heartbeat_err),
            )]));
            Some("heartbeat_failed".to_string())
        } else if !failure_analysis.is_null() {
            Some("connection_error".to_string())
        } else if !reader.state.valid_json
            || !reader.state.saw_thread_started
            || !reader.state.saw_turn_completed
            || reader.state.saw_turn_failed
            || reader.state.saw_top_error
            || reader.state.last_terminal.as_deref() != Some("turn.completed")
            || reader.state.final_agent_message.as_deref().unwrap_or("").is_empty()
        {
            Some("runtime_output_invalid".to_string())
        } else if exit_code.is_none() || exit_code.is_some_and(|code| code != 0) {
            Some("process_exit_nonzero".to_string())
        } else {
            None
        };

        let (finalize_status, finalize_result, finalize_error, exit) = if let Some(reason) = failure_reason.clone() {
            summary.status = "failed".to_string();
            summary.failure_reason = Some(reason.clone());
            if summary.failure_analysis.is_null()
                || summary.failure_analysis.as_object().is_some_and(|v| v.is_empty())
            {
                summary.failure_analysis = failure_analysis;
            }
            ("!".to_string(), reason, summary.failure_reason.clone().unwrap_or_default(), 1)
        } else {
            summary.status = "completed".to_string();
            summary.failure_analysis = Value::Object(Map::new());
            ("x".to_string(), "success".to_string(), String::new(), 0)
        };

        write_summary(&paths.summary, &summary)?;
        append_task_run(
            &paths.runs_jsonl,
            &Value::Object(Map::from_iter([
                ("task_id".to_string(), Value::from(summary.task_id.clone())),
                ("run_id".to_string(), Value::from(summary.run_id.clone())),
                ("status".to_string(), Value::from(summary.status.clone())),
                (
                    "failure_reason".to_string(),
                    summary
                        .failure_reason
                        .clone()
                        .map(Value::from)
                        .unwrap_or(Value::Null),
                ),
                ("started_at".to_string(), Value::from(summary.started_at.clone())),
                ("finished_at".to_string(), Value::from(summary.finished_at.clone())),
                ("summary_path".to_string(), Value::from(paths.summary.display().to_string())),
            ])),
        )?;

        let task_file_path = self.finalize_task(claimed, &finalize_status, &finalize_result, &finalize_error)?;
        let latest = load_snapshot(&task_file_path).map_err(task_err)?;
        if let Some(final_task) = latest
            .tasks
            .iter()
            .find(|item| item.task_id().ok() == Some(summary.task_id.as_str()))
        {
            update_task_snapshot(&paths.task_json, final_task, &paths.summary.display().to_string())?;
        }
        if let Some(ui) = self.ui.as_mut() {
            let _ = ui.on_result(
                &summary.task_id,
                &claimed.task.title,
                &summary.run_id,
                &summary.status,
                summary.failure_reason.as_deref(),
            );
        }

        Ok(exit)
    }

    fn finalize_task(
        &mut self,
        claimed: &ClaimedTask,
        status: &str,
        result: &str,
        error: &str,
    ) -> AppResult<PathBuf> {
        let mut lock = TaskFileLock::new(&self.config.task_file);
        lock.acquire().map_err(lock_err)?;
        lock.heartbeat(&self.worker_id).map_err(lock_err)?;
        let snapshot = load_snapshot(&self.config.task_file).map_err(task_err)?;
        let new_content =
            finalize_task(&snapshot, claimed, status, result, error).map_err(task_err)?;
        write_snapshot_atomic(&self.config.task_file, &new_content).map_err(task_err)?;
        let archived =
            archive_snapshot_if_completed(&self.config.task_file, &new_content).map_err(task_err)?;
        if let Some(path) = archived {
            self.config.task_file = path.clone();
        }
        lock.release().map_err(lock_err)?;
        Ok(self.config.task_file.clone())
    }

    fn start_heartbeat_loop(
        &self,
        task_file: PathBuf,
        task_id: String,
        worker_id: String,
        stop: Arc<AtomicBool>,
        error: Arc<Mutex<Option<String>>>,
    ) -> thread::JoinHandle<()> {
        let sleep_interval = heartbeat_interval(self.config.stale_after);
        thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                thread::sleep(sleep_interval);
                if stop.load(Ordering::Relaxed) {
                    break;
                }

                let result: AppResult<()> = (|| {
                    let mut lock = TaskFileLock::new(&task_file);
                    lock.acquire().map_err(lock_err)?;
                    lock.heartbeat(&worker_id).map_err(lock_err)?;
                    let snapshot = load_snapshot(&task_file).map_err(task_err)?;
                    let new_content =
                        refresh_heartbeat(&snapshot, &task_id, &worker_id).map_err(task_err)?;
                    write_snapshot_atomic(&task_file, &new_content).map_err(task_err)?;
                    lock.release().map_err(lock_err)?;
                    Ok(())
                })();

                if let Err(err) = result {
                    if let Ok(mut guard) = error.lock() {
                        if guard.is_none() {
                            *guard = Some(err.to_string());
                        }
                    }
                    break;
                }
            }
        })
    }

    fn build_prompt(&self, task: &TaskBlock) -> AppResult<String> {
        let template = if let Some(path) = &self.config.prompt_template {
            std::fs::read_to_string(path)?
        } else {
            "You are Codex worker.\nExecute the assigned task and return concise results.\n".to_string()
        };
        let extra_prompt = task
            .metadata
            .get("prompt")
            .map(|v| v.trim().to_string())
            .unwrap_or_default();
        let mut metadata_lines: Vec<String> = task
            .metadata
            .iter()
            .filter(|(k, _)| k.as_str() != "prompt")
            .map(|(k, v)| format!("{k}: {v}"))
            .collect();
        metadata_lines.sort();

        let mut parts = vec![
            template.trim_end().to_string(),
            String::new(),
            "Task Metadata:".to_string(),
        ];
        parts.extend(metadata_lines);
        parts.push(String::new());
        if !extra_prompt.is_empty() {
            parts.push("Additional Instructions:".to_string());
            parts.push(extra_prompt);
            parts.push(String::new());
        }
        parts.push("Task Body:".to_string());
        parts.push(task.body.clone());
        parts.push(String::new());
        Ok(parts.join("\n"))
    }

    fn build_codex_command(&self, task: &TaskBlock) -> Command {
        let mut cmd = Command::new(&self.config.codex_bin);
        cmd.arg("exec");
        if let Some(model) = task.metadata.get("model").or(self.config.model.as_ref()) {
            cmd.arg("--model").arg(model);
        }
        if let Some(sandbox) = task.metadata.get("sandbox").or(self.config.sandbox.as_ref()) {
            cmd.arg("--sandbox").arg(sandbox);
        }
        if let Some(ap) = task
            .metadata
            .get("approval_policy")
            .or(self.config.approval_policy.as_ref())
        {
            cmd.arg("--ask-for-approval").arg(ap);
        }

        let cwd = resolve_task_cwd(task, self.config.default_cwd.as_ref());
        cmd.arg("--json")
            .arg("-C")
            .arg(cwd)
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd
    }

    fn maybe_write_problem_examples(&self, paths: &RunPaths, event: &EventRecord) -> AppResult<()> {
        if event.event_type == "raw.unparsed" {
            let target = if event
                .payload
                .get("actor_type")
                .and_then(Value::as_str)
                == Some("subagent")
            {
                "subagents.jsonl"
            } else {
                "main.jsonl"
            };
            write_event(&paths.raw_unparsed_dir.join(target), event)?;
            write_event(&paths.problem_examples_dir.join(target), event)?;
        } else if event.event_type == "error" {
            write_event(&paths.problem_examples_dir.join("main.jsonl"), event)?;
        }
        Ok(())
    }

    fn analyze_failure(&self, stderr_text: &str, problem_examples_path: &Path) -> Value {
        let mut text = stderr_text.to_string();
        if let Ok(extra) = std::fs::read_to_string(problem_examples_path) {
            text.push('\n');
            text.push_str(&extra);
        }
        let lower = text.to_lowercase();
        if !(lower.contains("connect to websocket")
            || lower.contains("unexpected status")
            || lower.contains("http error")
            || lower.contains("reconnecting")
            || lower.contains("backend-api/codex/responses"))
        {
            return Value::Null;
        }

        let status = extract_status_code(&text);
        let endpoint = extract_endpoint(&text);
        let category = match status {
            Some(403) => "authz_forbidden",
            Some(401) => "authn_required",
            _ => "network_connect",
        };

        let mut analysis = Map::new();
        analysis.insert("failure_reason".to_string(), Value::from("connection_error"));
        analysis.insert("category".to_string(), Value::from(category));
        analysis.insert("stage".to_string(), Value::from("connect"));
        if let Some(code) = status {
            analysis.insert("http_status".to_string(), Value::from(code));
        }
        if let Some(ref url) = endpoint {
            analysis.insert("endpoint".to_string(), Value::from(url.clone()));
            analysis.insert(
                "endpoints".to_string(),
                Value::Array(vec![Value::from(url.clone())]),
            );
            if let Some(scheme) = url.split("://").next() {
                let transport = match scheme.to_ascii_lowercase().as_str() {
                    "wss" | "ws" => "websocket",
                    "https" | "http" => "https",
                    _ => "unknown",
                };
                analysis.insert("transport".to_string(), Value::from(transport));
            }
            if let Some(host) = host_from_url(url) {
                analysis.insert(
                    "hosts".to_string(),
                    Value::Array(vec![Value::from(host)]),
                );
            }
        }
        Value::Object(analysis)
    }

    fn sync_subagent_sessions(
        &mut self,
        claimed: &ClaimedTask,
        paths: &RunPaths,
        mut seq: u64,
        event_counts: &mut HashMap<String, u64>,
        tool_counts: &mut HashMap<String, u64>,
        subagent_counts: &mut HashMap<String, u64>,
        subagent_threads: &HashSet<String>,
        parent_thread_id: &str,
        session_tails: &mut HashMap<String, SubagentSessionTail>,
        final_pass: bool,
    ) -> AppResult<u64> {
        if subagent_threads.is_empty() {
            return Ok(seq);
        }
        let sessions_root = self.sessions_root().join("sessions");
        if !sessions_root.exists() {
            if final_pass {
                self.emit("subagent", &format!("sessions root not found: {}", sessions_root.display()));
            }
            return Ok(seq);
        }

        let mut thread_ids: Vec<String> = subagent_threads.iter().cloned().collect();
        thread_ids.sort();
        for thread_id in thread_ids {
            let tail = session_tails
                .entry(thread_id.clone())
                .or_insert_with(|| SubagentSessionTail {
                    thread_id: thread_id.clone(),
                    imported_path: paths.subagents_dir.join(format!("{thread_id}.jsonl")),
                    ..SubagentSessionTail::default()
                });

            if tail
                .session_path
                .as_ref()
                .is_none_or(|path| !path.exists())
            {
                let now = Instant::now();
                let should_lookup = final_pass
                    || tail.last_lookup_at.is_none()
                    || tail
                        .last_lookup_at
                        .is_some_and(|seen| seen.elapsed().as_secs_f64() >= SUBAGENT_SESSION_LOOKUP_RETRY_SECONDS);
                if should_lookup {
                    tail.last_lookup_at = Some(now);
                    tail.session_path =
                        self.find_session_file(&sessions_root, &thread_id, final_pass);
                }
            }

            if tail.session_path.is_none() {
                if final_pass && !tail.missing_announced {
                    self.emit("subagent", &format!("session missing thread_id={thread_id}"));
                    tail.missing_announced = true;
                }
                continue;
            }

            if !tail.announced {
                self.emit("subagent", &format!("session_imported thread_id={thread_id}"));
                tail.announced = true;
            }

            seq = self.tail_subagent_session(
                claimed,
                paths,
                seq,
                tail,
                event_counts,
                tool_counts,
                subagent_counts,
                parent_thread_id,
                final_pass,
            )?;
        }
        Ok(seq)
    }

    fn tail_subagent_session(
        &mut self,
        claimed: &ClaimedTask,
        paths: &RunPaths,
        mut seq: u64,
        tail: &mut SubagentSessionTail,
        event_counts: &mut HashMap<String, u64>,
        tool_counts: &mut HashMap<String, u64>,
        subagent_counts: &mut HashMap<String, u64>,
        parent_thread_id: &str,
        final_pass: bool,
    ) -> AppResult<u64> {
        let Some(session_path) = tail.session_path.clone() else {
            return Ok(seq);
        };

        let mut output_reader = JsonOutputEventReader::new(
            RunEventContext {
                task_id: claimed.task.task_id().unwrap_or_default().to_string(),
                run_id: claimed.run_id.clone(),
            },
            None,
        );

        if let Some(parent) = tail.imported_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut source = File::open(&session_path)?;
        source.seek(SeekFrom::Start(tail.read_cursor))?;
        let mut chunk = String::new();
        source.read_to_string(&mut chunk)?;
        tail.read_cursor = source.stream_position()?;
        if chunk.is_empty() && !final_pass {
            return Ok(seq);
        }

        let mut imported = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&tail.imported_path)?;
        if !chunk.is_empty() {
            imported.write_all(chunk.as_bytes())?;
        }

        let combined = format!("{}{}", tail.partial_line, chunk);
        let mut lines: Vec<String> = combined.lines().map(str::to_string).collect();
        if final_pass {
            tail.partial_line.clear();
        } else if combined.ends_with('\n') {
            tail.partial_line.clear();
        } else {
            tail.partial_line = lines.pop().unwrap_or(combined);
        }

        for stripped in lines {
            if stripped.is_empty() {
                continue;
            }
            seq += 1;
            let ts = utc_now_iso();
            let parsed = match serde_json::from_str::<Value>(&stripped) {
                Ok(Value::Object(obj)) => obj,
                _ => {
                    let event = EventRecord {
                        schema_version: 1,
                        ts,
                        task_id: claimed.task.task_id().unwrap_or_default().to_string(),
                        run_id: claimed.run_id.clone(),
                        seq,
                        event_type: "raw.unparsed".to_string(),
                        raw_type: "invalid_json".to_string(),
                        parse_status: "unparsed".to_string(),
                        payload: Value::Object(Map::from_iter([
                            ("actor_type".to_string(), Value::from("subagent")),
                            ("thread_id".to_string(), Value::from(tail.thread_id.clone())),
                            ("text".to_string(), Value::from(stripped.clone())),
                        ])),
                    };
                    write_event(&paths.events, &event)?;
                    self.maybe_write_problem_examples(paths, &event)?;
                    *event_counts.entry(event.event_type.clone()).or_insert(0) += 1;
                    self.emit_event(&event);
                    continue;
                }
            };

            let Some(codex_event) = output_reader.parse_subagent_session_payload(
                seq,
                &parsed,
                &tail.imported_path,
                parent_thread_id,
                &tail.thread_id,
                &mut tail.call_names,
                tool_counts,
                subagent_counts,
            ) else {
                seq -= 1;
                continue;
            };

            let event = codex_event.to_record();
            write_event(&paths.events, &event)?;
            self.maybe_write_problem_examples(paths, &event)?;
            *event_counts.entry(event.event_type.clone()).or_insert(0) += 1;
            self.emit_event(&event);
        }

        Ok(seq)
    }

    fn emit(&mut self, kind: &str, message: &str) {
        if let Some(ui) = self.ui.as_mut() {
            let _ = ui.on_emit(kind, message);
        }
    }

    fn emit_event(&mut self, event: &EventRecord) {
        if let Some(ui) = self.ui.as_mut() {
            let _ = ui.on_event(event);
        }
    }

    fn sessions_root(&self) -> PathBuf {
        if let Some(path) = &self.config.codex_home {
            return path.clone();
        }
        let raw = std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("~/.codex"));
        normalize_path(&raw)
    }

    fn find_session_file(
        &self,
        sessions_root: &Path,
        thread_id: &str,
        exhaustive: bool,
    ) -> Option<PathBuf> {
        let mut matches = Vec::new();
        for day_dir in self.candidate_session_dirs(sessions_root, exhaustive) {
            let entries = std::fs::read_dir(&day_dir).ok()?;
            for entry in entries.flatten() {
                let path = entry.path();
                let file_name = path.file_name().and_then(|value| value.to_str()).unwrap_or_default();
                if path.is_file() && file_name.ends_with(".jsonl") && file_name.contains(thread_id) {
                    matches.push(path);
                }
            }
            if !matches.is_empty() {
                matches.sort();
                return matches.pop();
            }
        }
        None
    }

    fn candidate_session_dirs(&self, sessions_root: &Path, exhaustive: bool) -> Vec<PathBuf> {
        if exhaustive {
            return self.iter_session_dirs(sessions_root);
        }

        let today = Utc::now().date_naive();
        let mut candidates = Vec::new();
        for offset in 0..RECENT_SUBAGENT_SESSION_DAY_WINDOW {
            let day = today - ChronoDuration::days(offset as i64);
            let day_dir = sessions_root
                .join(format!("{:04}", day.year()))
                .join(format!("{:02}", day.month()))
                .join(format!("{:02}", day.day()));
            if day_dir.is_dir() {
                candidates.push(day_dir);
            }
        }
        if !candidates.is_empty() {
            return candidates;
        }
        self.iter_session_dirs(sessions_root)
            .into_iter()
            .take(RECENT_SUBAGENT_SESSION_DAY_WINDOW)
            .collect()
    }

    fn iter_session_dirs(&self, sessions_root: &Path) -> Vec<PathBuf> {
        let mut result = Vec::new();
        let Ok(years) = std::fs::read_dir(sessions_root) else {
            return result;
        };
        let mut year_dirs: Vec<PathBuf> = years.flatten().map(|entry| entry.path()).filter(|path| path.is_dir()).collect();
        year_dirs.sort();
        year_dirs.reverse();
        for year_dir in year_dirs {
            let Ok(months) = std::fs::read_dir(&year_dir) else {
                continue;
            };
            let mut month_dirs: Vec<PathBuf> = months.flatten().map(|entry| entry.path()).filter(|path| path.is_dir()).collect();
            month_dirs.sort();
            month_dirs.reverse();
            for month_dir in month_dirs {
                let Ok(days) = std::fs::read_dir(&month_dir) else {
                    continue;
                };
                let mut day_dirs: Vec<PathBuf> = days.flatten().map(|entry| entry.path()).filter(|path| path.is_dir()).collect();
                day_dirs.sort();
                day_dirs.reverse();
                result.extend(day_dirs);
            }
        }
        result
    }
}

fn resolve_task_cwd(task: &TaskBlock, default_cwd: Option<&PathBuf>) -> PathBuf {
    let task_cwd = task.cwd().unwrap_or_default();
    if task_cwd.trim().is_empty() {
        return default_cwd
            .cloned()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    }
    let candidate = PathBuf::from(task_cwd);
    if candidate.is_absolute() {
        candidate
    } else {
        default_cwd
            .cloned()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
            .join(candidate)
    }
}

fn map_to_btree(map: &HashMap<String, u64>) -> BTreeMap<String, u64> {
    map.iter().map(|(k, v)| (k.clone(), *v)).collect()
}

fn next_run_id() -> String {
    let counter = RUN_COUNTER.fetch_add(1, Ordering::Relaxed);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{ts:x}-{counter:x}")
}

fn heartbeat_interval(stale_after_seconds: f64) -> Duration {
    let seconds = (stale_after_seconds / 2.0)
        .min(1.0)
        .min(stale_after_seconds * 0.9);
    Duration::from_secs_f64(seconds)
}

fn task_err(err: TaskFileError) -> AppError {
    AppError::Runner(format!("task file error: {err}"))
}

fn lock_err(err: crate::lockfile::LockfileError) -> AppError {
    AppError::Runner(format!("lock error: {err}"))
}

fn status_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(?:status|error:)\s*:?\s*(\d{3})\b")
            .expect("status regex must compile")
    })
}

fn url_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"(?i)\burl:\s*(https?://[^\s,)]+|wss?://[^\s,)]+)")
            .expect("url regex must compile")
    })
}

fn extract_status_code(text: &str) -> Option<u16> {
    status_regex()
        .captures(text)
        .and_then(|caps| caps.get(1))
        .and_then(|m| m.as_str().parse::<u16>().ok())
}

fn extract_endpoint(text: &str) -> Option<String> {
    let raw = url_regex()
        .captures(text)
        .and_then(|caps| caps.get(1))
        .map(|m| m.as_str().to_string())?;
    Some(trim_endpoint(&raw))
}

fn host_from_url(url: &str) -> Option<String> {
    let host_port = url.split("://").nth(1)?.split('/').next()?.trim();
    if host_port.is_empty() {
        return None;
    }
    Some(
        host_port
            .split(':')
            .next()
            .unwrap_or(host_port)
            .to_string(),
    )
}

fn trim_endpoint(value: &str) -> String {
    let mut out = value.trim().to_string();
    loop {
        let next = out
            .trim_end_matches(|ch: char| matches!(ch, '"' | '\'' | ')' | ']' | ',' | '.' | ';'))
            .to_string();
        if next == out {
            break;
        }
        out = next;
    }
    out
}

#[allow(dead_code)]
fn _heartbeat_probe(snapshot: &crate::models::TaskFileSnapshot, task_id: &str, worker_id: &str) -> AppResult<String> {
    refresh_heartbeat(snapshot, task_id, worker_id).map_err(task_err)
}

#[cfg(test)]
mod tests {
    use super::heartbeat_interval;

    #[test]
    fn heartbeat_interval_is_below_stale_threshold_for_small_values() {
        let stale_after = 0.3;
        let interval = heartbeat_interval(stale_after);
        assert!(interval.as_secs_f64() < stale_after);
        assert!(interval.as_secs_f64() < 1.0);
    }
}
