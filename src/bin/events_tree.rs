use std::fmt::Write as _;
use std::path::PathBuf;

use clap::Parser;
use codex_worker_rs::error::{AppError, AppResult};

#[path = "events_tree_shared.rs"]
mod events_tree_shared;

use events_tree_shared::{
    build_event_tree, format_event_line, load_records_from_run_input, ThreadNode, TimelineItem,
};

#[derive(Debug, Parser)]
#[command(
    name = "events_tree",
    about = "Отладочный вывод дерева событий из raw run-логов"
)]
struct Cli {
    #[arg(long = "input", alias = "events-file", value_name = "PATH")]
    input_path: PathBuf,

    #[arg(long, default_value_t = 120)]
    text_limit: usize,
}

fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(cli) {
        eprintln!("events_tree error: {err}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> AppResult<()> {
    if cli.input_path.as_os_str().is_empty() {
        return Err(AppError::EmptyPath(cli.input_path));
    }

    let (source_path, events) = load_records_from_run_input(&cli.input_path)?;
    let tree = build_event_tree(&source_path, &events, cli.text_limit);
    print!("{}", render_text_tree(&tree));
    Ok(())
}

fn render_text_tree(tree: &events_tree_shared::EventTree) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "file: {}", tree.source_path.display());
    let _ = writeln!(
        out,
        "run: task_id={} run_id={} events={} threads={}",
        tree.task_id, tree.run_id, tree.event_count, tree.thread_count
    );

    for node in &tree.roots {
        render_thread(&mut out, node, 0);
    }

    if !tree.orphan_events.is_empty() {
        let _ = writeln!(out, "orphan-events:");
        for event in &tree.orphan_events {
            let _ = writeln!(out, "  {}", format_event_line(event));
        }
    }

    out
}

fn render_thread(out: &mut String, node: &ThreadNode, depth: usize) {
    let indent = "  ".repeat(depth);
    let mut meta = Vec::new();
    if node.is_root {
        meta.push("root".to_string());
    }
    if let Some(status) = node.status.as_deref() {
        meta.push(format!("status={status}"));
    }
    if let Some(role) = node.role.as_deref() {
        meta.push(format!("role={role}"));
    }
    if let Some(nickname) = node.nickname.as_deref() {
        meta.push(format!("nickname={nickname}"));
    }
    if let Some(cwd) = node.cwd.as_deref() {
        meta.push(format!("cwd={cwd}"));
    }
    meta.push(format!("events={}", node.event_count));
    if node.child_thread_count > 0 {
        meta.push(format!("children={}", node.child_thread_count));
    }

    let _ = writeln!(out, "{indent}thread {} {}", node.thread_id, meta.join(" "));

    for item in &node.items {
        render_timeline_item(out, item, depth + 1);
    }
}

fn render_timeline_item(out: &mut String, item: &TimelineItem, depth: usize) {
    let indent = "  ".repeat(depth);
    match item {
        TimelineItem::Event(node) => {
            let parent_suffix = node
                .event
                .parent_event_id
                .as_deref()
                .map(|value| format!(" parent={value}"))
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "{indent}{}{}",
                format_event_line(&node.event),
                parent_suffix
            );
            for child in &node.children {
                render_timeline_item(out, child, depth + 1);
            }
        }
        TimelineItem::Thread(thread) => {
            render_thread(out, thread, depth);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::run;

    #[test]
    fn events_tree_accepts_run_artifact_path() {
        let run_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
            "target/manual-smoke/.codex-worker/tasks/all-operation-emulation-on-sub-agent--71f2caf3/runs/20260406T145909Z--18a3cc84d99c426e-2",
        );

        let result = run(super::Cli {
            input_path: run_dir.join("stdout.jsonl"),
            text_limit: 120,
        });

        assert!(result.is_ok());
    }

    #[test]
    fn events_tree_rejects_non_run_path() {
        let err = run(super::Cli {
            input_path: PathBuf::from("/tmp/not-a-run/events.jsonl"),
            text_limit: 120,
        })
        .expect_err("non-run path should fail");

        assert!(err.to_string().contains("путь не распознан"));
    }
}
