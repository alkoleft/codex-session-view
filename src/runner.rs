use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
use crate::ui::console::WorkerConsole;
use crate::util::utc_now_iso;

static RUN_COUNTER: AtomicU64 = AtomicU64::new(1);

enum StreamEvent {
    StdoutLine(String),
    StderrLine(String),
    StdoutDone,
    StderrDone,
    StdoutError(String),
    StderrError(String),
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
            let event = match rx.recv() {
                Ok(event) => event,
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
                }
                StreamEvent::StdoutDone => {
                    stdout_done = true;
                }
                StreamEvent::StderrDone => {
                    stderr_done = true;
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
            write_event(&paths.raw_unparsed_dir.join("main.jsonl"), event)?;
            write_event(&paths.problem_examples_dir.join("main.jsonl"), event)?;
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
