use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::events::record::EventRecord;
use crate::models::{RunSummary, TaskBlock};
use crate::util::{ensure_parent, hash8, slugify};

#[derive(Debug, Clone)]
pub struct RunPaths {
    pub task_dir: PathBuf,
    pub run_dir: PathBuf,
    pub subagents_dir: PathBuf,
    pub raw_unparsed_dir: PathBuf,
    pub problem_examples_dir: PathBuf,
    pub prompt: PathBuf,
    pub stdout: PathBuf,
    pub stderr: PathBuf,
    pub events: PathBuf,
    pub summary: PathBuf,
    pub task_json: PathBuf,
    pub runs_jsonl: PathBuf,
}

impl RunPaths {
    pub fn new(base_dir: &Path, task: &TaskBlock, run_id: &str, started_at: &str) -> Self {
        let task_id = task
            .task_id()
            .map(str::to_string)
            .unwrap_or_else(|_| "task".to_string());
        let task_slug = format!("{}--{}", slugify(&task_id, 48), hash8(&task_id));
        let run_slug = format!("{}--{}", started_at.replace([':', '-'], ""), run_id);

        let task_dir = base_dir.join("tasks").join(task_slug);
        let run_dir = task_dir.join("runs").join(run_slug);
        let subagents_dir = run_dir.join("subagents");
        let raw_unparsed_dir = run_dir.join("raw_unparsed");
        let problem_examples_dir = run_dir.join("problem_examples");

        Self {
            task_dir: task_dir.clone(),
            run_dir: run_dir.clone(),
            subagents_dir: subagents_dir.clone(),
            raw_unparsed_dir: raw_unparsed_dir.clone(),
            problem_examples_dir: problem_examples_dir.clone(),
            prompt: run_dir.join("prompt.md"),
            stdout: run_dir.join("stdout.jsonl"),
            stderr: run_dir.join("stderr.log"),
            events: run_dir.join("events.jsonl"),
            summary: run_dir.join("summary.json"),
            task_json: task_dir.join("task.json"),
            runs_jsonl: task_dir.join("runs.jsonl"),
        }
    }

    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.task_dir)?;
        std::fs::create_dir_all(&self.run_dir)?;
        std::fs::create_dir_all(&self.subagents_dir)?;
        std::fs::create_dir_all(&self.raw_unparsed_dir)?;
        std::fs::create_dir_all(&self.problem_examples_dir)?;
        Ok(())
    }
}

pub fn write_event(path: &Path, event: &EventRecord) -> std::io::Result<()> {
    ensure_parent(path)?;
    let mut handle = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    use std::io::Write;
    handle.write_all(serde_json::to_string(&event.to_dict())?.as_bytes())?;
    handle.write_all(b"\n")?;
    Ok(())
}

pub fn write_summary(path: &Path, summary: &RunSummary) -> std::io::Result<()> {
    ensure_parent(path)?;
    let mut payload = serde_json::to_value(summary)?;
    if let Some(obj) = payload.as_object_mut() {
        obj.insert(
            "failure_analysis".to_string(),
            summary.failure_analysis.clone(),
        );
    }
    std::fs::write(path, serde_json::to_string_pretty(&payload)? + "\n")?;
    Ok(())
}

pub fn update_task_snapshot(
    path: &Path,
    task: &TaskBlock,
    last_run_path: &str,
) -> std::io::Result<()> {
    ensure_parent(path)?;
    let mut payload = serde_json::Map::new();
    payload.insert(
        "task_id".to_string(),
        Value::from(task.task_id().unwrap_or_default().to_string()),
    );
    payload.insert("title".to_string(), Value::from(task.title.clone()));
    payload.insert("status".to_string(), Value::from(task.status.clone()));
    payload.insert(
        "metadata".to_string(),
        serde_json::to_value(&task.metadata)
            .unwrap_or_else(|_| Value::Object(serde_json::Map::new())),
    );
    payload.insert(
        "last_run_path".to_string(),
        Value::from(last_run_path.to_string()),
    );
    std::fs::write(
        path,
        serde_json::to_string_pretty(&Value::Object(payload))? + "\n",
    )?;
    Ok(())
}

pub fn append_task_run<T: Serialize>(path: &Path, payload: &T) -> std::io::Result<()> {
    ensure_parent(path)?;
    let mut handle = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    use std::io::Write;
    handle.write_all(serde_json::to_string(payload)?.as_bytes())?;
    handle.write_all(b"\n")?;
    Ok(())
}
