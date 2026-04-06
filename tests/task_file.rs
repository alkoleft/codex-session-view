use codex_worker_rs::task_file::{
    archive_snapshot_if_completed, claim_next_task, finalize_task, load_snapshot,
    recover_stale_tasks, refresh_heartbeat, write_snapshot_atomic, TaskFileError,
};

const TASK_TEXT: &str = "# Tasks\n\n# [ ] First task\nBody line\n\n## nested heading is allowed\n\n# [>] Running task\nid: task-2\ncwd: /tmp/work\nlast_heartbeat_at: 2000-01-01T00:00:00Z\nlease_pid: 999999\n\nBody\n";

#[test]
fn load_snapshot_parses_heading_delimited_tasks() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let path = tmp.path().join("task.md");
    std::fs::write(&path, TASK_TEXT).expect("task file should be written");

    let snapshot = load_snapshot(&path).expect("snapshot should load");
    assert_eq!(snapshot.tasks.len(), 2);
    assert!(snapshot.tasks[0].body.contains("nested heading is allowed"));
    assert_eq!(
        snapshot.tasks[0].metadata.get("id").map(String::as_str),
        Some("first-task")
    );
}

#[test]
fn duplicate_id_fails() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let path = tmp.path().join("task.md");
    std::fs::write(
        &path,
        "# [ ] One\nid: same\ncwd: /tmp\n\nX\n\n# [ ] Two\nid: same\ncwd: /tmp\n\nY\n",
    )
    .expect("task file should be written");

    let err = load_snapshot(&path).expect_err("duplicate ids should fail");
    assert!(matches!(err, TaskFileError::DuplicateTaskIds(_)));
}

#[test]
fn free_form_body_without_metadata_is_allowed() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let path = tmp.path().join("task.md");
    std::fs::write(
        &path,
        "# [ ] Свободная задача\nСделай что-нибудь полезное\nПотом проверь результат\n",
    )
    .expect("task file should be written");

    let snapshot = load_snapshot(&path).expect("snapshot should load");
    assert_eq!(
        snapshot.tasks[0].metadata.get("id").map(String::as_str),
        Some("task")
    );
    assert!(snapshot.tasks[0]
        .body
        .contains("Сделай что-нибудь полезное"));
}

#[test]
fn generated_id_avoids_collision_with_explicit_id() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let path = tmp.path().join("task.md");
    std::fs::write(
        &path,
        "# [ ] Явная\nid: task\n\nOne\n\n# [ ] Свободная задача\nTwo\n",
    )
    .expect("task file should be written");

    let snapshot = load_snapshot(&path).expect("snapshot should load");
    let ids: Vec<&str> = snapshot
        .tasks
        .iter()
        .map(|task| task.metadata.get("id").map(String::as_str).unwrap_or(""))
        .collect();
    assert_eq!(ids, vec!["task", "task-1"]);
}

#[test]
fn prompt_metadata_is_parsed_as_regular_task_metadata() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let path = tmp.path().join("task.md");
    std::fs::write(&path, "# [ ] Task\nprompt: Answer in Russian\n\nBody\n")
        .expect("task file should be written");

    let snapshot = load_snapshot(&path).expect("snapshot should load");
    assert_eq!(
        snapshot.tasks[0].metadata.get("prompt").map(String::as_str),
        Some("Answer in Russian")
    );
    assert_eq!(snapshot.tasks[0].body, "Body");
}

#[test]
fn multiple_non_ascii_free_form_tasks_get_unique_ids() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let path = tmp.path().join("task.md");
    std::fs::write(
        &path,
        "# [ ] Первая задача\nOne\n\n# [ ] Вторая задача\nTwo\n\n# [ ] Третья задача\nThree\n",
    )
    .expect("task file should be written");

    let snapshot = load_snapshot(&path).expect("snapshot should load");
    let ids: Vec<&str> = snapshot
        .tasks
        .iter()
        .map(|task| task.metadata.get("id").map(String::as_str).unwrap_or(""))
        .collect();
    assert_eq!(ids, vec!["task", "task-1", "task-2"]);
}

#[test]
fn recover_stale_running_marks_failed() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let path = tmp.path().join("task.md");
    std::fs::write(&path, TASK_TEXT).expect("task file should be written");

    let snapshot = load_snapshot(&path).expect("snapshot should load");
    let (content, recovered) =
        recover_stale_tasks(&snapshot, 1.0).expect("stale recovery should work");
    assert_eq!(recovered, vec!["task-2"]);
    assert!(content.contains("worker_interrupted"));
}

#[cfg(unix)]
#[test]
fn recover_stale_running_skips_live_lease() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let path = tmp.path().join("task.md");
    let pid = std::process::id();
    std::fs::write(
        &path,
        format!(
            "# [>] Running\nid: task-live\nlast_heartbeat_at: 2999-01-01T00:00:00Z\nlease_pid: {pid}\n\nBody\n"
        ),
    )
    .expect("task file should be written");

    let snapshot = load_snapshot(&path).expect("snapshot should load");
    let (content, recovered) = recover_stale_tasks(&snapshot, 3600.0).expect("recovery should run");
    assert!(recovered.is_empty());
    assert_eq!(content, snapshot.content);
}

#[test]
fn recover_stale_running_marks_failed_when_process_missing() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let path = tmp.path().join("task.md");
    std::fs::write(
        &path,
        "# [>] Running\nid: task-dead\nlast_heartbeat_at: 2999-01-01T00:00:00Z\nlease_pid: -1\n\nBody\n",
    )
    .expect("task file should be written");

    let snapshot = load_snapshot(&path).expect("snapshot should load");
    let (content, recovered) = recover_stale_tasks(&snapshot, 3600.0).expect("recovery should run");
    assert_eq!(recovered, vec!["task-dead"]);
    assert!(content.contains("last_result: worker_interrupted"));
    assert!(content.contains("last_error: stale_running_recovered"));
}

#[test]
fn claim_next_task_accepts_running_status() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let path = tmp.path().join("task.md");
    std::fs::write(&path, "# [>] Running\nid: running-task\n\nResume work\n")
        .expect("task file should be written");

    let snapshot = load_snapshot(&path).expect("snapshot should load");
    let claimed = claim_next_task(&snapshot, "run-1", "worker-1").expect("claim should run");
    assert!(claimed.is_some());

    let (content, task) = claimed.expect("claim should return task");
    assert_eq!(
        task.metadata.get("id").map(String::as_str),
        Some("running-task")
    );
    assert!(content.contains("last_run_id: run-1"));
    assert!(content.contains("lease_owner: worker-1"));
}

#[test]
fn archive_snapshot_moves_file_when_all_tasks_completed() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let path = tmp.path().join("task.md");
    std::fs::write(&path, "# [x] Done\nid: done\n\nFinished\n")
        .expect("task file should be written");

    let content = std::fs::read_to_string(&path).expect("task file should be readable");
    let archived = archive_snapshot_if_completed(&path, &content).expect("archive should run");
    let archived_path = archived.expect("archive target should exist");

    assert_eq!(archived_path, tmp.path().join("archive").join("task.md"));
    assert!(!path.exists());
    assert!(archived_path.exists());
}

#[test]
fn archive_snapshot_uses_incremented_name_when_target_exists() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let path = tmp.path().join("task.md");
    let archive_dir = tmp.path().join("archive");
    std::fs::create_dir_all(&archive_dir).expect("archive dir should be created");
    std::fs::write(archive_dir.join("task.md"), "occupied").expect("archive file should exist");
    std::fs::write(&path, "# [x] Done\nid: done\n\nFinished\n")
        .expect("task file should be written");

    let content = std::fs::read_to_string(&path).expect("task file should be readable");
    let archived = archive_snapshot_if_completed(&path, &content).expect("archive should run");
    let archived_path = archived.expect("archive target should exist");

    assert_eq!(archived_path, tmp.path().join("archive").join("task-1.md"));
    assert!(archived_path.exists());
    assert_eq!(
        std::fs::read_to_string(tmp.path().join("archive").join("task.md"))
            .expect("existing archive file should stay"),
        "occupied"
    );
}

#[test]
fn archive_snapshot_skips_incomplete_tasks() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let path = tmp.path().join("task.md");
    std::fs::write(
        &path,
        "# [x] Done\nid: done\n\nFinished\n\n# [ ] Pending\nid: pending\n\nTodo\n",
    )
    .expect("task file should be written");

    let content = std::fs::read_to_string(&path).expect("task file should be readable");
    let archived = archive_snapshot_if_completed(&path, &content).expect("archive should run");

    assert!(archived.is_none());
    assert!(path.exists());
}

#[test]
fn write_snapshot_atomic_creates_parent_and_replaces_content() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let path = tmp.path().join("nested").join("task.md");

    write_snapshot_atomic(&path, "first").expect("first write should succeed");
    assert_eq!(
        std::fs::read_to_string(&path).expect("task file should be readable"),
        "first"
    );

    write_snapshot_atomic(&path, "second").expect("second write should succeed");
    assert_eq!(
        std::fs::read_to_string(&path).expect("task file should be readable"),
        "second"
    );
}

#[test]
fn serialize_preserves_explicit_metadata_order() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let path = tmp.path().join("task.md");
    std::fs::write(
        &path,
        "# [ ] Task\nzeta: 1\nalpha: 2\nprompt: Keep order\n\nBody\n",
    )
    .expect("task file should be written");

    let snapshot = load_snapshot(&path).expect("snapshot should load");
    let (content, _task) = claim_next_task(&snapshot, "run-1", "worker-1")
        .expect("claim should run")
        .unwrap();

    let zeta = content.find("zeta: 1").expect("zeta metadata should exist");
    let alpha = content
        .find("alpha: 2")
        .expect("alpha metadata should exist");
    let prompt = content
        .find("prompt: Keep order")
        .expect("prompt metadata should exist");
    assert!(zeta < alpha);
    assert!(alpha < prompt);
}

#[test]
fn finalize_rejects_ownership_change() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let path = tmp.path().join("task.md");
    std::fs::write(
        &path,
        "# [>] Running\nid: running-task\nlast_run_id: run-1\nlease_owner: worker-1\nlease_pid: 1\n\nBody\n",
    )
    .expect("task file should be written");

    let snapshot = load_snapshot(&path).expect("snapshot should load");
    let task = snapshot.tasks[0].clone();
    let claimed = codex_worker_rs::models::ClaimedTask {
        task,
        run_id: "run-1".to_string(),
        worker_id: "worker-2".to_string(),
        worker_pid: 2,
        snapshot,
    };

    let err =
        finalize_task(&claimed.snapshot, &claimed, "x", "success", "").expect_err("must fail");
    assert!(matches!(err, TaskFileError::OwnershipChanged("finalize")));
}

#[test]
fn finalize_rejects_run_id_change() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let path = tmp.path().join("task.md");
    std::fs::write(
        &path,
        "# [>] Running\nid: running-task\nlast_run_id: run-1\nlease_owner: worker-1\nlease_pid: 1\n\nBody\n",
    )
    .expect("task file should be written");

    let snapshot = load_snapshot(&path).expect("snapshot should load");
    let task = snapshot.tasks[0].clone();
    let claimed = codex_worker_rs::models::ClaimedTask {
        task,
        run_id: "run-2".to_string(),
        worker_id: "worker-1".to_string(),
        worker_pid: 1,
        snapshot,
    };

    let err =
        finalize_task(&claimed.snapshot, &claimed, "x", "success", "").expect_err("must fail");
    assert!(matches!(err, TaskFileError::OwnershipChanged("finalize")));
}

#[test]
fn heartbeat_rejects_ownership_change() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let path = tmp.path().join("task.md");
    std::fs::write(
        &path,
        "# [>] Running\nid: running-task\nlast_run_id: run-1\nlease_owner: worker-1\nlease_pid: 1\n\nBody\n",
    )
    .expect("task file should be written");

    let snapshot = load_snapshot(&path).expect("snapshot should load");
    let err = refresh_heartbeat(&snapshot, "running-task", "worker-2").expect_err("must fail");
    assert!(matches!(err, TaskFileError::OwnershipChanged("heartbeat")));
}
