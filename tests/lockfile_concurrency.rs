use std::time::{Duration, Instant};

use codex_worker_rs::lockfile::TaskFileLock;
use codex_worker_rs::models::ClaimedTask;
use codex_worker_rs::task_file::{
    finalize_task, load_snapshot, refresh_heartbeat, write_snapshot_atomic,
};

#[test]
fn write_read_and_heartbeat_payload() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let task_file = tmp.path().join("task.md");
    std::fs::write(&task_file, "# [ ] Task\n").expect("task file should be written");

    let mut lock = TaskFileLock::new(&task_file);
    lock.acquire().expect("lock should be acquired");

    let payload = lock
        .write_payload("worker-1")
        .expect("payload should be written");
    assert_eq!(payload.worker_id, "worker-1");
    assert_eq!(lock.path(), &tmp.path().join(".task.md.lock"));

    let stored = lock
        .read_payload()
        .expect("payload should be readable")
        .expect("payload should exist");
    assert_eq!(stored["worker_id"], "worker-1");

    let heartbeat = lock.heartbeat("worker-2").expect("heartbeat should update");
    assert_eq!(heartbeat.worker_id, "worker-2");
    let stored_after = lock
        .read_payload()
        .expect("payload should be readable")
        .expect("payload should exist");
    assert_eq!(stored_after["worker_id"], "worker-2");

    lock.release().expect("lock should release");
}

#[cfg(unix)]
#[test]
fn lock_is_process_exclusive() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let task_file = tmp.path().join("task.md");
    std::fs::write(&task_file, "# [ ] Task\n").expect("task file should be written");

    let marker = tmp.path().join("elapsed_ms.txt");

    let mut first = TaskFileLock::new(&task_file);
    first.acquire().expect("first lock should be acquired");

    let pid = unsafe { libc::fork() };
    assert!(pid >= 0, "fork must succeed");

    if pid == 0 {
        let mut second = TaskFileLock::new(&task_file);
        let started = Instant::now();
        let acquired = second.acquire();
        let elapsed_ms = started.elapsed().as_millis();

        if acquired.is_ok() {
            let _ = second.release();
        }

        let _ = std::fs::write(&marker, elapsed_ms.to_string());
        unsafe { libc::_exit(0) };
    }

    std::thread::sleep(Duration::from_millis(350));
    first.release().expect("first lock should release");

    let mut status: libc::c_int = 0;
    let waited = unsafe { libc::waitpid(pid, &mut status as *mut libc::c_int, 0) };
    assert_eq!(waited, pid, "must wait for child process");
    assert!(libc::WIFEXITED(status), "child should exit cleanly");

    let elapsed_ms: u128 = std::fs::read_to_string(&marker)
        .expect("elapsed marker should exist")
        .trim()
        .parse()
        .expect("elapsed marker should be numeric");
    assert!(
        elapsed_ms >= 250,
        "acquire should block while lock is held, got {elapsed_ms}ms"
    );
}

#[cfg(unix)]
#[test]
fn finalize_under_lock_is_atomic_for_waiting_process() {
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let task_file = tmp.path().join("task.md");
    std::fs::write(
        &task_file,
        "# [>] Running\nid: task-1\nlast_run_id: run-1\nlease_owner: worker-1\nlease_pid: 123\n\nBody\n",
    )
    .expect("task file should be written");

    let marker = tmp.path().join("contention_result.txt");
    let mut first = TaskFileLock::new(&task_file);
    first.acquire().expect("first lock should be acquired");

    let pid = unsafe { libc::fork() };
    assert!(pid >= 0, "fork must succeed");

    if pid == 0 {
        let mut second = TaskFileLock::new(&task_file);
        let started = Instant::now();
        second.acquire().expect("second lock should be acquired");
        let elapsed_ms = started.elapsed().as_millis();

        let snapshot = load_snapshot(&task_file).expect("snapshot should load");
        let heartbeat_err = refresh_heartbeat(&snapshot, "task-1", "worker-2")
            .expect_err("heartbeat with wrong owner must fail");
        let status = match heartbeat_err {
            codex_worker_rs::task_file::TaskFileError::OwnershipChanged("heartbeat") => "ok",
            _ => "bad",
        };
        let _ = second.release();
        let _ = std::fs::write(&marker, format!("{elapsed_ms}:{status}"));
        unsafe { libc::_exit(0) };
    }

    std::thread::sleep(Duration::from_millis(350));
    let snapshot = load_snapshot(&task_file).expect("snapshot should load");
    let claimed = ClaimedTask {
        task: snapshot.tasks[0].clone(),
        run_id: "run-1".to_string(),
        worker_id: "worker-1".to_string(),
        worker_pid: 123,
        snapshot: snapshot.clone(),
    };
    let finalized =
        finalize_task(&snapshot, &claimed, "x", "success", "").expect("finalize should work");
    write_snapshot_atomic(&task_file, &finalized).expect("atomic write should work");
    first.release().expect("first lock should release");

    let mut status: libc::c_int = 0;
    let waited = unsafe { libc::waitpid(pid, &mut status as *mut libc::c_int, 0) };
    assert_eq!(waited, pid, "must wait for child process");
    assert!(libc::WIFEXITED(status), "child should exit cleanly");

    let marker_text = std::fs::read_to_string(&marker).expect("marker should exist");
    let mut parts = marker_text.trim().split(':');
    let elapsed_ms: u128 = parts
        .next()
        .expect("elapsed part should exist")
        .parse()
        .expect("elapsed must be numeric");
    let ownership_status = parts.next().expect("status part should exist");
    assert!(
        elapsed_ms >= 250,
        "acquire should block while lock is held, got {elapsed_ms}ms"
    );
    assert_eq!(ownership_status, "ok");

    let final_content = std::fs::read_to_string(&task_file).expect("task file should be readable");
    assert!(final_content.contains("## [x] Running"));
    assert!(final_content.contains("last_result: success"));
}
