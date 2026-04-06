use std::path::Path;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{Duration, Instant};

use codex_worker_rs::lockfile::TaskFileLock;
use codex_worker_rs::models::ClaimedTask;
use codex_worker_rs::task_file::{
    apply_task_update, claim_next_task, finalize_task, load_snapshot, recover_stale_tasks,
    refresh_heartbeat, write_snapshot_atomic,
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
fn fork_test_guard() -> MutexGuard<'static, ()> {
    static FORK_TEST_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
    FORK_TEST_MUTEX
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("fork test mutex must lock")
}

#[cfg(unix)]
#[test]
fn lock_is_process_exclusive() {
    let _guard = fork_test_guard();
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
    let _guard = fork_test_guard();
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

#[cfg(unix)]
#[derive(Debug, Default)]
struct ProtocolOutcome {
    claimed: bool,
    finalized: bool,
    run_id: String,
    worker_id: String,
}

#[cfg(unix)]
fn run_claim_finalize_protocol(
    task_file: &Path,
    run_id: &str,
    worker_id: &str,
) -> Result<ProtocolOutcome, String> {
    let mut lock = TaskFileLock::new(task_file);
    lock.acquire().map_err(|err| err.to_string())?;
    let snapshot = load_snapshot(task_file).map_err(|err| err.to_string())?;
    let claimed = claim_next_task(&snapshot, run_id, worker_id).map_err(|err| err.to_string())?;

    let Some((claimed_content, task)) = claimed else {
        lock.release().map_err(|err| err.to_string())?;
        return Ok(ProtocolOutcome::default());
    };

    write_snapshot_atomic(task_file, &claimed_content).map_err(|err| err.to_string())?;
    let finalize_snapshot = load_snapshot(task_file).map_err(|err| err.to_string())?;
    let claimed_task = ClaimedTask {
        task,
        run_id: run_id.to_string(),
        worker_id: worker_id.to_string(),
        worker_pid: std::process::id(),
        snapshot,
    };
    let finalized = finalize_task(&finalize_snapshot, &claimed_task, "x", "success", "")
        .map_err(|err| err.to_string())?;
    write_snapshot_atomic(task_file, &finalized).map_err(|err| err.to_string())?;
    lock.release().map_err(|err| err.to_string())?;

    Ok(ProtocolOutcome {
        claimed: true,
        finalized: true,
        run_id: run_id.to_string(),
        worker_id: worker_id.to_string(),
    })
}

#[cfg(unix)]
fn write_marker(path: &Path, outcome: &ProtocolOutcome, error: Option<&str>) {
    let error_text = error.unwrap_or("");
    let payload = format!(
        "claimed={}\nfinalized={}\nrun_id={}\nworker_id={}\nerror={}\n",
        outcome.claimed as u8,
        outcome.finalized as u8,
        outcome.run_id,
        outcome.worker_id,
        error_text
    );
    std::fs::write(path, payload).expect("marker should be written");
}

#[cfg(unix)]
fn parse_marker(path: &Path) -> ProtocolOutcome {
    let text = std::fs::read_to_string(path).expect("marker should be readable");
    let mut claimed = false;
    let mut finalized = false;
    let mut run_id = String::new();
    let mut worker_id = String::new();
    let mut error = String::new();
    for line in text.lines() {
        if let Some((key, value)) = line.split_once('=') {
            match key {
                "claimed" => claimed = value == "1",
                "finalized" => finalized = value == "1",
                "run_id" => run_id = value.to_string(),
                "worker_id" => worker_id = value.to_string(),
                "error" => error = value.to_string(),
                _ => {}
            }
        }
    }
    assert!(error.is_empty(), "child protocol failed: {error}");
    ProtocolOutcome {
        claimed,
        finalized,
        run_id,
        worker_id,
    }
}

#[cfg(unix)]
fn make_pipe() -> [libc::c_int; 2] {
    let mut fds = [0; 2];
    let rc = unsafe { libc::pipe(fds.as_mut_ptr()) };
    assert_eq!(rc, 0, "pipe must be created");
    fds
}

#[cfg(unix)]
fn read_start_signal(fd: libc::c_int) {
    let mut byte = [0u8; 1];
    loop {
        let rc = unsafe { libc::read(fd, byte.as_mut_ptr().cast(), 1) };
        if rc == 1 {
            return;
        }
        if rc == -1 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            panic!("read from pipe failed: {err}");
        }
        panic!("unexpected pipe EOF");
    }
}

#[cfg(unix)]
fn write_start_signal(fd: libc::c_int) {
    let byte = [1u8; 1];
    loop {
        let rc = unsafe { libc::write(fd, byte.as_ptr().cast(), 1) };
        if rc == 1 {
            return;
        }
        if rc == -1 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            panic!("write to pipe failed: {err}");
        }
    }
}

#[cfg(unix)]
fn close_fd(fd: libc::c_int) {
    let rc = unsafe { libc::close(fd) };
    assert_eq!(rc, 0, "fd should close");
}

#[cfg(unix)]
#[test]
fn concurrent_claim_finalize_two_processes() {
    let _guard = fork_test_guard();
    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let task_file = tmp.path().join("task.md");
    std::fs::write(&task_file, "# [ ] Only task\nid: task-1\n\nBody\n")
        .expect("task file should be written");
    let child_marker = tmp.path().join("child_protocol.txt");

    let start_pipe = make_pipe();
    let pid = unsafe { libc::fork() };
    assert!(pid >= 0, "fork must succeed");

    if pid == 0 {
        close_fd(start_pipe[1]);
        read_start_signal(start_pipe[0]);
        close_fd(start_pipe[0]);

        match run_claim_finalize_protocol(&task_file, "run-child", "worker-child") {
            Ok(outcome) => write_marker(&child_marker, &outcome, None),
            Err(err) => write_marker(&child_marker, &ProtocolOutcome::default(), Some(&err)),
        }
        unsafe { libc::_exit(0) };
    }

    close_fd(start_pipe[0]);
    write_start_signal(start_pipe[1]);
    close_fd(start_pipe[1]);

    let parent_outcome = run_claim_finalize_protocol(&task_file, "run-parent", "worker-parent")
        .expect("parent protocol should succeed");

    let mut status: libc::c_int = 0;
    let waited = unsafe { libc::waitpid(pid, &mut status as *mut libc::c_int, 0) };
    assert_eq!(waited, pid, "must wait for child process");
    assert!(libc::WIFEXITED(status), "child should exit cleanly");

    let child_outcome = parse_marker(&child_marker);
    let claimed_count = usize::from(parent_outcome.claimed) + usize::from(child_outcome.claimed);
    let finalized_count =
        usize::from(parent_outcome.finalized) + usize::from(child_outcome.finalized);
    assert_eq!(claimed_count, 1, "only one process may claim task");
    assert_eq!(finalized_count, 1, "only claimer may finalize task");

    let winner = if parent_outcome.claimed {
        parent_outcome
    } else {
        child_outcome
    };
    assert!(
        !winner.run_id.is_empty() && !winner.worker_id.is_empty(),
        "winner must keep claim identity"
    );

    let final_content = std::fs::read_to_string(&task_file).expect("task file should be readable");
    assert!(final_content.contains("## [x] Only task"));
    assert!(final_content.contains(&format!("last_run_id: {}", winner.run_id)));
    assert!(final_content.contains(&format!("lease_owner: {}", winner.worker_id)));
}

#[cfg(unix)]
#[test]
fn heartbeat_recovery_under_load() {
    let _guard = fork_test_guard();
    const ITERATIONS: usize = 140;

    let tmp = tempfile::tempdir().expect("tmpdir should be created");
    let task_file = tmp.path().join("task.md");
    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let owner_pid = std::process::id();
    std::fs::write(
        &task_file,
        format!(
            "# [>] Running\nid: task-1\nlast_run_id: run-live\nlease_owner: worker-live\nlease_pid: {owner_pid}\nlast_heartbeat_at: {now}\nlast_result: running\n\nBody\n"
        ),
    )
    .expect("task file should be written");
    let child_marker = tmp.path().join("recovery_stats.txt");

    let start_pipe = make_pipe();
    let pid = unsafe { libc::fork() };
    assert!(pid >= 0, "fork must succeed");

    if pid == 0 {
        close_fd(start_pipe[1]);
        read_start_signal(start_pipe[0]);
        close_fd(start_pipe[0]);

        let mut recovered_count = 0usize;
        let mut error = String::new();
        for _ in 0..ITERATIONS {
            let mut lock = TaskFileLock::new(&task_file);
            if let Err(err) = lock.acquire() {
                error = err.to_string();
                break;
            }

            let snapshot = match load_snapshot(&task_file) {
                Ok(snapshot) => snapshot,
                Err(err) => {
                    error = err.to_string();
                    let _ = lock.release();
                    break;
                }
            };
            let (updated, recovered) = match recover_stale_tasks(&snapshot, 2.0) {
                Ok(value) => value,
                Err(err) => {
                    error = err.to_string();
                    let _ = lock.release();
                    break;
                }
            };
            if !recovered.is_empty() {
                recovered_count += recovered.len();
                if let Err(err) = write_snapshot_atomic(&task_file, &updated) {
                    error = err.to_string();
                    let _ = lock.release();
                    break;
                }
            }
            if let Err(err) = lock.release() {
                error = err.to_string();
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }

        let payload = format!("recovered={recovered_count}\nerror={error}\n");
        std::fs::write(&child_marker, payload).expect("marker should be written");
        unsafe { libc::_exit(0) };
    }

    close_fd(start_pipe[0]);
    write_start_signal(start_pipe[1]);
    close_fd(start_pipe[1]);

    let mut heartbeat_errors = 0usize;
    for _ in 0..ITERATIONS {
        let mut lock = TaskFileLock::new(&task_file);
        lock.acquire().expect("lock should be acquired");
        let snapshot = load_snapshot(&task_file).expect("snapshot should load");
        let refreshed = refresh_heartbeat(&snapshot, "task-1", "worker-live");
        match refreshed {
            Ok(updated) => {
                write_snapshot_atomic(&task_file, &updated).expect("heartbeat write must succeed");
            }
            Err(codex_worker_rs::task_file::TaskFileError::OwnershipChanged("heartbeat")) => {
                heartbeat_errors += 1;
            }
            Err(other) => panic!("heartbeat failed unexpectedly: {other}"),
        }
        lock.release().expect("lock should release");
        std::thread::sleep(Duration::from_millis(1));
    }

    let mut status: libc::c_int = 0;
    let waited = unsafe { libc::waitpid(pid, &mut status as *mut libc::c_int, 0) };
    assert_eq!(waited, pid, "must wait for child process");
    assert!(libc::WIFEXITED(status), "child should exit cleanly");

    let marker_text = std::fs::read_to_string(&child_marker).expect("marker should exist");
    let mut recovered_count = 0usize;
    let mut child_error = String::new();
    for line in marker_text.lines() {
        if let Some((key, value)) = line.split_once('=') {
            match key {
                "recovered" => {
                    recovered_count = value.parse().expect("recovered must be numeric");
                }
                "error" => {
                    child_error = value.to_string();
                }
                _ => {}
            }
        }
    }

    assert!(
        child_error.is_empty(),
        "child recovery loop failed: {child_error}"
    );
    assert_eq!(recovered_count, 0, "running task must not be recovered");
    assert_eq!(
        heartbeat_errors, 0,
        "heartbeat should not lose ownership under lock"
    );

    let final_content = std::fs::read_to_string(&task_file).expect("task file should be readable");
    assert!(final_content.contains("## [>] Running"));
    assert!(final_content.contains("lease_owner: worker-live"));

    let stale_marker = tmp.path().join("forced_stale_marker.txt");
    let stale_pipe = make_pipe();
    let stale_pid = unsafe { libc::fork() };
    assert!(stale_pid >= 0, "fork must succeed");

    if stale_pid == 0 {
        close_fd(stale_pipe[1]);
        read_start_signal(stale_pipe[0]);
        close_fd(stale_pipe[0]);

        let result = (|| -> Result<(), String> {
            let mut lock = TaskFileLock::new(&task_file);
            lock.acquire().map_err(|err| err.to_string())?;
            let snapshot = load_snapshot(&task_file).map_err(|err| err.to_string())?;
            let stale_ts = "2000-01-01T00:00:00Z";
            let forced = apply_task_update(&snapshot, "task-1", |mut task| {
                task.metadata
                    .insert("last_heartbeat_at".to_string(), stale_ts.to_string());
                task.metadata
                    .insert("lease_pid".to_string(), "-1".to_string());
                Ok(task)
            })
            .map_err(|err| err.to_string())?;
            write_snapshot_atomic(&task_file, &forced).map_err(|err| err.to_string())?;
            lock.release().map_err(|err| err.to_string())?;
            Ok(())
        })();

        let error = result.err().unwrap_or_default();
        std::fs::write(&stale_marker, format!("error={error}\n"))
            .expect("marker should be written");
        unsafe { libc::_exit(0) };
    }

    close_fd(stale_pipe[0]);
    write_start_signal(stale_pipe[1]);
    close_fd(stale_pipe[1]);

    let mut stale_status: libc::c_int = 0;
    let stale_waited =
        unsafe { libc::waitpid(stale_pid, &mut stale_status as *mut libc::c_int, 0) };
    assert_eq!(stale_waited, stale_pid, "must wait for stale child");
    assert!(
        libc::WIFEXITED(stale_status),
        "stale child should exit cleanly"
    );
    let stale_text = std::fs::read_to_string(&stale_marker).expect("stale marker should exist");
    assert!(
        stale_text.trim() == "error=",
        "forced stale phase failed: {stale_text}"
    );

    let recover_marker = tmp.path().join("forced_recover_marker.txt");
    let recover_pipe = make_pipe();
    let recover_pid = unsafe { libc::fork() };
    assert!(recover_pid >= 0, "fork must succeed");

    if recover_pid == 0 {
        close_fd(recover_pipe[1]);
        read_start_signal(recover_pipe[0]);
        close_fd(recover_pipe[0]);

        let result = (|| -> Result<(usize, usize), String> {
            let mut lock1 = TaskFileLock::new(&task_file);
            lock1.acquire().map_err(|err| err.to_string())?;
            let snapshot1 = load_snapshot(&task_file).map_err(|err| err.to_string())?;
            let (updated1, recovered1) =
                recover_stale_tasks(&snapshot1, 0.1).map_err(|err| err.to_string())?;
            if !recovered1.is_empty() {
                write_snapshot_atomic(&task_file, &updated1).map_err(|err| err.to_string())?;
            }
            lock1.release().map_err(|err| err.to_string())?;

            let mut lock2 = TaskFileLock::new(&task_file);
            lock2.acquire().map_err(|err| err.to_string())?;
            let snapshot2 = load_snapshot(&task_file).map_err(|err| err.to_string())?;
            let (_updated2, recovered2) =
                recover_stale_tasks(&snapshot2, 0.1).map_err(|err| err.to_string())?;
            lock2.release().map_err(|err| err.to_string())?;

            Ok((recovered1.len(), recovered2.len()))
        })();

        let (recovered_first, recovered_second, error) = match result {
            Ok((a, b)) => (a, b, String::new()),
            Err(err) => (0, 0, err),
        };
        std::fs::write(
            &recover_marker,
            format!(
                "recovered_first={recovered_first}\nrecovered_second={recovered_second}\nerror={error}\n"
            ),
        )
        .expect("marker should be written");
        unsafe { libc::_exit(0) };
    }

    close_fd(recover_pipe[0]);
    write_start_signal(recover_pipe[1]);
    close_fd(recover_pipe[1]);

    let mut recover_status: libc::c_int = 0;
    let recover_waited =
        unsafe { libc::waitpid(recover_pid, &mut recover_status as *mut libc::c_int, 0) };
    assert_eq!(recover_waited, recover_pid, "must wait for recover child");
    assert!(
        libc::WIFEXITED(recover_status),
        "recover child should exit cleanly"
    );

    let recover_text =
        std::fs::read_to_string(&recover_marker).expect("recover marker should exist");
    let mut recovered_first = 0usize;
    let mut recovered_second = 0usize;
    let mut recover_error = String::new();
    for line in recover_text.lines() {
        if let Some((key, value)) = line.split_once('=') {
            match key {
                "recovered_first" => {
                    recovered_first = value.parse().expect("recovered_first must be numeric");
                }
                "recovered_second" => {
                    recovered_second = value.parse().expect("recovered_second must be numeric");
                }
                "error" => {
                    recover_error = value.to_string();
                }
                _ => {}
            }
        }
    }
    assert!(
        recover_error.is_empty(),
        "forced recovery phase failed: {recover_error}"
    );
    assert_eq!(
        recovered_first, 1,
        "stale recovery must trigger exactly once"
    );
    assert_eq!(
        recovered_second, 0,
        "second recovery pass must not recover already finalized stale task"
    );

    let recovered_content =
        std::fs::read_to_string(&task_file).expect("task file should be readable after recovery");
    assert!(recovered_content.contains("## [!] Running"));
    assert!(recovered_content.contains("last_result: worker_interrupted"));
    assert!(recovered_content.contains("last_error: stale_running_recovered"));
}
