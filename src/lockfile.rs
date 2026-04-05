use std::ffi::CStr;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::util::{ensure_parent, utc_now_iso};

#[derive(Debug, Error)]
pub enum LockfileError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("lock is not held")]
    LockNotHeld,
}

pub type LockfileResult<T> = Result<T, LockfileError>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockPayload {
    pub pid: u32,
    pub hostname: String,
    pub worker_id: String,
    pub started_at: String,
    pub task_file: String,
    pub last_heartbeat_at: String,
}

#[derive(Debug)]
pub struct TaskFileLock {
    task_file: PathBuf,
    path: PathBuf,
    handle: Option<File>,
}

impl TaskFileLock {
    pub fn new(task_file: &Path) -> Self {
        let path = task_file
            .parent()
            .map(|parent| {
                parent.join(format!(
                    ".{}.lock",
                    task_file
                        .file_name()
                        .and_then(|v| v.to_str())
                        .unwrap_or("task")
                ))
            })
            .unwrap_or_else(|| PathBuf::from(format!(".{}.lock", task_file.display())));

        Self {
            task_file: task_file.to_path_buf(),
            path,
            handle: None,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn acquire(&mut self) -> LockfileResult<()> {
        ensure_parent(&self.path)?;
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&self.path)?;
        flock_exclusive(file.as_raw_fd())?;
        self.handle = Some(file);
        Ok(())
    }

    pub fn write_payload(&mut self, worker_id: &str) -> LockfileResult<LockPayload> {
        let Some(handle) = self.handle.as_mut() else {
            return Err(LockfileError::LockNotHeld);
        };

        let now = utc_now_iso();
        let payload = LockPayload {
            pid: std::process::id(),
            hostname: hostname(),
            worker_id: worker_id.to_string(),
            started_at: now.clone(),
            task_file: self
                .task_file
                .canonicalize()
                .unwrap_or_else(|_| self.task_file.clone())
                .display()
                .to_string(),
            last_heartbeat_at: now,
        };

        let json = serde_json::to_string(&payload).map_err(|err| {
            LockfileError::Io(std::io::Error::new(std::io::ErrorKind::InvalidData, err))
        })?;

        handle.seek(SeekFrom::Start(0))?;
        handle.set_len(0)?;
        handle.write_all(json.as_bytes())?;
        handle.flush()?;
        handle.sync_all()?;

        Ok(payload)
    }

    pub fn heartbeat(&mut self, worker_id: &str) -> LockfileResult<LockPayload> {
        self.write_payload(worker_id)
    }

    pub fn read_payload(&self) -> LockfileResult<Option<serde_json::Value>> {
        read_payload(&self.path)
    }

    pub fn release(&mut self) -> LockfileResult<()> {
        if let Some(mut file) = self.handle.take() {
            file.flush()?;
            file.sync_all()?;
            flock_unlock(file.as_raw_fd())?;
        }
        Ok(())
    }
}

impl Drop for TaskFileLock {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

pub fn read_payload(path: &Path) -> LockfileResult<Option<serde_json::Value>> {
    if !path.exists() {
        return Ok(None);
    }

    let mut text = String::new();
    File::open(path)?.read_to_string(&mut text)?;
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }

    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return Ok(None);
    };

    if value.is_object() {
        Ok(Some(value))
    } else {
        Ok(None)
    }
}

fn hostname() -> String {
    #[cfg(unix)]
    {
        let mut buf = [0u8; 256];
        let rc = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
        if rc == 0 {
            if let Some(nul) = buf.iter().position(|byte| *byte == 0) {
                if nul > 0 {
                    if let Ok(name) = CStr::from_bytes_with_nul(&buf[..=nul]) {
                        let text = name.to_string_lossy().trim().to_string();
                        if !text.is_empty() {
                            return text;
                        }
                    } else if let Ok(text) = std::str::from_utf8(&buf[..nul]) {
                        let cleaned = text.trim().to_string();
                        if !cleaned.is_empty() {
                            return cleaned;
                        }
                    }
                }
            } else if let Ok(text) = std::str::from_utf8(&buf) {
                let cleaned = text.trim().to_string();
                if !cleaned.is_empty() {
                    return cleaned;
                }
            }
        }
    }

    std::env::var("HOSTNAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            fs::read_to_string("/etc/hostname")
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
        .unwrap_or_else(|| "unknown".to_string())
}

#[cfg(unix)]
use std::os::fd::AsRawFd;

#[cfg(unix)]
fn flock_exclusive(fd: i32) -> LockfileResult<()> {
    let rc = unsafe { libc::flock(fd, libc::LOCK_EX) };
    if rc == 0 {
        Ok(())
    } else {
        Err(LockfileError::Io(std::io::Error::last_os_error()))
    }
}

#[cfg(unix)]
fn flock_unlock(fd: i32) -> LockfileResult<()> {
    let rc = unsafe { libc::flock(fd, libc::LOCK_UN) };
    if rc == 0 {
        Ok(())
    } else {
        Err(LockfileError::Io(std::io::Error::last_os_error()))
    }
}

#[cfg(not(unix))]
fn flock_exclusive(_fd: i32) -> LockfileResult<()> {
    Ok(())
}

#[cfg(not(unix))]
fn flock_unlock(_fd: i32) -> LockfileResult<()> {
    Ok(())
}
