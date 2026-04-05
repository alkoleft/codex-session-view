use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use chrono::{DateTime, Utc};
use regex::Regex;
use thiserror::Error;

use crate::models::{ClaimedTask, TaskBlock, TaskFileSnapshot, WORKER_OWNED_METADATA};
use crate::util::{process_exists, sha256_text, slugify, utc_now_iso};

#[derive(Debug, Error)]
pub enum TaskFileError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("invalid task format: {0}")]
    InvalidFormat(String),

    #[error("duplicate task ids: {0}")]
    DuplicateTaskIds(String),

    #[error("task not found: {0}")]
    TaskNotFound(String),

    #[error("claim ownership changed before {0}")]
    OwnershipChanged(&'static str),
}

pub type TaskFileResult<T> = Result<T, TaskFileError>;

fn task_heading_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"^(?P<level>#+) \[(?P<status>[ >x!])\] (?P<title>.+?)\s*$")
            .expect("task heading regex must compile")
    })
}

#[derive(Debug, Clone)]
struct RawTask {
    title: String,
    status: String,
    metadata: HashMap<String, String>,
    explicit_metadata_keys: HashSet<String>,
    explicit_metadata_order: Vec<String>,
    body: String,
    raw_text: String,
    start: usize,
    end: usize,
}

pub fn load_snapshot(path: &Path) -> TaskFileResult<TaskFileSnapshot> {
    let content = fs::read_to_string(path)?;
    let stat = fs::metadata(path)?;
    let tasks = parse_tasks(&content)?;

    let mut counts: HashMap<String, usize> = HashMap::new();
    for task in &tasks {
        let Some(id) = task.metadata.get("id") else {
            continue;
        };
        *counts.entry(id.clone()).or_insert(0) += 1;
    }

    let mut duplicates: Vec<String> = counts
        .into_iter()
        .filter_map(|(id, count)| (count > 1).then_some(id))
        .collect();
    duplicates.sort();

    if !duplicates.is_empty() {
        return Err(TaskFileError::DuplicateTaskIds(duplicates.join(", ")));
    }

    let modified = stat.modified()?;
    let mtime_ns = modified
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();

    Ok(TaskFileSnapshot {
        path: path.to_path_buf(),
        content: content.clone(),
        tasks,
        sha256: sha256_text(&content),
        mtime_ns,
    })
}

pub fn parse_tasks(content: &str) -> TaskFileResult<Vec<TaskBlock>> {
    let lines = split_lines_keepends(content);
    let mut offsets = Vec::with_capacity(lines.len());
    let mut cursor = 0usize;
    for line in &lines {
        offsets.push(cursor);
        cursor += line.len();
    }

    let mut headers: Vec<(usize, regex::Captures<'_>)> = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let without_newline = line.trim_end_matches('\n');
        if let Some(captures) = task_heading_pattern().captures(without_newline) {
            headers.push((index, captures));
        }
    }

    let mut raw_tasks: Vec<RawTask> = Vec::new();
    let mut used_ids: HashSet<String> = HashSet::new();

    for (position, (line_index, captures)) in headers.iter().enumerate() {
        let next_index = if position + 1 < headers.len() {
            headers[position + 1].0
        } else {
            lines.len()
        };
        let start = offsets[*line_index];
        let end = if next_index < lines.len() {
            offsets[next_index]
        } else {
            content.len()
        };

        let block_lines: Vec<&str> = lines[*line_index..next_index]
            .iter()
            .map(|line| line.trim_end_matches('\n'))
            .collect();
        let status = captures
            .name("status")
            .map(|v| v.as_str().to_string())
            .ok_or_else(|| {
                TaskFileError::InvalidFormat("missing status in task header".to_string())
            })?;
        let title = captures
            .name("title")
            .map(|v| v.as_str().trim().to_string())
            .ok_or_else(|| {
                TaskFileError::InvalidFormat("missing title in task header".to_string())
            })?;

        let mut metadata: HashMap<String, String> = HashMap::new();
        let mut explicit_metadata_keys: HashSet<String> = HashSet::new();
        let mut explicit_metadata_order: Vec<String> = Vec::new();
        let mut body_start = 1usize;
        while body_start < block_lines.len() {
            let line = block_lines[body_start];
            if line.trim().is_empty() {
                body_start += 1;
                break;
            }
            let Some((raw_key, raw_value)) = line.split_once(':') else {
                break;
            };

            let key = raw_key.trim();
            if key.is_empty() || key.chars().any(char::is_whitespace) {
                break;
            }

            metadata.insert(key.to_string(), raw_value.trim().to_string());
            if !explicit_metadata_keys.contains(key) {
                explicit_metadata_order.push(key.to_string());
            }
            explicit_metadata_keys.insert(key.to_string());
            body_start += 1;
        }

        let body = block_lines[body_start..].join("\n").trim_end().to_string();
        if let Some(explicit_id) = metadata.get("id") {
            used_ids.insert(explicit_id.clone());
        }

        raw_tasks.push(RawTask {
            title,
            status,
            metadata,
            explicit_metadata_keys,
            explicit_metadata_order,
            body,
            raw_text: content[start..end].to_string(),
            start,
            end,
        });
    }

    let mut tasks: Vec<TaskBlock> = Vec::new();
    let mut generated_ids: HashMap<String, usize> = HashMap::new();
    for raw in raw_tasks {
        let mut metadata = raw.metadata.clone();
        if !metadata.contains_key("id") {
            let base = slugify(&raw.title, 48);
            let mut candidate = base.clone();
            let mut next_suffix = generated_ids.get(&base).copied().unwrap_or(0);
            while candidate.is_empty() || used_ids.contains(&candidate) {
                next_suffix += 1;
                candidate = if base.is_empty() {
                    format!("task-{next_suffix}")
                } else {
                    format!("{base}-{next_suffix}")
                };
            }
            generated_ids.insert(base, next_suffix);
            used_ids.insert(candidate.clone());
            metadata.insert("id".to_string(), candidate);
        }

        let metadata = metadata
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        let explicit_metadata_keys = raw
            .explicit_metadata_keys
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();

        tasks.push(TaskBlock {
            title: raw.title,
            status: raw.status,
            metadata,
            explicit_metadata_keys,
            explicit_metadata_order: raw.explicit_metadata_order,
            body: raw.body,
            raw_text: raw.raw_text,
            start: raw.start,
            end: raw.end,
        });
    }

    Ok(tasks)
}

pub fn serialize_task(task: &TaskBlock) -> String {
    let heading = format!("## [{}] {}", task.status, task.title);
    let mut parts = vec![heading];

    let mut metadata_lines: Vec<String> = Vec::new();

    for key in &task.explicit_metadata_order {
        if task.explicit_metadata_keys.contains(key) {
            if let Some(value) = task.metadata.get(key) {
                metadata_lines.push(format!("{key}: {value}"));
            }
        }
    }
    for key in &task.explicit_metadata_keys {
        if task.explicit_metadata_order.iter().any(|item| item == key) {
            continue;
        }
        if let Some(value) = task.metadata.get(key) {
            metadata_lines.push(format!("{key}: {value}"));
        }
    }

    for key in ["id", "cwd"] {
        if let Some(value) = task.metadata.get(key) {
            if !task.explicit_metadata_keys.contains(key) {
                metadata_lines.push(format!("{key}: {value}"));
            }
        }
    }

    for key in WORKER_OWNED_METADATA {
        if let Some(value) = task.metadata.get(*key) {
            if !task.explicit_metadata_keys.contains(*key) {
                metadata_lines.push(format!("{key}: {value}"));
            }
        }
    }

    if !metadata_lines.is_empty() {
        parts.extend(metadata_lines);
        if !task.body.is_empty() {
            parts.push(String::new());
        }
    } else if !task.body.is_empty() {
        parts.push(String::new());
    }

    if !task.body.is_empty() {
        parts.push(task.body.trim_end().to_string());
    }

    format!("{}\n", parts.join("\n").trim_end())
}

pub fn write_snapshot_atomic(path: &Path, content: &str) -> TaskFileResult<()> {
    let parent = path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    fs::create_dir_all(&parent)?;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            TaskFileError::InvalidFormat(format!("invalid file name: {}", path.display()))
        })?;

    let mut tmp = tempfile::Builder::new()
        .prefix(&format!(".{file_name}."))
        .suffix(".tmp")
        .tempfile_in(&parent)?;
    tmp.write_all(content.as_bytes())?;
    tmp.as_file().sync_all()?;

    tmp.persist(path)
        .map_err(|err| TaskFileError::Io(err.error))?;
    sync_dir(&parent)?;
    Ok(())
}

pub fn archive_snapshot_if_completed(
    path: &Path,
    content: &str,
) -> TaskFileResult<Option<PathBuf>> {
    let tasks = parse_tasks(content)?;
    if tasks.is_empty() || tasks.iter().any(|task| task.status != "x") {
        return Ok(None);
    }

    let archive_dir = path
        .parent()
        .map(|parent| parent.join("archive"))
        .unwrap_or_else(|| PathBuf::from("archive"));
    fs::create_dir_all(&archive_dir)?;

    let mut target = archive_dir.join(path.file_name().ok_or_else(|| {
        TaskFileError::InvalidFormat(format!("invalid file name: {}", path.display()))
    })?);

    if target.exists() {
        let stem = path
            .file_stem()
            .and_then(|v| v.to_str())
            .unwrap_or("task")
            .to_string();
        let suffix = path
            .extension()
            .and_then(|v| v.to_str())
            .map(|v| format!(".{v}"))
            .unwrap_or_default();
        let mut index = 1usize;
        while target.exists() {
            target = archive_dir.join(format!("{stem}-{index}{suffix}"));
            index += 1;
        }
    }

    fs::rename(path, &target)?;
    if let Some(source_parent) = path.parent() {
        sync_dir(source_parent)?;
    }
    sync_dir(&archive_dir)?;

    Ok(Some(target))
}

pub fn apply_task_update<F>(
    snapshot: &TaskFileSnapshot,
    task_id: &str,
    updater: F,
) -> TaskFileResult<String>
where
    F: FnOnce(TaskBlock) -> TaskFileResult<TaskBlock>,
{
    let Some(target) = snapshot.tasks.iter().find(|task| {
        task.metadata
            .get("id")
            .map(|value| value == task_id)
            .unwrap_or(false)
    }) else {
        return Err(TaskFileError::TaskNotFound(task_id.to_string()));
    };

    let updated = updater(target.clone())?;

    let prefix = &snapshot.content[..target.start];
    let suffix = &snapshot.content[target.end..];
    let separator = if prefix.is_empty() || prefix.ends_with('\n') {
        ""
    } else {
        "\n"
    };

    Ok(format!(
        "{prefix}{separator}{}{suffix}",
        serialize_task(&updated)
    ))
}

pub fn claim_next_task(
    snapshot: &TaskFileSnapshot,
    run_id: &str,
    worker_id: &str,
) -> TaskFileResult<Option<(String, TaskBlock)>> {
    let Some(available) = snapshot
        .tasks
        .iter()
        .find(|task| task.status == " " || task.status == ">")
        .cloned()
    else {
        return Ok(None);
    };

    let content = apply_task_update(snapshot, &task_id_of(&available)?, |mut task| {
        task.status = ">".to_string();
        task.metadata
            .insert("last_run_id".to_string(), run_id.to_string());
        let now = utc_now_iso();
        task.metadata
            .insert("last_started_at".to_string(), now.clone());
        task.metadata.insert("last_heartbeat_at".to_string(), now);
        task.metadata
            .insert("last_result".to_string(), "running".to_string());
        task.metadata
            .insert("last_error".to_string(), String::new());
        task.metadata
            .insert("lease_owner".to_string(), worker_id.to_string());
        task.metadata
            .insert("lease_pid".to_string(), std::process::id().to_string());
        Ok(task)
    })?;

    Ok(Some((content, available)))
}

pub fn finalize_task(
    snapshot: &TaskFileSnapshot,
    claimed: &ClaimedTask,
    status: &str,
    result: &str,
    error: &str,
) -> TaskFileResult<String> {
    apply_task_update(snapshot, &task_id_of(&claimed.task)?, |mut task| {
        let owner = task.metadata.get("lease_owner");
        let run_id = task.metadata.get("last_run_id");
        if owner != Some(&claimed.worker_id) || run_id != Some(&claimed.run_id) {
            return Err(TaskFileError::OwnershipChanged("finalize"));
        }

        task.status = status.to_string();
        let now = utc_now_iso();
        task.metadata
            .insert("last_finished_at".to_string(), now.clone());
        task.metadata
            .insert("last_result".to_string(), result.to_string());
        task.metadata
            .insert("last_error".to_string(), error.to_string());
        task.metadata.insert("last_heartbeat_at".to_string(), now);
        Ok(task)
    })
}

pub fn refresh_heartbeat(
    snapshot: &TaskFileSnapshot,
    task_id: &str,
    worker_id: &str,
) -> TaskFileResult<String> {
    apply_task_update(snapshot, task_id, |mut task| {
        if task.metadata.get("lease_owner").map(String::as_str) != Some(worker_id) {
            return Err(TaskFileError::OwnershipChanged("heartbeat"));
        }
        task.metadata
            .insert("last_heartbeat_at".to_string(), utc_now_iso());
        Ok(task)
    })
}

pub fn recover_stale_tasks(
    snapshot: &TaskFileSnapshot,
    stale_after: f64,
) -> TaskFileResult<(String, Vec<String>)> {
    let mut content = snapshot.content.clone();
    let mut recovered: Vec<String> = Vec::new();

    for task in &snapshot.tasks {
        if task.status != ">" {
            continue;
        }

        let heartbeat = task.metadata.get("last_heartbeat_at").cloned();
        let pid = task
            .metadata
            .get("lease_pid")
            .and_then(|value| value.parse::<i32>().ok())
            .unwrap_or(0);

        let mut stale = match heartbeat {
            None => true,
            Some(value) => match parse_utc(&value) {
                Some(beat_dt) => {
                    let age = (Utc::now() - beat_dt).num_milliseconds() as f64 / 1000.0;
                    age > stale_after
                }
                None => true,
            },
        };

        let alive = process_exists(pid);
        if alive && !stale {
            continue;
        }
        if !stale && !alive {
            stale = true;
        }
        if !stale {
            continue;
        }

        let current_snapshot = TaskFileSnapshot {
            path: snapshot.path.clone(),
            content: content.clone(),
            tasks: parse_tasks(&content)?,
            sha256: sha256_text(&content),
            mtime_ns: snapshot.mtime_ns,
        };

        let task_id = task_id_of(task)?;
        content = apply_task_update(&current_snapshot, &task_id, |mut current| {
            current.status = "!".to_string();
            current
                .metadata
                .insert("last_finished_at".to_string(), utc_now_iso());
            current
                .metadata
                .insert("last_result".to_string(), "worker_interrupted".to_string());
            current.metadata.insert(
                "last_error".to_string(),
                "stale_running_recovered".to_string(),
            );
            Ok(current)
        })?;
        recovered.push(task_id);
    }

    Ok((content, recovered))
}

fn split_lines_keepends(content: &str) -> Vec<&str> {
    if content.is_empty() {
        return Vec::new();
    }

    let mut lines = Vec::new();
    let mut start = 0usize;
    for (index, ch) in content.char_indices() {
        if ch == '\n' {
            lines.push(&content[start..=index]);
            start = index + 1;
        }
    }
    if start < content.len() {
        lines.push(&content[start..]);
    }
    lines
}

fn task_id_of(task: &TaskBlock) -> TaskFileResult<String> {
    let Some(task_id) = task.metadata.get("id") else {
        return Err(TaskFileError::InvalidFormat(
            "task id is missing".to_string(),
        ));
    };

    if task_id.trim().is_empty() {
        return Err(TaskFileError::InvalidFormat("task id is empty".to_string()));
    }

    Ok(task_id.clone())
}

fn parse_utc(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&value.replace('Z', "+00:00"))
        .ok()
        .map(|parsed| parsed.with_timezone(&Utc))
}

fn sync_dir(path: &Path) -> TaskFileResult<()> {
    #[cfg(unix)]
    {
        let dir = fs::File::open(path)?;
        dir.sync_all()?;
    }
    Ok(())
}
