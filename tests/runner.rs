#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use codex_worker_rs::models::WorkerConfig;
use codex_worker_rs::runner::CodexWorker;
use serde_json::Value;

fn write_fake_codex(path: &Path, body: &str) {
    let script = format!(
        "#!/usr/bin/env bash\nset -eu\ncat >/dev/null\n{body}\n"
    );
    fs::write(path, script).expect("fake codex should be written");
    let mut perms = fs::metadata(path)
        .expect("fake codex should exist")
        .permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).expect("fake codex should be executable");
}

fn worker_config(task_file: &Path, codex_bin: &Path) -> WorkerConfig {
    WorkerConfig {
        task_file: task_file.to_path_buf(),
        codex_bin: codex_bin.display().to_string(),
        codex_home: None,
        logs_dir: None,
        default_cwd: None,
        model: None,
        sandbox: None,
        approval_policy: None,
        prompt_template: None,
        poll_interval: 2.0,
        stale_after: 30.0,
        dry_run: false,
        log_to_stdout: false,
    }
}

fn find_named_file(root: &Path, name: &str) -> Option<PathBuf> {
    let entries = fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_named_file(&path, name) {
                return Some(found);
            }
            continue;
        }
        if path.file_name().and_then(|n| n.to_str()) == Some(name) {
            return Some(path);
        }
    }
    None
}

fn find_path_ending_with(root: &Path, suffix: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_path_ending_with(&path, suffix) {
                return Some(found);
            }
            continue;
        }
        if path.ends_with(suffix) {
            return Some(path);
        }
    }
    None
}

fn read_task_content(worker: &CodexWorker, original_task_file: &Path) -> String {
    let path = if worker.config.task_file.exists() {
        worker.config.task_file.as_path()
    } else {
        original_task_file
    };
    fs::read_to_string(path).expect("task file should be readable")
}

fn metadata_value(content: &str, key: &str) -> Option<String> {
    content
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}: ")))
        .map(str::trim)
        .map(str::to_string)
}

#[test]
fn cli_missing_task_file_is_reported_without_traceback() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let missing = tmp.path().join("missing-task.md");

    let output = Command::new(env!("CARGO_BIN_EXE_codex-worker"))
        .arg("run-next")
        .arg("--task-file")
        .arg(&missing)
        .arg("--quiet")
        .output()
        .expect("cli should execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be utf-8");
    assert!(stderr.contains("codex-worker error"));
    assert!(stderr.contains("Task file was not found."));
    assert!(stderr.contains("--task-file"));
    assert!(stderr.contains(&missing.display().to_string()));
    assert!(!stderr.contains("Traceback"));
}

#[test]
fn successful_run_completes_task_and_writes_logs() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let repo = tmp.path().join("repo");
    fs::create_dir_all(&repo).expect("repo dir should be created");
    let task_file = tmp.path().join("task.md");
    fs::write(
        &task_file,
        format!("# [ ] Demo\nid: demo\ncwd: {}\n\nImplement\n", repo.display()),
    )
    .expect("task file should be written");

    let fake = tmp.path().join("fake-codex");
    write_fake_codex(
        &fake,
        r#"printf '%s\n' '{"type":"thread.started","thread_id":"t1"}'
printf '%s\n' '{"type":"turn.started"}'
printf '%s\n' '{"type":"item.completed","item":{"type":"tool_use","id":"a1","name":"spawn_agent","input":{"task":"review"}}}'
printf '%s\n' '{"type":"turn.completed"}'
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"done"}}'"#,
    );

    let mut worker = CodexWorker::new(worker_config(&task_file, &fake));
    let exit = worker.run_next().expect("run should succeed");
    assert_eq!(exit, 0);

    let content = read_task_content(&worker, &task_file);
    assert!(content.contains("## [x] Demo"));
    assert!(content.contains("last_result: success"));

    let logs_root = tmp.path().join(".codex-worker");
    let events = fs::read_to_string(
        find_named_file(&logs_root, "events.jsonl").expect("events.jsonl should exist"),
    )
    .expect("events should be readable");
    assert!(events.contains("\"event_type\":\"tool.call\""));

    let summary = fs::read_to_string(
        find_named_file(&logs_root, "summary.json").expect("summary.json should exist"),
    )
    .expect("summary should be readable");
    assert!(summary.contains("\"spawn_agent\": 1"));
}

#[test]
fn invalid_json_fails_closed() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let repo = tmp.path().join("repo");
    fs::create_dir_all(&repo).expect("repo dir should be created");
    let task_file = tmp.path().join("task.md");
    fs::write(
        &task_file,
        format!("# [ ] Demo\nid: demo\ncwd: {}\n\nImplement\n", repo.display()),
    )
    .expect("task file should be written");

    let fake = tmp.path().join("fake-codex");
    write_fake_codex(
        &fake,
        r#"printf '%s\n' '{"type":"thread.started","thread_id":"t1"}'
printf '%s\n' '{bad json'"#,
    );

    let mut worker = CodexWorker::new(worker_config(&task_file, &fake));
    let exit = worker.run_next().expect("run should finish");
    assert_eq!(exit, 1);

    let content = read_task_content(&worker, &task_file);
    assert!(content.contains("## [!] Demo"));
    assert!(content.contains("last_result: runtime_output_invalid"));

    let logs_root = tmp.path().join(".codex-worker");
    let raw_unparsed = fs::read_to_string(
        find_path_ending_with(&logs_root, Path::new("raw_unparsed/main.jsonl"))
            .expect("raw_unparsed/main.jsonl should exist"),
    )
    .expect("raw_unparsed should be readable");
    assert!(raw_unparsed.contains("\"event_type\":\"raw.unparsed\""));
}

#[test]
fn connection_failure_is_classified_in_summary() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let repo = tmp.path().join("repo");
    fs::create_dir_all(&repo).expect("repo dir should be created");
    let task_file = tmp.path().join("task.md");
    fs::write(
        &task_file,
        format!("# [ ] Demo\nid: demo\ncwd: {}\n\nImplement\n", repo.display()),
    )
    .expect("task file should be written");

    let fake = tmp.path().join("fake-codex");
    write_fake_codex(
        &fake,
        r#"printf '%s\n' '{"type":"thread.started","thread_id":"t1"}'
printf '%s\n' '{"type":"turn.started"}'
printf '%s\n' '{"type":"error","message":"Reconnecting... 2/5 (unexpected status 403 Forbidden, url: wss://chatgpt.com/backend-api/codex/responses)"}'
printf '%s\n' '2026-03-23T16:38:10Z ERROR codex_api::endpoint::responses_websocket: failed to connect to websocket: HTTP error: 403 Forbidden, url: wss://chatgpt.com/backend-api/codex/responses' 1>&2
exit 1"#,
    );

    let mut worker = CodexWorker::new(worker_config(&task_file, &fake));
    let exit = worker.run_next().expect("run should finish");
    assert_eq!(exit, 1);

    let logs_root = tmp.path().join(".codex-worker");
    let summary_path = find_named_file(&logs_root, "summary.json").expect("summary should exist");
    let summary: Value = serde_json::from_str(
        &fs::read_to_string(summary_path).expect("summary should be readable"),
    )
    .expect("summary should be valid json");
    assert_eq!(summary.get("failure_reason").and_then(Value::as_str), Some("connection_error"));
    assert_eq!(
        summary
            .get("failure_analysis")
            .and_then(Value::as_object)
            .and_then(|v| v.get("category"))
            .and_then(Value::as_str),
        Some("authz_forbidden")
    );
}

#[test]
fn connection_failure_401_is_classified_as_authn_required() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let repo = tmp.path().join("repo");
    fs::create_dir_all(&repo).expect("repo dir should be created");
    let task_file = tmp.path().join("task.md");
    fs::write(
        &task_file,
        format!("# [ ] Demo\nid: demo\ncwd: {}\n\nImplement\n", repo.display()),
    )
    .expect("task file should be written");

    let fake = tmp.path().join("fake-codex");
    write_fake_codex(
        &fake,
        r#"printf '%s\n' '{"type":"thread.started","thread_id":"t1"}'
printf '%s\n' '{"type":"turn.started"}'
printf '%s\n' '{"type":"error","message":"Reconnecting... 2/5 (unexpected status 401 Unauthorized, url: wss://chatgpt.com/backend-api/codex/responses)"}'
exit 1"#,
    );

    let mut worker = CodexWorker::new(worker_config(&task_file, &fake));
    let exit = worker.run_next().expect("run should finish");
    assert_eq!(exit, 1);

    let logs_root = tmp.path().join(".codex-worker");
    let summary_path = find_named_file(&logs_root, "summary.json").expect("summary should exist");
    let summary: Value = serde_json::from_str(
        &fs::read_to_string(summary_path).expect("summary should be readable"),
    )
    .expect("summary should be valid json");
    assert_eq!(
        summary
            .get("failure_analysis")
            .and_then(Value::as_object)
            .and_then(|v| v.get("category"))
            .and_then(Value::as_str),
        Some("authn_required")
    );
    assert_eq!(
        summary
            .get("failure_analysis")
            .and_then(Value::as_object)
            .and_then(|v| v.get("http_status"))
            .and_then(Value::as_u64),
        Some(401)
    );
}

#[test]
fn successful_run_archives_task_file_when_all_tasks_completed() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let repo = tmp.path().join("repo");
    fs::create_dir_all(&repo).expect("repo dir should be created");
    let task_file = tmp.path().join("task.md");
    fs::write(
        &task_file,
        format!("# [ ] Demo\nid: demo\ncwd: {}\n\nImplement\n", repo.display()),
    )
    .expect("task file should be written");

    let fake = tmp.path().join("fake-codex");
    write_fake_codex(
        &fake,
        r#"printf '%s\n' '{"type":"thread.started","thread_id":"t1"}'
printf '%s\n' '{"type":"turn.started"}'
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"done"}}'
printf '%s\n' '{"type":"turn.completed"}'"#,
    );

    let mut worker = CodexWorker::new(worker_config(&task_file, &fake));
    let exit = worker.run_next().expect("run should finish");
    assert_eq!(exit, 0);

    let archived_task_file = tmp.path().join("archive").join("task.md");
    assert!(!task_file.exists());
    assert!(archived_task_file.exists());
    assert_eq!(worker.config.task_file, archived_task_file);
}

#[test]
fn stream_read_error_finalizes_task_as_failed_and_writes_summary() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let repo = tmp.path().join("repo");
    fs::create_dir_all(&repo).expect("repo dir should be created");
    let task_file = tmp.path().join("task.md");
    fs::write(
        &task_file,
        format!("# [ ] Demo\nid: demo\ncwd: {}\n\nImplement\n", repo.display()),
    )
    .expect("task file should be written");

    let fake = tmp.path().join("fake-codex");
    write_fake_codex(&fake, "printf '\\377\\n'");

    let mut worker = CodexWorker::new(worker_config(&task_file, &fake));
    let exit = worker.run_next().expect("run should finish");
    assert_eq!(exit, 1);

    let content = read_task_content(&worker, &task_file);
    assert!(content.contains("## [!] Demo"));
    assert!(content.contains("last_result: process_read_error"));

    let logs_root = tmp.path().join(".codex-worker");
    let summary_path = find_named_file(&logs_root, "summary.json").expect("summary should exist");
    let summary: Value = serde_json::from_str(
        &fs::read_to_string(summary_path).expect("summary should be readable"),
    )
    .expect("summary should be valid json");
    assert_eq!(
        summary.get("failure_reason").and_then(Value::as_str),
        Some("process_read_error")
    );
}

#[test]
fn child_terminated_by_signal_is_failed_closed() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let repo = tmp.path().join("repo");
    fs::create_dir_all(&repo).expect("repo dir should be created");
    let task_file = tmp.path().join("task.md");
    fs::write(
        &task_file,
        format!("# [ ] Demo\nid: demo\ncwd: {}\n\nImplement\n", repo.display()),
    )
    .expect("task file should be written");

    let fake = tmp.path().join("fake-codex");
    write_fake_codex(
        &fake,
        r#"printf '%s\n' '{"type":"thread.started","thread_id":"t1"}'
printf '%s\n' '{"type":"turn.started"}'
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"done"}}'
printf '%s\n' '{"type":"turn.completed"}'
kill -TERM $$"#,
    );

    let mut worker = CodexWorker::new(worker_config(&task_file, &fake));
    let exit = worker.run_next().expect("run should finish");
    assert_eq!(exit, 1);

    let content = read_task_content(&worker, &task_file);
    assert!(content.contains("## [!] Demo"));
    assert!(content.contains("last_result: process_exit_nonzero"));

    let logs_root = tmp.path().join(".codex-worker");
    let summary_path = find_named_file(&logs_root, "summary.json").expect("summary should exist");
    let summary: Value = serde_json::from_str(
        &fs::read_to_string(summary_path).expect("summary should be readable"),
    )
    .expect("summary should be valid json");
    assert_eq!(
        summary.get("failure_reason").and_then(Value::as_str),
        Some("process_exit_nonzero")
    );
    assert!(summary.get("exit_code").is_some_and(Value::is_null));
}

#[test]
fn heartbeat_updates_lease_during_long_running_child() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let repo = tmp.path().join("repo");
    fs::create_dir_all(&repo).expect("repo dir should be created");
    let task_file = tmp.path().join("task.md");
    fs::write(
        &task_file,
        format!("# [ ] Demo\nid: demo\ncwd: {}\n\nImplement\n", repo.display()),
    )
    .expect("task file should be written");

    let fake = tmp.path().join("fake-codex");
    write_fake_codex(
        &fake,
        r#"sleep 3
printf '%s\n' '{"type":"thread.started","thread_id":"t1"}'
printf '%s\n' '{"type":"turn.started"}'
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"done"}}'
printf '%s\n' '{"type":"turn.completed"}'"#,
    );

    let worker = CodexWorker::new(worker_config(&task_file, &fake));
    let handle = thread::spawn(move || {
        let mut worker = worker;
        let exit = worker.run_next().expect("run should finish");
        (worker, exit)
    });

    let started = Instant::now();
    let mut heartbeat_observed = false;
    while started.elapsed() < Duration::from_secs(5) {
        if let Ok(content) = fs::read_to_string(&task_file) {
            if content.contains("## [>] Demo") {
                let started_at = metadata_value(&content, "last_started_at");
                let heartbeat_at = metadata_value(&content, "last_heartbeat_at");
                if started_at.is_some() && heartbeat_at.is_some() && started_at != heartbeat_at {
                    heartbeat_observed = true;
                    break;
                }
            }
        }
        thread::sleep(Duration::from_millis(100));
    }

    let (_worker, exit) = handle.join().expect("runner thread should join");
    assert_eq!(exit, 0);
    assert!(
        heartbeat_observed,
        "last_heartbeat_at should diverge from last_started_at while task is running"
    );
}

#[test]
fn heartbeat_interval_tracks_small_stale_after() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let repo = tmp.path().join("repo");
    fs::create_dir_all(&repo).expect("repo dir should be created");
    let task_file = tmp.path().join("task.md");
    fs::write(
        &task_file,
        format!("# [ ] Demo\nid: demo\ncwd: {}\n\nImplement\n", repo.display()),
    )
    .expect("task file should be written");

    let fake = tmp.path().join("fake-codex");
    write_fake_codex(
        &fake,
        r#"sleep 2
printf '%s\n' '{"type":"thread.started","thread_id":"t1"}'
printf '%s\n' '{"type":"turn.started"}'
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"done"}}'
printf '%s\n' '{"type":"turn.completed"}'"#,
    );

    let mut config = worker_config(&task_file, &fake);
    config.stale_after = 0.3;
    let worker = CodexWorker::new(config);
    let handle = thread::spawn(move || {
        let mut worker = worker;
        let exit = worker.run_next().expect("run should finish");
        (worker, exit)
    });

    let running_deadline = Instant::now() + Duration::from_secs(2);
    let mut baseline = None;
    while Instant::now() < running_deadline {
        if let Ok(content) = fs::read_to_string(&task_file) {
            if content.contains("## [>] Demo") {
                let modified = fs::metadata(&task_file)
                    .expect("task file metadata should be readable")
                    .modified()
                    .expect("modified time should be readable");
                baseline = Some((Instant::now(), modified));
                break;
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
    let (baseline_at, baseline_mtime) = baseline.expect("task should enter running state");

    let mut heartbeat_elapsed = None;
    while baseline_at.elapsed() < Duration::from_millis(900) {
        if let Ok(content) = fs::read_to_string(&task_file) {
            if content.contains("## [>] Demo") {
                let modified = fs::metadata(&task_file)
                    .expect("task file metadata should be readable")
                    .modified()
                    .expect("modified time should be readable");
                if modified > baseline_mtime {
                    heartbeat_elapsed = Some(baseline_at.elapsed());
                    break;
                }
            }
        }
        thread::sleep(Duration::from_millis(20));
    }

    let (_worker, exit) = handle.join().expect("runner thread should join");
    assert_eq!(exit, 0);
    assert!(
        heartbeat_elapsed.is_some(),
        "expected heartbeat write before stale threshold window"
    );
}

#[test]
fn dry_run_does_not_execute_or_mutate_task_or_create_artifacts() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let repo = tmp.path().join("repo");
    fs::create_dir_all(&repo).expect("repo dir should be created");
    let task_file = tmp.path().join("task.md");
    let original = format!("# [ ] Demo\nid: demo\ncwd: {}\n\nImplement\n", repo.display());
    fs::write(&task_file, &original).expect("task file should be written");

    let fake = tmp.path().join("fake-codex");
    let marker = tmp.path().join("codex-ran.marker");
    write_fake_codex(
        &fake,
        &format!("touch \"{}\"", marker.display()),
    );

    let mut config = worker_config(&task_file, &fake);
    config.dry_run = true;

    let mut worker = CodexWorker::new(config);
    let exit = worker.run_next().expect("dry run should succeed");
    assert_eq!(exit, 0);
    assert_eq!(
        fs::read_to_string(&task_file).expect("task file should remain readable"),
        original
    );
    assert!(!marker.exists(), "codex binary must not be executed in dry_run");
    assert!(
        !tmp.path().join(".codex-worker").exists(),
        "dry_run must not create run artifacts"
    );
}

#[test]
fn spawn_failure_finalizes_task_and_writes_summary() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let repo = tmp.path().join("repo");
    fs::create_dir_all(&repo).expect("repo dir should be created");
    let task_file = tmp.path().join("task.md");
    fs::write(
        &task_file,
        format!("# [ ] Demo\nid: demo\ncwd: {}\n\nImplement\n", repo.display()),
    )
    .expect("task file should be written");

    let missing_codex = tmp.path().join("missing-codex-bin");
    let mut config = worker_config(&task_file, &missing_codex);
    config.codex_bin = missing_codex.display().to_string();

    let mut worker = CodexWorker::new(config);
    let exit = worker.run_next().expect("spawn failure should be fail-closed");
    assert_eq!(exit, 1);

    let content = read_task_content(&worker, &task_file);
    assert!(content.contains("## [!] Demo"));
    assert!(content.contains("last_result: process_spawn_failed"));

    let logs_root = tmp.path().join(".codex-worker");
    let summary_path = find_named_file(&logs_root, "summary.json").expect("summary should exist");
    let summary: Value = serde_json::from_str(
        &fs::read_to_string(summary_path).expect("summary should be readable"),
    )
    .expect("summary should be valid json");
    assert_eq!(
        summary.get("failure_reason").and_then(Value::as_str),
        Some("process_spawn_failed")
    );
}

#[test]
fn subagent_sessions_are_imported_from_codex_home() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let repo = tmp.path().join("repo");
    fs::create_dir_all(&repo).expect("repo dir should be created");
    let task_file = tmp.path().join("task.md");
    fs::write(
        &task_file,
        format!("# [ ] Demo\nid: demo\ncwd: {}\n\nImplement\n", repo.display()),
    )
    .expect("task file should be written");

    let codex_home = tmp.path().join("codex-home");
    let session_dir = codex_home.join("sessions").join("2026").join("03").join("24");
    fs::create_dir_all(&session_dir).expect("session dir should be created");
    fs::write(
        session_dir.join("sub-1-session.jsonl"),
        concat!(
            "{\"timestamp\":\"2026-03-24T09:41:00Z\",\"type\":\"session_meta\",\"payload\":{\"id\":\"sub-1\",\"cwd\":\"/repo\",\"agent_nickname\":\"Lovelace\",\"agent_role\":\"reviewer\",\"source\":{\"subagent\":{\"thread_spawn\":{\"parent_thread_id\":\"root-1\"}}}}}\n",
            "{\"timestamp\":\"2026-03-24T09:41:01Z\",\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"assistant\",\"content\":[{\"text\":\"checking\"}]}}\n",
            "{bad json\n"
        ),
    )
    .expect("session file should be written");

    let fake = tmp.path().join("fake-codex");
    write_fake_codex(
        &fake,
        r#"printf '%s\n' '{"type":"thread.started","thread_id":"root-1"}'
printf '%s\n' '{"type":"turn.started"}'
printf '%s\n' '{"type":"item.completed","item":{"type":"collab_tool_call","id":"ct-1","tool":"spawn_agent","status":"completed","sender_thread_id":"root-1","receiver_thread_ids":["sub-1"],"prompt":"go","agents_states":{"sub-1":{"status":"ok","message":"done"}}}}'
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"done"}}'
printf '%s\n' '{"type":"turn.completed"}'"#,
    );

    let mut config = worker_config(&task_file, &fake);
    config.codex_home = Some(codex_home);
    let mut worker = CodexWorker::new(config);
    let exit = worker.run_next().expect("run should finish");
    assert_eq!(exit, 0);

    let logs_root = tmp.path().join(".codex-worker");
    let events = fs::read_to_string(
        find_named_file(&logs_root, "events.jsonl").expect("events.jsonl should exist"),
    )
    .expect("events should be readable");
    assert!(events.contains("\"event_type\":\"agent.session\""));
    assert!(events.contains("\"event_type\":\"agent.message\""));
    assert!(events.contains("\"actor_type\":\"subagent\""));
    assert!(events.contains("\"thread_id\":\"sub-1\""));

    let imported = fs::read_to_string(
        find_path_ending_with(&logs_root, Path::new("subagents/sub-1.jsonl"))
            .expect("imported subagent log should exist"),
    )
    .expect("imported subagent log should be readable");
    assert!(imported.contains("\"type\":\"session_meta\""));
    assert!(imported.contains("\"type\":\"response_item\""));

    let raw_unparsed = fs::read_to_string(
        find_path_ending_with(&logs_root, Path::new("raw_unparsed/subagents.jsonl"))
            .expect("raw_unparsed/subagents.jsonl should exist"),
    )
    .expect("raw_unparsed/subagents should be readable");
    assert!(raw_unparsed.contains("\"event_type\":\"raw.unparsed\""));
    assert!(raw_unparsed.contains("\"thread_id\":\"sub-1\""));

    let problem_examples = fs::read_to_string(
        find_path_ending_with(&logs_root, Path::new("problem_examples/subagents.jsonl"))
            .expect("problem_examples/subagents.jsonl should exist"),
    )
    .expect("problem_examples/subagents should be readable");
    assert!(problem_examples.contains("\"event_type\":\"raw.unparsed\""));
}
