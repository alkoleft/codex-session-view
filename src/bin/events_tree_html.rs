use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Parser;
use codex_worker_rs::error::{AppError, AppResult};
use codex_worker_rs::events::projector::EventSummaryCategory;
use codex_worker_rs::events::types::{
    COLLAB_CLOSE_AGENT, COLLAB_RESUME_AGENT, COLLAB_SEND_INPUT, COLLAB_SPAWN_AGENT, COLLAB_WAIT,
    INFO_TOKENS, MESSAGE_USER, RUNTIME_CONTEXT, SHELL_CALL, SHELL_RESULT, TASK_COMPLETED,
    TASK_STARTED, USER_INPUT_REQUEST,
};

#[path = "events_tree_shared.rs"]
mod events_tree_shared;

use events_tree_shared::{
    build_event_tree, build_event_tree_with_standalone_startup_metadata, is_rollout_jsonl_family,
    is_run_input, load_records_from_run_input, load_records_from_standalone_rollout,
    validate_standalone_rollout_root, EventEntry, EventNode, EventTree, ThreadNode, TimelineItem,
    UserInputAnswerEntry, UserInputOptionEntry, UserInputQuestionEntry, UserInputRequestEntry,
};

#[derive(Debug, Parser)]
#[command(
    name = "events_tree_html",
    about = "Генерирует HTML-визуализацию дерева событий из raw run-логов"
)]
struct Cli {
    #[arg(long = "input", alias = "events-file", value_name = "PATH")]
    input_path: PathBuf,

    #[arg(long)]
    output_file: Option<PathBuf>,

    #[arg(long, default_value_t = 180)]
    text_limit: usize,

    #[arg(long)]
    no_open: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct TokenUsage {
    input_tokens: u64,
    cached_input_tokens: u64,
    output_tokens: u64,
    reasoning_output_tokens: u64,
    total_tokens: u64,
}

fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(cli) {
        eprintln!("events_tree_html error: {err}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> AppResult<()> {
    if cli.input_path.as_os_str().is_empty() {
        return Err(AppError::EmptyPath(cli.input_path));
    }

    let (source_path, events, standalone_startup_metadata) = if is_run_input(&cli.input_path) {
        let (source_path, events) = load_records_from_run_input(&cli.input_path)?;
        (source_path, events, None)
    } else if is_rollout_jsonl_family(&cli.input_path) {
        let session_id = validate_standalone_rollout_root(&cli.input_path)?;
        let loaded = load_records_from_standalone_rollout(&cli.input_path, &session_id)?;
        (
            cli.input_path.clone(),
            loaded.events,
            Some(loaded.startup_metadata),
        )
    } else {
        return Err(AppError::Runner(format!(
            "invalid input: ожидалась run-директория/артефакт или rollout-*.jsonl, получено {}",
            cli.input_path.display()
        )));
    };
    let tree = if let Some(startup_metadata) = standalone_startup_metadata {
        build_event_tree_with_standalone_startup_metadata(
            &source_path,
            &events,
            Some(startup_metadata),
            cli.text_limit,
        )
    } else {
        build_event_tree(&source_path, &events, cli.text_limit)
    };
    let output_file = cli
        .output_file
        .unwrap_or_else(|| default_output_path(&source_path));
    let html = render_html(&tree);

    fs::write(&output_file, html)?;
    println!("{}", output_file.display());

    if !cli.no_open {
        if let Err(err) = open_in_browser(&output_file) {
            eprintln!("events_tree_html warning: {err}");
        }
    }

    Ok(())
}

fn default_output_path(source_path: &Path) -> PathBuf {
    if source_path.is_dir() {
        return source_path.join("events.tree.html");
    }

    let stem = source_path
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or("events");
    source_path.with_file_name(format!("{stem}.tree.html"))
}

fn open_in_browser(path: &Path) -> AppResult<()> {
    let absolute_path = fs::canonicalize(path)?;
    let file_url = format!("file://{}", absolute_path.display());

    for browser in [
        "firefox",
        "google-chrome",
        "chromium",
        "chromium-browser",
        "brave-browser",
        "brave",
    ] {
        if Command::new(browser).arg(&file_url).spawn().is_ok() {
            return Ok(());
        }
    }

    let opener_candidates = [
        ("xdg-open", vec![file_url.as_str()]),
        ("gio", vec!["open", file_url.as_str()]),
        ("open", vec![file_url.as_str()]),
    ];
    for (program, args) in opener_candidates {
        if Command::new(program)
            .args(args)
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
        {
            return Ok(());
        }
    }

    Err(AppError::Runner(format!(
        "не удалось открыть браузер для {}",
        path.display()
    )))
}

const PAGE_STYLE: &str = r#"body{margin:0;font:13px/1.45 ui-monospace,SFMono-Regular,Menlo,Monaco,Consolas,monospace;background:#f6f3ee;color:#1f2937;}
.page{max-width:1280px;margin:0 auto;padding:20px;}
.hero{display:flex;flex-direction:column;gap:14px;background:linear-gradient(135deg,#fcf7ea,#eff5ff);border:1px solid #d8dee9;border-radius:18px;padding:18px 20px;box-shadow:0 10px 28px rgba(15,23,42,.06);}
.hero-head{display:flex;justify-content:space-between;align-items:flex-start;gap:14px;flex-wrap:wrap;}
.hero-title{display:flex;flex-direction:column;gap:4px;min-width:0;}
h1{margin:0;font-size:26px;line-height:1.1;}
.hero-lead{color:#475569;max-width:68ch;font-size:13px;}
.hero-source{display:flex;flex-direction:column;gap:4px;min-width:min(360px,100%);max-width:100%;padding:10px 12px;border:1px solid #d8dee9;border-radius:14px;background:rgba(255,255,255,.74);}
.hero-source-label{font-size:11px;text-transform:uppercase;letter-spacing:.08em;font-weight:700;color:#64748b;}
.hero-source-path{white-space:pre-wrap;word-break:break-word;color:#334155;font-size:13px;}
.hero-source-path code{background:none;border:none;padding:0;color:inherit;}
.hero-grid{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:10px;}
.hero-card{display:flex;flex-direction:column;gap:6px;min-width:0;padding:10px 12px;border:1px solid #d8dee9;border-radius:14px;background:rgba(255,255,255,.82);}
.hero-card-label{font-size:10px;text-transform:uppercase;letter-spacing:.08em;font-weight:700;color:#64748b;}
.hero-card-value{font-size:14px;font-weight:700;color:#0f172a;white-space:pre-wrap;word-break:break-word;}
.hero-card-value code{background:none;border:none;padding:0;color:inherit;font-size:inherit;}
.hero-metrics{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:8px;}
.hero-metric{display:flex;flex-direction:column;gap:2px;padding-top:2px;border-top:1px dashed #d8dee9;}
.hero-metric-label{font-size:10px;text-transform:uppercase;letter-spacing:.04em;color:#64748b;}
.hero-metric-value{font-size:18px;font-weight:800;color:#0f172a;line-height:1.1;}
.hero-startup{display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:10px;}
.hero-base{display:flex;flex-direction:column;gap:8px;}
.hero-base .inset-block{margin-top:0;}
.hero-startup-card{display:flex;flex-direction:column;gap:8px;min-width:0;padding:10px 12px;border:1px solid #d8dee9;border-radius:14px;background:rgba(255,255,255,.66);}
.hero-startup-value{color:#334155;white-space:pre-wrap;word-break:break-word;font-size:13px;}
.hero-startup-value code{background:none;border:none;padding:0;color:inherit;font-size:inherit;}
.hero-startup-rows{display:flex;flex-direction:column;gap:8px;}
.hero-startup-row{display:flex;justify-content:space-between;align-items:flex-start;gap:10px;padding-top:8px;border-top:1px dashed #d8dee9;}
.hero-startup-row:first-child{padding-top:0;border-top:none;}
.hero-startup-row-label{font-size:10px;text-transform:uppercase;letter-spacing:.04em;color:#64748b;flex:0 0 auto;}
.hero-startup-row-value{min-width:0;text-align:right;color:#0f172a;white-space:pre-wrap;word-break:break-word;font-size:13px;}
.hero-startup-row-value code{background:none;border:none;padding:0;color:inherit;font-size:inherit;}
.kv-table{width:100%;border-collapse:collapse;table-layout:fixed;}
.kv-table-row+.kv-table-row{border-top:1px dashed #d8dee9;}
.kv-table-label{width:32%;padding:8px 10px 8px 0;font-size:10px;text-transform:uppercase;letter-spacing:.04em;color:#64748b;text-align:left;vertical-align:top;}
.kv-table-value{padding:8px 0 8px 10px;text-align:left;color:#0f172a;white-space:pre-wrap;word-break:break-word;font-size:13px;vertical-align:top;}
.kv-table-value code{background:none;border:none;padding:0;color:inherit;font-size:inherit;}
.pill{display:inline-flex;align-items:center;gap:6px;padding:4px 9px;border-radius:999px;background:#fff;border:1px solid #d8dee9;color:#334155;font-size:12px;}
.inset-block{position:relative;margin-top:8px;padding:16px 12px 12px;border:1px solid #d8dee9;border-radius:14px;background:rgba(255,255,255,.94);box-shadow:inset 0 1px 0 rgba(255,255,255,.85);}
.inset-block.inline-title{padding-top:12px;}
.inset-block-title{position:absolute;top:0;left:12px;transform:translateY(-50%);display:inline-flex;align-items:center;padding:3px 10px;border-radius:999px;background:#ece8df;border:1px solid #d8dee9;color:#475569;font-size:12px;font-weight:600;}
.inset-block-title.inline-title{display:block;position:static;transform:none;margin-bottom:10px;padding:0;border:none;border-radius:0;background:none;font-size:10px;text-transform:uppercase;letter-spacing:.08em;font-weight:700;color:#64748b;}
.inset-block-body{display:flex;flex-direction:column;gap:10px;min-width:0;}
.inset-block-footer{display:flex;justify-content:flex-end;align-items:center;margin-top:8px;font-size:12px;color:#64748b;}
.inset-block .message-collapse{width:100%;}
.shell-block-command{white-space:pre-wrap;word-break:break-word;font-size:14px;line-height:1.45;color:#0f172a;}
.shell-block-output{color:#475569;}
.shell-block-empty{color:#94a3b8;}
.shell-block-status{display:inline-flex;align-items:center;gap:6px;}
.shell-block-status.is-failure{color:#b91c1c;}
.plan-explanation{color:#334155;}
.plan-steps{display:flex;flex-direction:column;gap:8px;}
.plan-step{display:flex;align-items:flex-start;gap:10px;padding:8px 10px;border:1px solid #d8dee9;border-radius:12px;background:#f8fafc;}
.plan-step-index{flex:0 0 auto;min-width:20px;color:#94a3b8;font-size:12px;font-weight:700;line-height:1.6;}
.plan-step-text{flex:1 1 auto;white-space:pre-wrap;word-break:break-word;color:#0f172a;}
.plan-step-status{flex:0 0 auto;display:inline-flex;align-items:center;padding:2px 8px;border-radius:999px;border:1px solid #d8dee9;background:#fff;color:#475569;font-size:11px;text-transform:uppercase;letter-spacing:.04em;}
.plan-step-status.is-completed{background:#e8f7ec;color:#166534;border-color:#b7e4c7;}
.plan-step-status.is-in-progress{background:#e6f0ff;color:#1d4ed8;border-color:#bfdbfe;}
.plan-step-status.is-pending{background:#fff7d6;color:#92400e;border-color:#fde68a;}
.user-input-questions{display:flex;flex-direction:column;gap:10px;}
.user-input-question{display:flex;flex-direction:column;gap:8px;padding:10px;border:1px solid #d8dee9;border-radius:12px;background:#f8fafc;}
.user-input-question-head{display:flex;flex-wrap:wrap;gap:6px 8px;align-items:center;}
.user-input-question-tag{display:inline-flex;align-items:center;gap:6px;padding:2px 8px;border-radius:999px;border:1px solid #d8dee9;background:#fff;color:#475569;font-size:11px;}
.user-input-question-tag-label{font-size:10px;text-transform:uppercase;letter-spacing:.04em;color:#94a3b8;}
.user-input-question-text{color:#0f172a;white-space:pre-wrap;word-break:break-word;}
.user-input-options{display:flex;flex-direction:column;gap:8px;}
.user-input-option{display:flex;flex-direction:column;gap:6px;padding:8px 10px;border:1px solid #d8dee9;border-radius:10px;background:#fff;}
.user-input-option.is-selected{background:#e8f7ec;border-color:#b7e4c7;}
.user-input-option-head{display:flex;justify-content:space-between;align-items:flex-start;gap:8px;flex-wrap:wrap;}
.user-input-option-label{font-weight:700;color:#0f172a;white-space:pre-wrap;word-break:break-word;}
.user-input-option-description{color:#475569;white-space:pre-wrap;word-break:break-word;}
.user-input-answer-list{display:flex;flex-wrap:wrap;gap:6px;}
.user-input-answer-chip{display:inline-flex;align-items:center;padding:2px 8px;border-radius:999px;border:1px solid #d8dee9;background:#fff;color:#334155;font-size:12px;}
.user-input-answer-chip.is-selected{background:#dcfce7;border-color:#86efac;color:#166534;}
.user-input-extra-answers{display:flex;flex-direction:column;gap:8px;}
.user-input-extra-answer{display:flex;align-items:flex-start;gap:10px;flex-wrap:wrap;padding:8px 10px;border:1px solid #d8dee9;border-radius:10px;background:#fff;}
.user-input-extra-answer-id{font-size:11px;font-weight:700;color:#64748b;}
.tree{margin-top:20px;}
.children{margin:12px 0 0 22px;padding-left:14px;border-left:2px solid #d8dee9;}
details.thread{margin:12px 0;border:1px solid #d8dee9;border-radius:16px;background:#fff;box-shadow:0 8px 18px rgba(15,23,42,.04);}
details.thread[open]{background:#fffdfa;}
details.thread>summary{cursor:pointer;list-style:none;padding:14px 16px;display:flex;flex-wrap:wrap;gap:8px 10px;align-items:center;}
summary::-webkit-details-marker{display:none;}
.thread-id{font-size:15px;font-weight:700;color:#0f172a;}
.thread-body{padding:0 16px 16px;}
.thread-flow{display:flex;flex-direction:column;margin-top:10px;}
.thread-flow>*+*{position:relative;margin-top:0;padding-top:16px;}
.thread-flow>*+*::before{content:"";position:absolute;top:0;left:0;right:0;border-top:1px dashed #98a6b9;}
.task-lifecycle{position:relative;margin:2px 0;padding-left:28px;}
.task-lifecycle::before{content:"";position:absolute;top:8px;bottom:8px;left:10px;width:2px;border-radius:999px;background:linear-gradient(180deg,#7c8ea3 0%,#cbd5e1 100%);}
.task-lifecycle.is-open::before{background:linear-gradient(180deg,#2563eb 0%,rgba(37,99,235,.18) 100%);}
.task-lifecycle-items{position:relative;}
.task-lifecycle-items.thread-flow{margin-top:0;}
.event-footnote{margin:6px calc(50% - 50vw) 0;padding:0 20px;background:linear-gradient(90deg,rgba(253,246,227,.96),rgba(231,240,255,.96));border-top:1px dashed #d8dee9;border-bottom:1px solid #d8dee9;}
.event-footnote-content{display:flex;flex-wrap:wrap;gap:8px 12px;align-items:baseline;padding:8px 0 10px;}
.event-footnote-seq{display:inline-flex;align-items:center;padding:2px 8px;border-radius:999px;background:#fff;border:1px solid #d8dee9;color:#334155;font-size:11px;}
.event-footnote-title{font-size:10px;text-transform:uppercase;letter-spacing:.08em;font-weight:700;color:#475569;}
.event-footnote-pair{display:inline-flex;align-items:baseline;gap:6px;}
.event-footnote-label{font-size:10px;text-transform:uppercase;letter-spacing:.04em;color:#64748b;}
.event-footnote-value{font-size:12px;font-weight:400;color:#0f172a;}
.event-footnote-diff{font-size:13px;font-weight:800;color:#64748b;}
.event-footnote-pair.is-total .event-footnote-label{font-size:10px;font-weight:700;}
.event-footnote-pair.is-total .event-footnote-value{font-size:12px;font-weight:400;}
.event-footnote-pair.is-total .event-footnote-diff{font-size:13px;font-weight:800;}
.diff-pos{color:#166534;}
.diff-neg{color:#b91c1c;}
.event-card{display:flex;flex-direction:column;gap:8px;padding:10px 0;border:none;border-radius:0;background:transparent;box-shadow:none;}
.event-card.is-task-started,.event-card.is-task-completed{position:relative;}
.event-card.is-task-started::before,.event-card.is-task-completed::before{content:"";position:absolute;top:14px;left:-24px;width:10px;height:10px;border-radius:999px;box-shadow:0 0 0 4px rgba(246,243,238,.96);}
.event-card.is-task-started::before{background:#2563eb;border:2px solid #2563eb;}
.event-card.is-task-completed::before{background:#f8fffb;border:2px solid #16a34a;}
.event-card.is-user-prompt{margin:4px 0;padding:12px 14px;border:1px solid #86efac;border-radius:16px;background:linear-gradient(180deg,#f4fff6,#ecfdf3);box-shadow:inset 0 1px 0 rgba(255,255,255,.92);}
.event-card.is-user-prompt .badge{background:#dcfce7;color:#166534;border-color:#86efac;}
.event-card.is-user-prompt .event-ts{color:#15803d;}
.event-card.is-user-prompt .event-meta-value{color:#166534;}
.event-card.is-user-prompt .summary-text{color:#14532d;}
.event-card.is-user-prompt .event-section-label{color:#16a34a;}
.event-header{display:flex;justify-content:space-between;align-items:flex-start;gap:8px 12px;flex-wrap:wrap;}
.event-header-main{display:flex;align-items:center;gap:8px;flex-wrap:wrap;min-width:0;}
.event-ts{font-size:12px;color:#64748b;white-space:nowrap;}
.event-meta{display:flex;flex-wrap:wrap;gap:8px 12px;font-size:12px;color:#64748b;}
.event-meta-item{display:inline-flex;align-items:baseline;gap:6px;min-width:0;}
.event-meta-label{font-size:10px;text-transform:uppercase;letter-spacing:.04em;color:#94a3b8;}
.event-meta-value{min-width:0;color:#475569;white-space:pre-wrap;word-break:break-word;}
.event-summary{display:flex;flex-direction:column;gap:6px;padding-top:8px;border-top:none;}
.event-detail{display:flex;flex-direction:column;gap:6px;padding-top:8px;border-top:1px dashed #d2dae4;}
.event-section-label{font-size:10px;text-transform:uppercase;letter-spacing:.08em;font-weight:700;color:#94a3b8;}
.seq-chip{display:inline-flex;align-items:center;padding:3px 8px;border-radius:999px;background:#0f172a;color:#fff;font-weight:700;font-size:12px;}
.summary-text{white-space:pre-wrap;word-break:break-word;color:#0f172a;}
.message-collapse{display:flex;flex-direction:column;align-items:flex-start;gap:8px;}
.message-toggle{padding:4px 10px;border-radius:999px;border:1px solid #cbd5e1;background:#f8fafc;color:#334155;font:inherit;font-size:12px;cursor:pointer;}
.message-toggle:hover{background:#eef2f7;}
.message-collapse[data-expanded="false"] .message-full{display:none;}
.message-collapse[data-expanded="true"] .message-preview{display:none;}
.summary-structured{display:flex;flex-wrap:wrap;gap:8px 12px;align-items:baseline;color:#0f172a;}
.summary-title{font-weight:700;color:#334155;}
.summary-pair{display:inline-flex;align-items:baseline;gap:6px;}
.summary-label{font-size:11px;text-transform:uppercase;letter-spacing:.04em;color:#64748b;}
.summary-value{font-weight:700;color:#111827;}
.badge{display:inline-flex;align-items:center;padding:3px 8px;border-radius:999px;font-size:12px;border:1px solid transparent;}
.cat-default{background:#eef2f7;color:#334155;border-color:#d8dee9;}
.cat-assistant{background:#e8f7ec;color:#166534;border-color:#b7e4c7;}
.cat-command{background:#e6f0ff;color:#1d4ed8;border-color:#bfdbfe;}
.cat-search{background:#e6fffb;color:#0f766e;border-color:#99f6e4;}
.cat-subagent{background:#f8e8ff;color:#86198f;border-color:#f0abfc;}
.cat-file{background:#fff7d6;color:#92400e;border-color:#fde68a;}
.cat-todo{background:#ecfeff;color:#155e75;border-color:#a5f3fc;}
.cat-error{background:#fee2e2;color:#b91c1c;border-color:#fecaca;}
.empty{margin-top:10px;padding:12px;border:1px dashed #d8dee9;border-radius:12px;color:#64748b;background:#fafaf9;}
@media (max-width:1200px){.hero-grid{grid-template-columns:repeat(2,minmax(0,1fr));}}
@media (max-width:980px){.hero-source{min-width:0;width:100%;}.event-footnote{padding:0 16px;}}
@media (max-width:640px){.page{padding:16px;}.hero{padding:16px;}.hero-grid{grid-template-columns:1fr;}.thread-body{padding:0 12px 12px;}.children{margin-left:16px;padding-left:12px;}.task-lifecycle{padding-left:24px;}.task-lifecycle::before{left:8px;}.event-card.is-task-started::before,.event-card.is-task-completed::before{left:-20px;}}
code{background:#f8fafc;padding:2px 6px;border-radius:6px;border:1px solid #e2e8f0;}"#;

const PAGE_SCRIPT: &str = r#"function toggleMessageBlock(button){var block=button.closest('.message-collapse');if(!block){return;}var expanded=block.getAttribute('data-expanded')==='true';var nextState=expanded?'false':'true';block.setAttribute('data-expanded',nextState);button.setAttribute('aria-expanded',nextState);button.textContent=expanded?'see full':'collapse';}"#;

fn render_html(tree: &EventTree) -> String {
    let mut out = String::new();
    out.push_str("<!doctype html><html lang=\"ru\"><head><meta charset=\"utf-8\">");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    out.push_str("<title>Events Tree</title><style>");
    out.push_str(PAGE_STYLE);
    out.push_str("</style><script>");
    out.push_str(PAGE_SCRIPT);
    out.push_str("</script></head><body><main class=\"page\">");
    let startup_cards = tree
        .standalone_startup_metadata
        .as_ref()
        .map(render_startup_metadata_cards)
        .unwrap_or_default();
    let base_instructions_panel = tree
        .standalone_startup_metadata
        .as_ref()
        .and_then(render_base_instructions_panel);
    let startup_cards_block = if startup_cards.is_empty() {
        String::new()
    } else {
        format!("<div class=\"hero-startup\">{startup_cards}</div>")
    };
    let base_instructions_block = base_instructions_panel
        .as_deref()
        .map(|panel| format!("<div class=\"hero-base\">{panel}</div>"))
        .unwrap_or_default();
    let _ = write!(
        out,
        "<section class=\"hero\">\
         <div class=\"hero-head\">\
         <div class=\"hero-title\"><h1>Дерево событий</h1><div class=\"hero-lead\">Потоки, операции и системные метаданные одного run-лога в одной ленте.</div></div>\
         <div class=\"hero-source\"><span class=\"hero-source-label\">Источник</span><div class=\"hero-source-path\"><code>{}</code></div></div>\
         </div>\
         <div class=\"hero-grid\">{}{}{}{}\
         </div>{}{}\
         </section>",
        escape_html(&tree.source_path.display().to_string()),
        render_hero_card(
            "task_id",
            &format!("<code>{}</code>", escape_html(&tree.task_id)),
        ),
        render_hero_card(
            "run_id",
            &format!("<code>{}</code>", escape_html(&tree.run_id)),
        ),
        render_hero_overview_card(tree.event_count, tree.thread_count),
        render_hero_card(
            "root",
            &format!("<code>{}</code>", escape_html(&tree.root_thread_id)),
        ),
        startup_cards_block,
        base_instructions_block,
    );
    out.push_str("<section class=\"tree\">");
    for node in &tree.roots {
        render_thread_html(&mut out, node, 0);
    }
    if !tree.orphan_events.is_empty() {
        out.push_str("<section class=\"children\"><details class=\"thread\" open><summary><span class=\"thread-id\">orphan-events</span></summary><div class=\"thread-body\">");
        render_event_flow(&mut out, &tree.orphan_events);
        out.push_str("</div></details></section>");
    }
    out.push_str("</section>");
    out.push_str("</main></body></html>");
    out
}

fn render_thread_html(out: &mut String, node: &ThreadNode, depth: usize) {
    let container_class = if depth == 0 { "tree-root" } else { "children" };
    let open = if depth <= 1 { " open" } else { "" };
    let mut last_token_usage = TokenUsage::default();

    let mut meta = Vec::new();
    if node.is_root {
        meta.push("<span class=\"pill\">root</span>".to_string());
    }
    if let Some(status) = node.status.as_deref() {
        meta.push(format!(
            "<span class=\"pill\">status <strong>{}</strong></span>",
            escape_html(status)
        ));
    }
    if let Some(role) = node.role.as_deref() {
        meta.push(format!(
            "<span class=\"pill\">role <strong>{}</strong></span>",
            escape_html(role)
        ));
    }
    if let Some(nickname) = node.nickname.as_deref() {
        meta.push(format!(
            "<span class=\"pill\">nickname <strong>{}</strong></span>",
            escape_html(nickname)
        ));
    }
    if let Some(cwd) = node.cwd.as_deref() {
        meta.push(format!(
            "<span class=\"pill\">cwd <code>{}</code></span>",
            escape_html(cwd)
        ));
    }
    meta.push(format!(
        "<span class=\"pill\">events <strong>{}</strong></span>",
        node.event_count
    ));
    if node.child_thread_count > 0 {
        meta.push(format!(
            "<span class=\"pill\">children <strong>{}</strong></span>",
            node.child_thread_count
        ));
    }
    if let Some(parent_event_id) = node.parent_event_id.as_deref() {
        meta.push(format!(
            "<span class=\"pill\">parent_event <code>{}</code></span>",
            escape_html(parent_event_id)
        ));
    }

    let _ = write!(
        out,
        "<section class=\"{}\" data-thread-id=\"{}\" data-depth=\"{}\" data-parent-event-id=\"{}\"><details class=\"thread\"{}><summary>\
         <span class=\"thread-id\">{}</span>{}</summary><div class=\"thread-body\">",
        container_class,
        escape_html(&node.thread_id),
        depth,
        escape_html(node.parent_event_id.as_deref().unwrap_or("")),
        open,
        escape_html(&node.thread_id),
        meta.join("")
    );

    render_timeline_items(out, &node.items, depth, &mut last_token_usage);

    out.push_str("</div></details></section>");
}

fn render_timeline_items(
    out: &mut String,
    items: &[TimelineItem],
    depth: usize,
    last_token_usage: &mut TokenUsage,
) {
    if items.is_empty() {
        out.push_str("<div class=\"empty\">Для этого потока нет событий.</div>");
        return;
    }

    out.push_str("<div class=\"thread-flow\">");
    render_timeline_items_internal(out, items, depth, last_token_usage, true);
    out.push_str("</div>");
}

fn render_timeline_items_internal(
    out: &mut String,
    items: &[TimelineItem],
    depth: usize,
    last_token_usage: &mut TokenUsage,
    group_task_lifecycles: bool,
) {
    let mut index = 0usize;
    while index < items.len() {
        if group_task_lifecycles {
            if let Some((end_index, is_closed)) = task_lifecycle_segment_end(items, index) {
                render_task_lifecycle_segment(
                    out,
                    &items[index..=end_index],
                    depth,
                    last_token_usage,
                    is_closed,
                );
                index = end_index + 1;
                continue;
            }
        }
        render_timeline_item(out, &items[index], depth, last_token_usage);
        index += 1;
    }
}

fn render_event_flow(out: &mut String, events: &[EventEntry]) {
    if events.is_empty() {
        out.push_str("<div class=\"empty\">Сиротских событий нет.</div>");
        return;
    }

    let mut last_token_usage = TokenUsage::default();
    out.push_str("<div class=\"thread-flow\">");
    for event in events {
        render_event_card(out, event, &mut last_token_usage);
    }
    out.push_str("</div>");
}

fn render_timeline_item(
    out: &mut String,
    item: &TimelineItem,
    depth: usize,
    last_token_usage: &mut TokenUsage,
) {
    match item {
        TimelineItem::Event(node) => render_event_node(out, node, depth, last_token_usage),
        TimelineItem::Thread(thread) => render_thread_html(out, thread, depth + 1),
    }
}

fn render_task_lifecycle_segment(
    out: &mut String,
    items: &[TimelineItem],
    depth: usize,
    last_token_usage: &mut TokenUsage,
    is_closed: bool,
) {
    let class_name = if is_closed {
        "task-lifecycle is-closed"
    } else {
        "task-lifecycle is-open"
    };
    let _ = write!(out, "<section class=\"{}\">", class_name);
    out.push_str("<div class=\"thread-flow task-lifecycle-items\">");
    render_timeline_items_internal(out, items, depth, last_token_usage, false);
    out.push_str("</div></section>");
}

fn render_event_node(
    out: &mut String,
    node: &EventNode,
    depth: usize,
    last_token_usage: &mut TokenUsage,
) {
    if let Some(shell_result_index) = paired_command_shell_result_child_index(node) {
        let shell_result = match &node.children[shell_result_index] {
            TimelineItem::Event(child) => child,
            TimelineItem::Thread(_) => unreachable!("shell result child must be an event"),
        };
        render_combined_shell_operation_card(out, &node.event, &shell_result.event);

        let combined_children = merged_shell_operation_children(node, shell_result_index);
        if !combined_children.is_empty() {
            let _ = depth;
            out.push_str("<div class=\"children\">");
            render_timeline_items(out, &combined_children, depth + 1, last_token_usage);
            out.push_str("</div>");
        }
        return;
    }
    if let Some(spawn_result_index) = paired_spawn_agent_result_child_index(node) {
        let spawn_result = match &node.children[spawn_result_index] {
            TimelineItem::Event(child) => child,
            TimelineItem::Thread(_) => unreachable!("spawn result child must be an event"),
        };
        render_combined_spawn_agent_operation_card(out, &node.event, &spawn_result.event);

        let combined_children = merged_spawn_agent_operation_children(node, spawn_result_index);
        if !combined_children.is_empty() {
            let _ = depth;
            out.push_str("<div class=\"children\">");
            render_timeline_items(out, &combined_children, depth + 1, last_token_usage);
            out.push_str("</div>");
        }
        return;
    }
    if let Some(user_input_result_index) = paired_user_input_request_result_child_index(node) {
        let user_input_result = match &node.children[user_input_result_index] {
            TimelineItem::Event(child) => child,
            TimelineItem::Thread(_) => {
                unreachable!("user input result child must be an event")
            }
        };
        render_combined_user_input_request_operation_card(
            out,
            &node.event,
            &user_input_result.event,
        );

        let combined_children =
            merged_user_input_request_operation_children(node, user_input_result_index);
        if !combined_children.is_empty() {
            let _ = depth;
            out.push_str("<div class=\"children\">");
            render_timeline_items(out, &combined_children, depth + 1, last_token_usage);
            out.push_str("</div>");
        }
        return;
    }
    if let Some(collab_result_index) = paired_collab_operation_result_child_index(node) {
        let collab_result = match &node.children[collab_result_index] {
            TimelineItem::Event(child) => child,
            TimelineItem::Thread(_) => unreachable!("collab result child must be an event"),
        };
        render_combined_collab_operation_card(out, &node.event, &collab_result.event);

        let combined_children = merged_collab_operation_children(node, collab_result_index);
        if !combined_children.is_empty() {
            let _ = depth;
            out.push_str("<div class=\"children\">");
            render_timeline_items(out, &combined_children, depth + 1, last_token_usage);
            out.push_str("</div>");
        }
        return;
    }

    render_event_card(out, &node.event, last_token_usage);
    if !node.children.is_empty() {
        let _ = depth;
        out.push_str("<div class=\"children\">");
        render_timeline_items(out, &node.children, depth + 1, last_token_usage);
        out.push_str("</div>");
    }
}

fn task_lifecycle_segment_end(items: &[TimelineItem], start_index: usize) -> Option<(usize, bool)> {
    let start_event = timeline_item_event(items.get(start_index)?)?;
    if !is_task_started_event(start_event) {
        return None;
    }

    let start_turn_id = start_event.turn_id.as_deref();
    let mut fallback_end = items.len().saturating_sub(1);

    for (index, item) in items.iter().enumerate().skip(start_index + 1) {
        let Some(event) = timeline_item_event(item) else {
            continue;
        };

        if is_task_completed_event(event) {
            let same_turn = match (start_turn_id, event.turn_id.as_deref()) {
                (Some(left), Some(right)) => left == right,
                _ => true,
            };
            if same_turn {
                return Some((index, true));
            }
        }

        if is_task_started_event(event) {
            fallback_end = index.saturating_sub(1);
            break;
        }
    }

    Some((fallback_end.max(start_index), false))
}

fn timeline_item_event(item: &TimelineItem) -> Option<&EventEntry> {
    match item {
        TimelineItem::Event(node) => Some(&node.event),
        TimelineItem::Thread(_) => None,
    }
}

fn paired_command_shell_result_child_index(node: &EventNode) -> Option<usize> {
    if node.event.event_type != SHELL_CALL || !is_command_shell_event(&node.event) {
        return None;
    }

    node.children
        .iter()
        .enumerate()
        .filter_map(|(index, item)| match item {
            TimelineItem::Event(child)
                if child.event.event_type == SHELL_RESULT
                    && is_command_shell_event(&child.event)
                    && shell_operation_ids_match(&node.event, &child.event) =>
            {
                Some((index, shell_result_preference(&child.event)))
            }
            TimelineItem::Event(_) | TimelineItem::Thread(_) => None,
        })
        .max_by_key(|(_, score)| *score)
        .map(|(index, _)| index)
}

fn merged_shell_operation_children(
    node: &EventNode,
    shell_result_index: usize,
) -> Vec<TimelineItem> {
    let preferred_shell_result = match &node.children[shell_result_index] {
        TimelineItem::Event(child) => &child.event,
        TimelineItem::Thread(_) => unreachable!("shell result child must be an event"),
    };
    let mut children = Vec::new();
    for (index, item) in node.children.iter().enumerate() {
        if index == shell_result_index {
            if let TimelineItem::Event(child) = item {
                children.extend(child.children.clone());
            }
            continue;
        }
        if is_redundant_response_item_shell_result(item, &node.event, preferred_shell_result) {
            continue;
        }
        children.push(item.clone());
    }
    children
}

fn shell_result_preference(event: &EventEntry) -> (u8, u8, u8, u8, u64) {
    (
        u8::from(event.duplicate_of.as_deref() == Some(RESPONSE_ITEM_FUNCTION_CALL_OUTPUT)),
        u8::from(event.shell_command.is_some()),
        u8::from(event.aggregated_output.is_some()),
        u8::from(event.shell_exit_code.is_some()),
        event.seq,
    )
}

fn paired_spawn_agent_result_child_index(node: &EventNode) -> Option<usize> {
    if node.event.event_type != COLLAB_SPAWN_AGENT || node.event.phase.as_deref() != Some("started")
    {
        return None;
    }

    node.children
        .iter()
        .enumerate()
        .filter_map(|(index, item)| match item {
            TimelineItem::Event(child)
                if child.event.event_type == COLLAB_SPAWN_AGENT
                    && child.event.phase.as_deref() == Some("completed")
                    && shell_operation_ids_match(&node.event, &child.event) =>
            {
                Some((index, spawn_agent_result_preference(&child.event)))
            }
            TimelineItem::Event(_) | TimelineItem::Thread(_) => None,
        })
        .max_by_key(|(_, score)| *score)
        .map(|(index, _)| index)
}

fn merged_spawn_agent_operation_children(
    node: &EventNode,
    spawn_result_index: usize,
) -> Vec<TimelineItem> {
    let preferred_spawn_result = match &node.children[spawn_result_index] {
        TimelineItem::Event(child) => &child.event,
        TimelineItem::Thread(_) => unreachable!("spawn result child must be an event"),
    };
    let mut children = Vec::new();
    for (index, item) in node.children.iter().enumerate() {
        if index == spawn_result_index {
            if let TimelineItem::Event(child) = item {
                children.extend(child.children.clone());
            }
            continue;
        }
        if is_redundant_response_item_spawn_result(item, &node.event, preferred_spawn_result) {
            continue;
        }
        children.push(item.clone());
    }
    children
}

fn spawn_agent_result_preference(event: &EventEntry) -> (u8, u8, u8, u8, u8, u8, u8, u64) {
    let spawn = event.spawn_agent.as_ref();
    (
        u8::from(event.duplicate_of.as_deref() == Some(RESPONSE_ITEM_FUNCTION_CALL_OUTPUT)),
        u8::from(
            spawn
                .and_then(|entry| entry.receiver_thread_id.as_deref())
                .is_some(),
        ),
        u8::from(
            spawn
                .and_then(|entry| entry.receiver_nickname.as_deref())
                .is_some(),
        ),
        u8::from(
            spawn
                .and_then(|entry| entry.receiver_role.as_deref())
                .is_some(),
        ),
        u8::from(spawn.and_then(|entry| entry.model.as_deref()).is_some()),
        u8::from(
            spawn
                .and_then(|entry| entry.reasoning_effort.as_deref())
                .is_some(),
        ),
        u8::from(
            spawn
                .and_then(|entry| entry.receiver_status.as_deref())
                .is_some(),
        ),
        event.seq,
    )
}

fn paired_user_input_request_result_child_index(node: &EventNode) -> Option<usize> {
    if node.event.event_type != USER_INPUT_REQUEST || node.event.phase.as_deref() != Some("started")
    {
        return None;
    }

    node.children
        .iter()
        .enumerate()
        .filter_map(|(index, item)| match item {
            TimelineItem::Event(child)
                if child.event.event_type == USER_INPUT_REQUEST
                    && child.event.phase.as_deref() == Some("completed")
                    && shell_operation_ids_match(&node.event, &child.event) =>
            {
                Some((index, user_input_request_result_preference(&child.event)))
            }
            TimelineItem::Event(_) | TimelineItem::Thread(_) => None,
        })
        .max_by_key(|(_, score)| *score)
        .map(|(index, _)| index)
}

fn merged_user_input_request_operation_children(
    node: &EventNode,
    user_input_result_index: usize,
) -> Vec<TimelineItem> {
    let preferred_result = match &node.children[user_input_result_index] {
        TimelineItem::Event(child) => &child.event,
        TimelineItem::Thread(_) => unreachable!("user input result child must be an event"),
    };
    let mut children = Vec::new();
    for (index, item) in node.children.iter().enumerate() {
        if index == user_input_result_index {
            if let TimelineItem::Event(child) = item {
                children.extend(child.children.clone());
            }
            continue;
        }
        if is_redundant_response_item_user_input_request_result(item, &node.event, preferred_result)
        {
            continue;
        }
        children.push(item.clone());
    }
    children
}

fn user_input_request_result_preference(event: &EventEntry) -> (u8, usize, usize, u64) {
    let request = event.user_input_request.as_ref();
    (
        u8::from(event.duplicate_of.as_deref() == Some(RESPONSE_ITEM_FUNCTION_CALL_OUTPUT)),
        request
            .map(total_user_input_request_answer_count)
            .unwrap_or_default(),
        request
            .map(|entry| entry.questions.len())
            .unwrap_or_default(),
        event.seq,
    )
}

fn is_redundant_response_item_user_input_request_result(
    item: &TimelineItem,
    call: &EventEntry,
    preferred_result: &EventEntry,
) -> bool {
    let TimelineItem::Event(child) = item else {
        return false;
    };

    child.event.event_id != preferred_result.event_id
        && child.event.event_type == USER_INPUT_REQUEST
        && child.event.phase.as_deref() == Some("completed")
        && child.event.raw_type == "response_item"
        && child.event.duplicate_of.is_none()
        && shell_operation_ids_match(call, &child.event)
        && preferred_result.raw_type != "response_item"
}

fn paired_collab_operation_result_child_index(node: &EventNode) -> Option<usize> {
    if !is_pairable_collab_operation_event(&node.event)
        || node.event.phase.as_deref() != Some("started")
    {
        return None;
    }

    node.children
        .iter()
        .enumerate()
        .filter_map(|(index, item)| match item {
            TimelineItem::Event(child)
                if child.event.event_type == node.event.event_type
                    && child.event.phase.as_deref() == Some("completed")
                    && shell_operation_ids_match(&node.event, &child.event) =>
            {
                Some((index, collab_operation_result_preference(&child.event)))
            }
            TimelineItem::Event(_) | TimelineItem::Thread(_) => None,
        })
        .max_by_key(|(_, score)| *score)
        .map(|(index, _)| index)
}

fn merged_collab_operation_children(
    node: &EventNode,
    collab_result_index: usize,
) -> Vec<TimelineItem> {
    let preferred_result = match &node.children[collab_result_index] {
        TimelineItem::Event(child) => &child.event,
        TimelineItem::Thread(_) => unreachable!("collab result child must be an event"),
    };
    let mut children = Vec::new();
    for (index, item) in node.children.iter().enumerate() {
        if index == collab_result_index {
            if let TimelineItem::Event(child) = item {
                children.extend(child.children.clone());
            }
            continue;
        }
        if is_redundant_response_item_collab_result(item, &node.event, preferred_result) {
            continue;
        }
        children.push(item.clone());
    }
    children
}

fn collab_operation_result_preference(event: &EventEntry) -> (u8, usize, u64) {
    (
        u8::from(event.duplicate_of.as_deref() == Some(RESPONSE_ITEM_FUNCTION_CALL_OUTPUT)),
        collab_operation_state_count(event),
        event.seq,
    )
}

fn is_redundant_response_item_collab_result(
    item: &TimelineItem,
    call: &EventEntry,
    preferred_result: &EventEntry,
) -> bool {
    let TimelineItem::Event(child) = item else {
        return false;
    };

    child.event.event_id != preferred_result.event_id
        && is_pairable_collab_operation_event(&child.event)
        && child.event.phase.as_deref() == Some("completed")
        && child.event.raw_type == "response_item"
        && child.event.duplicate_of.is_none()
        && shell_operation_ids_match(call, &child.event)
        && preferred_result.raw_type != "response_item"
}

fn is_pairable_collab_operation_event(event: &EventEntry) -> bool {
    matches!(
        event.event_type.as_str(),
        COLLAB_SEND_INPUT | COLLAB_WAIT | COLLAB_CLOSE_AGENT | COLLAB_RESUME_AGENT
    )
}

fn is_redundant_response_item_spawn_result(
    item: &TimelineItem,
    call: &EventEntry,
    preferred_spawn_result: &EventEntry,
) -> bool {
    let TimelineItem::Event(child) = item else {
        return false;
    };

    child.event.event_id != preferred_spawn_result.event_id
        && child.event.event_type == COLLAB_SPAWN_AGENT
        && child.event.phase.as_deref() == Some("completed")
        && child.event.raw_type == "response_item"
        && child.event.duplicate_of.is_none()
        && shell_operation_ids_match(call, &child.event)
        && preferred_spawn_result.raw_type != "response_item"
}

fn is_redundant_response_item_shell_result(
    item: &TimelineItem,
    call: &EventEntry,
    preferred_shell_result: &EventEntry,
) -> bool {
    let TimelineItem::Event(child) = item else {
        return false;
    };

    child.event.event_id != preferred_shell_result.event_id
        && child.event.event_type == SHELL_RESULT
        && child.event.raw_type == "response_item"
        && child.event.duplicate_of.is_none()
        && is_command_shell_event(&child.event)
        && shell_operation_ids_match(call, &child.event)
        && preferred_shell_result.raw_type != "response_item"
}

const TEXT_COLLAPSE_CHAR_LIMIT: usize = 240;
const TEXT_COLLAPSE_LINE_LIMIT: usize = 4;
const RESPONSE_ITEM_FUNCTION_CALL_OUTPUT: &str = "response_item.function_call_output";

fn render_hero_card(label: &str, value_html: &str) -> String {
    format!(
        "<div class=\"hero-card\"><span class=\"hero-card-label\">{}</span><div class=\"hero-card-value\">{}</div></div>",
        escape_html(label),
        value_html,
    )
}

fn render_hero_overview_card(event_count: usize, thread_count: usize) -> String {
    format!(
        "<div class=\"hero-card\"><span class=\"hero-card-label\">overview</span><div class=\"hero-metrics\">\
         <div class=\"hero-metric\"><span class=\"hero-metric-label\">events</span><span class=\"hero-metric-value\">{}</span></div>\
         <div class=\"hero-metric\"><span class=\"hero-metric-label\">threads</span><span class=\"hero-metric-value\">{}</span></div>\
         </div></div>",
        event_count,
        thread_count,
    )
}

fn render_startup_metadata_cards(startup: &serde_json::Map<String, serde_json::Value>) -> String {
    let mut out = String::new();
    for (key, value) in startup {
        if key == "base_instructions" {
            continue;
        }
        out.push_str(&render_startup_metadata_card(key, value));
    }
    out
}

fn render_startup_metadata_card(key: &str, value: &serde_json::Value) -> String {
    format!(
        "<div class=\"hero-startup-card\"><span class=\"hero-card-label\">{}</span>{}</div>",
        escape_html(key),
        render_startup_metadata_body(value),
    )
}

fn render_startup_metadata_body(value: &serde_json::Value) -> String {
    if let Some(entries) = startup_metadata_entries(value) {
        let mut out = String::from("<div class=\"hero-startup-rows\">");
        for (entry_key, entry_value) in entries {
            let _ = write!(
                out,
                "<div class=\"hero-startup-row\"><span class=\"hero-startup-row-label\">{}</span><div class=\"hero-startup-row-value\"><code>{}</code></div></div>",
                escape_html(&entry_key),
                escape_html(&entry_value),
            );
        }
        out.push_str("</div>");
        return out;
    }

    let rendered_value = render_startup_metadata_value(value);
    format!(
        "<div class=\"hero-startup-value\"><code>{}</code></div>",
        escape_html(&rendered_value),
    )
}

fn startup_metadata_entries(value: &serde_json::Value) -> Option<Vec<(String, String)>> {
    match value {
        serde_json::Value::Object(object) => Some(
            object
                .iter()
                .map(|(entry_key, entry_value)| {
                    (
                        entry_key.clone(),
                        render_startup_metadata_value(entry_value),
                    )
                })
                .collect(),
        ),
        serde_json::Value::String(text) => serde_json::from_str::<serde_json::Value>(text)
            .ok()
            .and_then(|parsed| startup_metadata_entries(&parsed)),
        _ => None,
    }
}

fn render_base_instructions_panel(
    startup: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    let value = startup.get("base_instructions")?;
    let rendered_value = render_startup_metadata_text(value);
    let content = if should_collapse_text_content(&rendered_value) {
        render_collapsible_text_block(&rendered_value)
    } else {
        format!(
            "<div class=\"summary-text\">{}</div>",
            escape_html(&rendered_value)
        )
    };
    Some(render_inset_block(
        "base_instructions",
        &content,
        None,
        "meta-inset-block",
        true,
    ))
}

fn render_startup_metadata_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Bool(_) | serde_json::Value::Number(_) | serde_json::Value::Null => {
            value.to_string()
        }
        _ => serde_json::to_string(value).unwrap_or_else(|_| "<invalid-json>".to_string()),
    }
}

fn render_startup_metadata_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Object(object) => object
            .get("text")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| {
                serde_json::to_string_pretty(value).unwrap_or_else(|_| "<invalid-json>".to_string())
            }),
        _ => serde_json::to_string_pretty(value).unwrap_or_else(|_| "<invalid-json>".to_string()),
    }
}

fn render_inset_block(
    title: &str,
    body_html: &str,
    footer_html: Option<&str>,
    class_name: &str,
    inline_title: bool,
) -> String {
    let footer = footer_html
        .map(|content| format!("<div class=\"inset-block-footer\">{content}</div>"))
        .unwrap_or_default();
    let block_class = if inline_title {
        format!("inset-block {class_name} inline-title")
    } else {
        format!("inset-block {class_name}")
    };
    let title_class = if inline_title {
        "inset-block-title inline-title"
    } else {
        "inset-block-title"
    };
    format!(
        "<div class=\"{}\"><span class=\"{}\">{}</span><div class=\"inset-block-body\">{}{}</div></div>",
        block_class,
        title_class,
        escape_html(title),
        body_html,
        footer,
    )
}

fn render_summary_block(event: &EventEntry) -> String {
    if event.summary_pairs.is_empty() {
        if should_collapse_summary(event) {
            return render_collapsible_text_block(&event.summary);
        }
        return format!(
            "<div class=\"summary-text\">{}</div>",
            escape_html(&event.summary)
        );
    }

    let mut out = String::from(
        "<div class=\"summary-structured\"><span class=\"summary-title\">tokens</span>",
    );
    for (label, value) in &event.summary_pairs {
        let _ = write!(
            out,
            "<span class=\"summary-pair\"><span class=\"summary-label\">{}</span><span class=\"summary-value\">{}</span></span>",
            escape_html(label),
            escape_html(value)
        );
    }
    out.push_str("</div>");
    out
}

fn should_collapse_summary(event: &EventEntry) -> bool {
    matches_collapse_event_type(event) && should_collapse_text_content(&event.summary)
}

fn matches_collapse_event_type(event: &EventEntry) -> bool {
    event.event_type.starts_with("message.") || event.event_type == SHELL_RESULT
}

fn should_collapse_text_content(text: &str) -> bool {
    text.chars().count() > TEXT_COLLAPSE_CHAR_LIMIT
        || text.lines().count() > TEXT_COLLAPSE_LINE_LIMIT
}

fn render_collapsible_text_block(summary: &str) -> String {
    let preview = truncate_message_preview(summary, TEXT_COLLAPSE_CHAR_LIMIT);
    format!(
        "<div class=\"message-collapse\" data-expanded=\"false\">\
         <div class=\"summary-text message-preview\">{}</div>\
         <div class=\"summary-text message-full\">{}</div>\
         <button class=\"message-toggle\" type=\"button\" aria-expanded=\"false\" onclick=\"toggleMessageBlock(this)\">see full</button>\
         </div>",
        escape_html(&preview),
        escape_html(summary),
    )
}

fn truncate_message_preview(summary: &str, limit: usize) -> String {
    let char_count = summary.chars().count();
    if char_count <= limit {
        return summary.to_string();
    }

    let truncated = summary.chars().take(limit).collect::<String>();
    let trimmed = truncated.trim_end_matches(char::is_whitespace);
    format!("{trimmed}...")
}

fn render_event_card(out: &mut String, event: &EventEntry, last_token_usage: &mut TokenUsage) {
    let token_footnote = render_token_footnote(event, last_token_usage);
    if event.event_type == INFO_TOKENS {
        out.push_str(&token_footnote);
        return;
    }

    let subagent_badge = render_subagent_badge(event);
    let meta_row = render_event_meta_row(event);
    let summary_block = render_event_summary_block(event);
    let detail_block = render_event_detail_block(event);
    let card_class = event_card_class(event);
    let _ = write!(
        out,
        "<article class=\"{}\" data-seq=\"{}\" data-event-id=\"{}\" data-parent-event-id=\"{}\" data-raw-type=\"{}\" data-parse-status=\"{}\">\
         <div class=\"event-header\">\
         <div class=\"event-header-main\">\
         <span class=\"seq-chip\">#{:04}</span>\
         <span class=\"badge {}\">{}</span>{}\
         </div>\
         <span class=\"event-ts\">{}</span>\
         </div>\
         {}{}{}\
         </article>{}",
        card_class,
        event.seq,
        escape_html(&event.event_id),
        escape_html(event.parent_event_id.as_deref().unwrap_or("")),
        escape_html(&event.raw_type),
        escape_html(&event.parse_status),
        event.seq,
        category_class(event.category),
        escape_html(&event.event_type),
        subagent_badge,
        escape_html(&event.ts),
        meta_row,
        summary_block,
        detail_block,
        token_footnote,
    );
}

fn event_card_class(event: &EventEntry) -> &'static str {
    match (
        event.event_type == MESSAGE_USER,
        is_task_started_event(event),
        is_task_completed_event(event),
    ) {
        (true, false, false) => "event-card is-user-prompt",
        (false, true, false) => "event-card is-task-started",
        (false, false, true) => "event-card is-task-completed",
        (true, true, false) => "event-card is-user-prompt is-task-started",
        (true, false, true) => "event-card is-user-prompt is-task-completed",
        _ => "event-card",
    }
}

fn render_combined_shell_operation_card(out: &mut String, call: &EventEntry, result: &EventEntry) {
    let subagent_badge = {
        let badge = render_subagent_badge(call);
        if badge.is_empty() {
            render_subagent_badge(result)
        } else {
            badge
        }
    };
    let meta_row = render_combined_shell_meta_row(call, result);
    let detail_block = render_combined_shell_operation_detail_block(call, result);
    let seq_label = format!("#{:04}, #{:04}", call.seq, result.seq);
    let event_label = format!("{}, {}", call.event_type, result.event_type);
    let timestamp_label = if call.ts == result.ts {
        call.ts.clone()
    } else {
        format!("{} -> {}", call.ts, result.ts)
    };
    let event_ids = format!("{},{}", call.event_id, result.event_id);
    let raw_types = format!("{},{}", call.raw_type, result.raw_type);
    let parse_statuses = format!("{},{}", call.parse_status, result.parse_status);
    let _ = write!(
        out,
        "<article class=\"event-card\" data-seq=\"{},{}\" data-event-id=\"{}\" data-event-ids=\"{}\" data-parent-event-id=\"{}\" data-raw-type=\"{}\" data-parse-status=\"{}\">\
         <div class=\"event-header\">\
         <div class=\"event-header-main\">\
         <span class=\"seq-chip\">{}</span>\
         <span class=\"badge {}\">{}</span>{}\
         </div>\
         <span class=\"event-ts\">{}</span>\
         </div>\
         {}{}\
         </article>",
        call.seq,
        result.seq,
        escape_html(&call.event_id),
        escape_html(&event_ids),
        escape_html(call.parent_event_id.as_deref().unwrap_or("")),
        escape_html(&raw_types),
        escape_html(&parse_statuses),
        escape_html(&seq_label),
        category_class(call.category),
        escape_html(&event_label),
        subagent_badge,
        escape_html(&timestamp_label),
        meta_row,
        detail_block,
    );
}

fn render_combined_spawn_agent_operation_card(
    out: &mut String,
    call: &EventEntry,
    result: &EventEntry,
) {
    let subagent_badge = {
        let badge = render_subagent_badge(call);
        if badge.is_empty() {
            render_subagent_badge(result)
        } else {
            badge
        }
    };
    let meta_row = render_combined_spawn_agent_meta_row(call, result);
    let detail_block = render_combined_spawn_agent_operation_detail_block(call, result);
    let seq_label = format!("#{:04}, #{:04}", call.seq, result.seq);
    let event_label = call.event_type.clone();
    let timestamp_label = if call.ts == result.ts {
        call.ts.clone()
    } else {
        format!("{} -> {}", call.ts, result.ts)
    };
    let event_ids = format!("{},{}", call.event_id, result.event_id);
    let raw_types = format!("{},{}", call.raw_type, result.raw_type);
    let parse_statuses = format!("{},{}", call.parse_status, result.parse_status);
    let _ = write!(
        out,
        "<article class=\"event-card\" data-seq=\"{},{}\" data-event-id=\"{}\" data-event-ids=\"{}\" data-parent-event-id=\"{}\" data-raw-type=\"{}\" data-parse-status=\"{}\">\
         <div class=\"event-header\">\
         <div class=\"event-header-main\">\
         <span class=\"seq-chip\">{}</span>\
         <span class=\"badge {}\">{}</span>{}\
         </div>\
         <span class=\"event-ts\">{}</span>\
         </div>\
         {}{}\
         </article>",
        call.seq,
        result.seq,
        escape_html(&call.event_id),
        escape_html(&event_ids),
        escape_html(call.parent_event_id.as_deref().unwrap_or("")),
        escape_html(&raw_types),
        escape_html(&parse_statuses),
        escape_html(&seq_label),
        category_class(call.category),
        escape_html(&event_label),
        subagent_badge,
        escape_html(&timestamp_label),
        meta_row,
        detail_block,
    );
}

fn render_combined_user_input_request_operation_card(
    out: &mut String,
    call: &EventEntry,
    result: &EventEntry,
) {
    let subagent_badge = {
        let badge = render_subagent_badge(call);
        if badge.is_empty() {
            render_subagent_badge(result)
        } else {
            badge
        }
    };
    let meta_row = render_combined_user_input_request_meta_row(call, result);
    let detail_block = render_combined_user_input_request_operation_detail_block(call, result);
    let seq_label = format!("#{:04}, #{:04}", call.seq, result.seq);
    let event_label = call.event_type.clone();
    let timestamp_label = if call.ts == result.ts {
        call.ts.clone()
    } else {
        format!("{} -> {}", call.ts, result.ts)
    };
    let event_ids = format!("{},{}", call.event_id, result.event_id);
    let raw_types = format!("{},{}", call.raw_type, result.raw_type);
    let parse_statuses = format!("{},{}", call.parse_status, result.parse_status);
    let _ = write!(
        out,
        "<article class=\"event-card\" data-seq=\"{},{}\" data-event-id=\"{}\" data-event-ids=\"{}\" data-parent-event-id=\"{}\" data-raw-type=\"{}\" data-parse-status=\"{}\">\
         <div class=\"event-header\">\
         <div class=\"event-header-main\">\
         <span class=\"seq-chip\">{}</span>\
         <span class=\"badge {}\">{}</span>{}\
         </div>\
         <span class=\"event-ts\">{}</span>\
         </div>\
         {}{}\
         </article>",
        call.seq,
        result.seq,
        escape_html(&call.event_id),
        escape_html(&event_ids),
        escape_html(call.parent_event_id.as_deref().unwrap_or("")),
        escape_html(&raw_types),
        escape_html(&parse_statuses),
        escape_html(&seq_label),
        category_class(call.category),
        escape_html(&event_label),
        subagent_badge,
        escape_html(&timestamp_label),
        meta_row,
        detail_block,
    );
}

fn render_combined_collab_operation_card(out: &mut String, call: &EventEntry, result: &EventEntry) {
    let subagent_badge = {
        let badge = render_subagent_badge(call);
        if badge.is_empty() {
            render_subagent_badge(result)
        } else {
            badge
        }
    };
    let meta_row = render_combined_collab_operation_meta_row(call, result);
    let summary_block = render_event_summary_block(result);
    let detail_block = render_combined_collab_operation_detail_block(call, result);
    let seq_label = format!("#{:04}, #{:04}", call.seq, result.seq);
    let event_label = call.event_type.clone();
    let timestamp_label = if call.ts == result.ts {
        call.ts.clone()
    } else {
        format!("{} -> {}", call.ts, result.ts)
    };
    let event_ids = format!("{},{}", call.event_id, result.event_id);
    let raw_types = format!("{},{}", call.raw_type, result.raw_type);
    let parse_statuses = format!("{},{}", call.parse_status, result.parse_status);
    let _ = write!(
        out,
        "<article class=\"event-card\" data-seq=\"{},{}\" data-event-id=\"{}\" data-event-ids=\"{}\" data-parent-event-id=\"{}\" data-raw-type=\"{}\" data-parse-status=\"{}\">\
         <div class=\"event-header\">\
         <div class=\"event-header-main\">\
         <span class=\"seq-chip\">{}</span>\
         <span class=\"badge {}\">{}</span>{}\
         </div>\
         <span class=\"event-ts\">{}</span>\
         </div>\
         {}{}{}\
         </article>",
        call.seq,
        result.seq,
        escape_html(&call.event_id),
        escape_html(&event_ids),
        escape_html(call.parent_event_id.as_deref().unwrap_or("")),
        escape_html(&raw_types),
        escape_html(&parse_statuses),
        escape_html(&seq_label),
        category_class(call.category),
        escape_html(&event_label),
        subagent_badge,
        escape_html(&timestamp_label),
        meta_row,
        summary_block,
        detail_block,
    );
}

fn render_combined_shell_operation_detail_block(call: &EventEntry, result: &EventEntry) -> String {
    render_shell_operation_block(
        call.shell_command
            .as_deref()
            .or(result.shell_command.as_deref()),
        result.aggregated_output.as_deref(),
        result.shell_exit_code,
    )
    .map(|shell_block| format!("<div class=\"event-detail\">{shell_block}</div>"))
    .unwrap_or_default()
}

fn render_combined_spawn_agent_operation_detail_block(
    call: &EventEntry,
    result: &EventEntry,
) -> String {
    let Some(data) = merged_spawn_agent_render_data(call, result) else {
        return String::new();
    };
    render_spawn_agent_operation_block(&data)
        .map(|block| format!("<div class=\"event-detail\">{block}</div>"))
        .unwrap_or_default()
}

fn render_combined_user_input_request_operation_detail_block(
    call: &EventEntry,
    result: &EventEntry,
) -> String {
    merged_user_input_request_entry(call, result)
        .and_then(|entry| render_user_input_request_operation_block(&entry))
        .map(|block| format!("<div class=\"event-detail\">{block}</div>"))
        .unwrap_or_default()
}

fn render_combined_collab_operation_detail_block(
    _call: &EventEntry,
    result: &EventEntry,
) -> String {
    render_collab_operation_block(result)
        .map(|block| format!("<div class=\"event-detail\">{block}</div>"))
        .unwrap_or_default()
}

fn render_event_summary_block(event: &EventEntry) -> String {
    if event.event_type == SHELL_RESULT || should_skip_event_summary(event) {
        return String::new();
    }

    format!(
        "<div class=\"event-summary\">{}</div>",
        render_summary_block(event)
    )
}

fn render_event_detail_block(event: &EventEntry) -> String {
    if let Some(runtime_context_block) = render_runtime_context_block(event) {
        return format!("<div class=\"event-detail\">{runtime_context_block}</div>");
    }

    if let Some(task_message_block) = render_task_message_block(event) {
        return format!("<div class=\"event-detail\">{task_message_block}</div>");
    }

    if let Some(shell_block) = render_shell_result_block(event) {
        return format!("<div class=\"event-detail\">{shell_block}</div>");
    }
    if let Some(spawn_agent_block) = render_spawn_agent_block(event) {
        return format!("<div class=\"event-detail\">{spawn_agent_block}</div>");
    }
    if let Some(user_input_request_block) = render_user_input_request_block(event) {
        return format!("<div class=\"event-detail\">{user_input_request_block}</div>");
    }
    if let Some(collab_operation_block) = render_collab_operation_block(event) {
        return format!("<div class=\"event-detail\">{collab_operation_block}</div>");
    }

    if let Some(plan_block) = render_plan_update_block(event) {
        return format!("<div class=\"event-detail\">{plan_block}</div>");
    }

    event.aggregated_output
        .as_deref()
        .filter(|output| !output.is_empty())
        .map(|output| {
            let content = if should_collapse_text_content(output) {
                render_collapsible_text_block(output)
            } else {
                format!("<div class=\"summary-text\">{}</div>", escape_html(output))
            };
            format!(
                "<div class=\"event-detail\"><span class=\"event-section-label\">output</span>{}</div>",
                content
            )
        })
        .unwrap_or_default()
}

fn render_task_message_block(event: &EventEntry) -> Option<String> {
    if !is_task_completed_event(event) {
        return None;
    }

    let message = event
        .last_agent_message
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let body = if should_collapse_text_content(message) {
        render_collapsible_text_block(message)
    } else {
        format!("<div class=\"summary-text\">{}</div>", escape_html(message))
    };
    Some(render_inset_block(
        "Last Agent Message",
        &body,
        None,
        "task-message-block",
        true,
    ))
}

fn render_plan_update_block(event: &EventEntry) -> Option<String> {
    if event.event_type != "plan.update"
        || (event.plan_explanation.is_none() && event.plan_steps.is_empty())
    {
        return None;
    }

    let mut body = String::new();
    if let Some(explanation) = event
        .plan_explanation
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let explanation_body = if should_collapse_text_content(explanation) {
            render_collapsible_text_block(explanation)
        } else {
            format!(
                "<div class=\"summary-text plan-explanation\">{}</div>",
                escape_html(explanation)
            )
        };
        let _ = write!(
            body,
            "<div><span class=\"event-section-label\">explanation</span>{}</div>",
            explanation_body,
        );
    }
    if !event.plan_steps.is_empty() {
        body.push_str(
            "<div><span class=\"event-section-label\">steps</span><div class=\"plan-steps\">",
        );
        for (index, step) in event.plan_steps.iter().enumerate() {
            let status_badge = step
                .status
                .as_deref()
                .filter(|value| !value.is_empty())
                .map(render_plan_step_status)
                .unwrap_or_default();
            let _ = write!(
                body,
                "<div class=\"plan-step\"><span class=\"plan-step-index\">{:02}</span><span class=\"plan-step-text\">{}</span>{}</div>",
                index + 1,
                escape_html(&step.step),
                status_badge,
            );
        }
        body.push_str("</div></div>");
    }

    Some(render_inset_block("Plan", &body, None, "plan-block", true))
}

fn render_user_input_request_block(event: &EventEntry) -> Option<String> {
    let request = event.user_input_request.as_ref()?;
    render_user_input_request_operation_block(request)
}

fn render_user_input_request_operation_block(request: &UserInputRequestEntry) -> Option<String> {
    if request.questions.is_empty() && request.extra_answers.is_empty() {
        return None;
    }

    let mut body = String::new();
    if !request.questions.is_empty() {
        body.push_str(
            "<div><span class=\"event-section-label\">questions</span><div class=\"user-input-questions\">",
        );
        for question in &request.questions {
            body.push_str(&render_user_input_question_block(question));
        }
        body.push_str("</div></div>");
    }
    if !request.extra_answers.is_empty() {
        body.push_str(
            "<div><span class=\"event-section-label\">answers</span><div class=\"user-input-extra-answers\">",
        );
        for answer in &request.extra_answers {
            let _ = write!(
                body,
                "<div class=\"user-input-extra-answer\"><span class=\"user-input-extra-answer-id\">{}</span>{}</div>",
                escape_html(&answer.id),
                render_user_input_answer_chips(&answer.answers, false),
            );
        }
        body.push_str("</div></div>");
    }

    Some(render_inset_block(
        "User Input",
        &body,
        None,
        "user-input-block",
        true,
    ))
}

fn render_user_input_question_block(question: &UserInputQuestionEntry) -> String {
    let mut out = String::from("<div class=\"user-input-question\">");
    let mut head = String::new();
    if let Some(header) = question.header.as_deref().filter(|value| !value.is_empty()) {
        let _ = write!(
            head,
            "<span class=\"user-input-question-tag\"><span class=\"user-input-question-tag-label\">header</span>{}</span>",
            escape_html(header),
        );
    }
    if let Some(id) = question.id.as_deref().filter(|value| !value.is_empty()) {
        let _ = write!(
            head,
            "<span class=\"user-input-question-tag\"><span class=\"user-input-question-tag-label\">id</span><code>{}</code></span>",
            escape_html(id),
        );
    }
    if !head.is_empty() {
        let _ = write!(out, "<div class=\"user-input-question-head\">{head}</div>");
    }
    if let Some(prompt) = question
        .question
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let prompt_body = if should_collapse_text_content(prompt) {
            render_collapsible_text_block(prompt)
        } else {
            format!(
                "<div class=\"summary-text user-input-question-text\">{}</div>",
                escape_html(prompt)
            )
        };
        out.push_str(&prompt_body);
    }
    if !question.options.is_empty() {
        out.push_str("<div class=\"user-input-options\">");
        for option in &question.options {
            let is_selected = question
                .answers
                .iter()
                .any(|answer| answer.trim() == option.label.trim());
            out.push_str(&render_user_input_option_block(option, is_selected));
        }
        out.push_str("</div>");
    }

    let unmatched_answers = question
        .answers
        .iter()
        .filter(|answer| {
            !question
                .options
                .iter()
                .any(|option| option.label.trim() == answer.trim())
        })
        .cloned()
        .collect::<Vec<_>>();
    if !unmatched_answers.is_empty() {
        let _ = write!(
            out,
            "<div><span class=\"event-section-label\">answers</span>{}</div>",
            render_user_input_answer_chips(&unmatched_answers, true),
        );
    }
    out.push_str("</div>");
    out
}

fn render_user_input_option_block(option: &UserInputOptionEntry, is_selected: bool) -> String {
    let selected_chip = if is_selected {
        "<span class=\"user-input-answer-chip is-selected\">selected</span>"
    } else {
        ""
    };
    let description = option
        .description
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|description| {
            if should_collapse_text_content(description) {
                render_collapsible_text_block(description)
            } else {
                format!(
                    "<div class=\"user-input-option-description\">{}</div>",
                    escape_html(description)
                )
            }
        })
        .unwrap_or_default();
    format!(
        "<div class=\"user-input-option{}\"><div class=\"user-input-option-head\"><span class=\"user-input-option-label\">{}</span>{}</div>{}</div>",
        if is_selected { " is-selected" } else { "" },
        escape_html(&option.label),
        selected_chip,
        description,
    )
}

fn render_user_input_answer_chips(answers: &[String], selected: bool) -> String {
    let mut out = String::from("<div class=\"user-input-answer-list\">");
    for answer in answers.iter().filter(|answer| !answer.trim().is_empty()) {
        let _ = write!(
            out,
            "<span class=\"user-input-answer-chip{}\">{}</span>",
            if selected { " is-selected" } else { "" },
            escape_html(answer),
        );
    }
    out.push_str("</div>");
    out
}

fn merged_user_input_request_entry(
    call: &EventEntry,
    result: &EventEntry,
) -> Option<UserInputRequestEntry> {
    let call_request = call.user_input_request.as_ref();
    let result_request = result.user_input_request.as_ref();
    let Some(base_request) = call_request.or(result_request) else {
        return None;
    };

    let mut answers_by_id = BTreeMap::new();
    if let Some(request) = call_request {
        collect_user_input_answers(request, &mut answers_by_id);
    }
    if let Some(request) = result_request {
        collect_user_input_answers(request, &mut answers_by_id);
    }

    let mut matched_answer_ids = Vec::new();
    let mut questions = if !base_request.questions.is_empty() {
        base_request.questions.clone()
    } else {
        result_request
            .map(|request| request.questions.clone())
            .unwrap_or_default()
    };
    for question in &mut questions {
        if let Some(id) = question.id.as_ref() {
            if let Some(answers) = answers_by_id.get(id) {
                question.answers = answers.clone();
                matched_answer_ids.push(id.clone());
            }
        }
    }

    let extra_answers = answers_by_id
        .into_iter()
        .filter(|(id, answers)| {
            !matched_answer_ids.iter().any(|matched| matched == id) && !answers.is_empty()
        })
        .map(|(id, answers)| UserInputAnswerEntry { id, answers })
        .collect::<Vec<_>>();

    Some(UserInputRequestEntry {
        questions,
        extra_answers,
    })
}

fn collect_user_input_answers(
    request: &UserInputRequestEntry,
    answers_by_id: &mut BTreeMap<String, Vec<String>>,
) {
    for question in &request.questions {
        let Some(id) = question.id.as_ref() else {
            continue;
        };
        merge_user_input_answer_values(
            answers_by_id.entry(id.clone()).or_default(),
            &question.answers,
        );
    }
    for answer in &request.extra_answers {
        merge_user_input_answer_values(
            answers_by_id.entry(answer.id.clone()).or_default(),
            &answer.answers,
        );
    }
}

fn merge_user_input_answer_values(target: &mut Vec<String>, source: &[String]) {
    for answer in source {
        if answer.trim().is_empty() || target.iter().any(|known| known == answer) {
            continue;
        }
        target.push(answer.clone());
    }
}

fn total_user_input_request_answer_count(request: &UserInputRequestEntry) -> usize {
    request
        .questions
        .iter()
        .map(|question| question.answers.len())
        .sum::<usize>()
        + request
            .extra_answers
            .iter()
            .map(|answer| answer.answers.len())
            .sum::<usize>()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CollabOperationStateEntry {
    thread_id: Option<String>,
    label: Option<String>,
    text: String,
}

fn render_collab_operation_block(event: &EventEntry) -> Option<String> {
    if !is_pairable_collab_operation_event(event) {
        return None;
    }

    let states = collab_operation_states(event);
    if states.is_empty() {
        return None;
    }

    let mut body = String::new();
    for state in states {
        let mut header_parts = Vec::new();
        if let Some(thread_id) = state.thread_id.as_deref().filter(|value| !value.is_empty()) {
            header_parts.push(format!(
                "<span class=\"user-input-question-tag\"><span class=\"user-input-question-tag-label\">thread</span><code>{}</code></span>",
                escape_html(thread_id),
            ));
        }
        if let Some(label) = state.label.as_deref().filter(|value| !value.is_empty()) {
            header_parts.push(format!(
                "<span class=\"user-input-question-tag\"><span class=\"user-input-question-tag-label\">status</span>{}</span>",
                escape_html(label),
            ));
        }
        let content = if should_collapse_text_content(&state.text) {
            render_collapsible_text_block(&state.text)
        } else {
            format!(
                "<div class=\"summary-text\">{}</div>",
                escape_html(&state.text)
            )
        };
        let header_html = if header_parts.is_empty() {
            String::new()
        } else {
            format!(
                "<div class=\"user-input-question-head\">{}</div>",
                header_parts.join("")
            )
        };
        let _ = write!(
            body,
            "<div class=\"user-input-question\">{}{}</div>",
            header_html, content,
        );
    }

    Some(render_inset_block(
        collab_operation_block_title(event),
        &body,
        None,
        "user-input-block",
        true,
    ))
}

fn collab_operation_block_title(event: &EventEntry) -> &'static str {
    match event.event_type.as_str() {
        COLLAB_SEND_INPUT => "Subagent Input",
        COLLAB_WAIT => "Subagent Wait",
        COLLAB_CLOSE_AGENT => "Subagent Close",
        COLLAB_RESUME_AGENT => "Subagent Resume",
        _ => "Subagent Result",
    }
}

fn collab_operation_state_count(event: &EventEntry) -> usize {
    collab_operation_states(event).len()
}

fn collab_operation_states(event: &EventEntry) -> Vec<CollabOperationStateEntry> {
    let Some(output) = event.output_value.as_ref() else {
        return Vec::new();
    };

    let fallback_thread_id = event.receiver_thread_ids.first().map(String::as_str);
    let mut states = Vec::new();

    if let Some(output_obj) = output.as_object() {
        if let Some(statuses) = output_obj
            .get("statuses")
            .and_then(serde_json::Value::as_object)
        {
            for (thread_id, value) in statuses {
                collect_collab_operation_state_entries(
                    &mut states,
                    Some(thread_id.as_str()),
                    None,
                    value,
                );
            }
        }
        if let Some(agent_statuses) = output_obj
            .get("agent_statuses")
            .and_then(serde_json::Value::as_array)
        {
            for entry in agent_statuses {
                let Some(entry_obj) = entry.as_object() else {
                    continue;
                };
                let thread_id = entry_obj
                    .get("thread_id")
                    .and_then(serde_json::Value::as_str);
                if let Some(status) = entry_obj.get("status") {
                    collect_collab_operation_state_entries(&mut states, thread_id, None, status);
                }
                if let Some(message) = entry_obj.get("message") {
                    collect_collab_operation_state_entries(
                        &mut states,
                        thread_id,
                        Some("message"),
                        message,
                    );
                }
            }
        }
        if let Some(status) = output_obj.get("status") {
            collect_collab_operation_state_entries(&mut states, fallback_thread_id, None, status);
        }
        if let Some(previous_status) = output_obj.get("previous_status") {
            collect_collab_operation_state_entries(
                &mut states,
                fallback_thread_id,
                Some("previous status"),
                previous_status,
            );
        }
        if let Some(agents_states) = output_obj
            .get("agents_states")
            .and_then(serde_json::Value::as_object)
        {
            for (thread_id, value) in agents_states {
                collect_collab_operation_state_entries(
                    &mut states,
                    Some(thread_id.as_str()),
                    None,
                    value,
                );
            }
        }
    }

    if states.is_empty() {
        collect_collab_operation_state_entries(&mut states, fallback_thread_id, None, output);
    }

    let mut deduped = Vec::new();
    for state in states {
        if deduped
            .iter()
            .any(|known: &CollabOperationStateEntry| known == &state)
        {
            continue;
        }
        deduped.push(state);
    }
    deduped
}

fn collect_collab_operation_state_entries(
    out: &mut Vec<CollabOperationStateEntry>,
    thread_id: Option<&str>,
    label: Option<&str>,
    value: &serde_json::Value,
) {
    match value {
        serde_json::Value::Null => {}
        serde_json::Value::String(text) => {
            let text = text.trim();
            if text.is_empty() {
                return;
            }
            out.push(CollabOperationStateEntry {
                thread_id: thread_id.map(str::to_string),
                label: label.map(str::to_string),
                text: text.to_string(),
            });
        }
        serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {
            out.push(CollabOperationStateEntry {
                thread_id: thread_id.map(str::to_string),
                label: label.map(str::to_string),
                text: value.to_string(),
            });
        }
        serde_json::Value::Object(object) => {
            for (key, nested) in object {
                if matches!(
                    key.as_str(),
                    "agent_nickname"
                        | "agent_role"
                        | "model"
                        | "reasoning_effort"
                        | "thread_id"
                        | "session_path"
                ) {
                    continue;
                }
                collect_collab_operation_state_entries(out, thread_id, Some(key.as_str()), nested);
            }
        }
        serde_json::Value::Array(values) => {
            for nested in values {
                collect_collab_operation_state_entries(out, thread_id, label, nested);
            }
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SpawnAgentRenderData {
    prompt: Option<String>,
    requested_agent_type: Option<String>,
    model: Option<String>,
    reasoning_effort: Option<String>,
    receiver_thread_id: Option<String>,
    receiver_nickname: Option<String>,
    receiver_role: Option<String>,
    receiver_status: Option<String>,
}

fn render_spawn_agent_block(event: &EventEntry) -> Option<String> {
    let data = spawn_agent_render_data(event)?;
    render_spawn_agent_operation_block(&data)
}

fn render_spawn_agent_operation_block(data: &SpawnAgentRenderData) -> Option<String> {
    if data == &SpawnAgentRenderData::default() {
        return None;
    }

    let mut body = String::new();
    if let Some(prompt) = data
        .prompt
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let prompt_body = if should_collapse_text_content(prompt) {
            render_collapsible_text_block(prompt)
        } else {
            format!("<div class=\"summary-text\">{}</div>", escape_html(prompt))
        };
        let _ = write!(
            body,
            "<div><span class=\"event-section-label\">prompt</span>{}</div>",
            prompt_body,
        );
    }

    let mut rows = Vec::new();
    for (label, value) in [
        ("thread id", data.receiver_thread_id.as_deref()),
        ("nickname", data.receiver_nickname.as_deref()),
        ("role", data.receiver_role.as_deref()),
        ("agent type", data.requested_agent_type.as_deref()),
        ("model", data.model.as_deref()),
        ("reasoning effort", data.reasoning_effort.as_deref()),
        ("status", data.receiver_status.as_deref()),
    ] {
        let Some(value) = value.filter(|value| !value.is_empty()) else {
            continue;
        };
        rows.push(format!(
            "<tr class=\"kv-table-row\"><th class=\"kv-table-label\">{}</th><td class=\"kv-table-value\"><code>{}</code></td></tr>",
            escape_html(label),
            escape_html(value),
        ));
    }
    if !rows.is_empty() {
        let _ = write!(
            body,
            "<div><span class=\"event-section-label\">agent</span><table class=\"kv-table\"><tbody>{}</tbody></table></div>",
            rows.join(""),
        );
    }

    Some(render_inset_block(
        "Spawn Agent",
        &body,
        None,
        "spawn-agent-block",
        true,
    ))
}

fn spawn_agent_render_data(event: &EventEntry) -> Option<SpawnAgentRenderData> {
    let spawn = event.spawn_agent.as_ref()?;
    Some(SpawnAgentRenderData {
        prompt: spawn.prompt.clone(),
        requested_agent_type: spawn.requested_agent_type.clone(),
        model: spawn.model.clone(),
        reasoning_effort: spawn.reasoning_effort.clone(),
        receiver_thread_id: spawn.receiver_thread_id.clone(),
        receiver_nickname: spawn.receiver_nickname.clone(),
        receiver_role: spawn.receiver_role.clone(),
        receiver_status: spawn.receiver_status.clone(),
    })
}

fn merged_spawn_agent_render_data(
    call: &EventEntry,
    result: &EventEntry,
) -> Option<SpawnAgentRenderData> {
    let call_data = spawn_agent_render_data(call).unwrap_or_default();
    let result_data = spawn_agent_render_data(result).unwrap_or_default();
    let merged = SpawnAgentRenderData {
        prompt: call_data.prompt.or(result_data.prompt),
        requested_agent_type: call_data
            .requested_agent_type
            .or(result_data.requested_agent_type),
        model: result_data.model.or(call_data.model),
        reasoning_effort: result_data.reasoning_effort.or(call_data.reasoning_effort),
        receiver_thread_id: result_data
            .receiver_thread_id
            .or(call_data.receiver_thread_id),
        receiver_nickname: result_data
            .receiver_nickname
            .or(call_data.receiver_nickname),
        receiver_role: result_data.receiver_role.or(call_data.receiver_role),
        receiver_status: result_data.receiver_status.or(call_data.receiver_status),
    };
    (merged != SpawnAgentRenderData::default()).then_some(merged)
}

fn render_combined_spawn_agent_meta_row(call: &EventEntry, result: &EventEntry) -> String {
    let mut items = Vec::new();
    if let Some(data) = merged_spawn_agent_render_data(call, result) {
        if let Some(agent_type) = data
            .requested_agent_type
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            items.push(render_event_meta_item(
                "agent type",
                &escape_html(agent_type),
            ));
        }
        if let Some(model) = data.model.as_deref().filter(|value| !value.is_empty()) {
            items.push(render_event_meta_item("model", &escape_html(model)));
        }
        if let Some(reasoning_effort) = data
            .reasoning_effort
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            items.push(render_event_meta_item(
                "effort",
                &escape_html(reasoning_effort),
            ));
        }
        if let Some(status) = data
            .receiver_status
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            items.push(render_event_meta_item("status", &escape_html(status)));
        }
    }
    if call.parse_status != "parsed" {
        items.push(render_event_meta_item(
            "call parse",
            &escape_html(&call.parse_status),
        ));
        items.push(render_event_meta_item(
            "call raw",
            &escape_html(&call.raw_type),
        ));
    }
    if result.parse_status != "parsed" {
        items.push(render_event_meta_item(
            "result parse",
            &escape_html(&result.parse_status),
        ));
        items.push(render_event_meta_item(
            "result raw",
            &escape_html(&result.raw_type),
        ));
    }

    if items.is_empty() {
        String::new()
    } else {
        format!("<div class=\"event-meta\">{}</div>", items.join(""))
    }
}

fn render_plan_step_status(status: &str) -> String {
    let class_name = match status {
        "completed" => "plan-step-status is-completed",
        "in_progress" => "plan-step-status is-in-progress",
        "pending" => "plan-step-status is-pending",
        _ => "plan-step-status",
    };
    format!(
        "<span class=\"{}\">{}</span>",
        class_name,
        escape_html(&status.replace('_', " ")),
    )
}

fn render_runtime_context_block(event: &EventEntry) -> Option<String> {
    if event.event_type != RUNTIME_CONTEXT || event.runtime_context_pairs.is_empty() {
        return None;
    }

    let mut out = String::from("<table class=\"kv-table\"><tbody>");
    for (label, value) in &event.runtime_context_pairs {
        let rendered_value = render_runtime_context_value(label, value);
        let _ = write!(
            out,
            "<tr class=\"kv-table-row\"><th class=\"kv-table-label\">{}</th><td class=\"{}\">{}</td></tr>",
            escape_html(label),
            "kv-table-value",
            rendered_value,
        );
    }
    out.push_str("</tbody></table>");

    Some(render_inset_block(
        "runtime.context",
        &out,
        None,
        "runtime-context-block",
        true,
    ))
}

fn render_runtime_context_value(label: &str, value: &str) -> String {
    if is_runtime_context_collapsible_key(label) && should_collapse_text_content(value) {
        return render_collapsible_text_block(value);
    }

    format!("<code>{}</code>", escape_html(value))
}

fn is_runtime_context_collapsible_key(label: &str) -> bool {
    label == "collaboration_mode" || label.ends_with("_instructions")
}

fn should_skip_event_summary(event: &EventEntry) -> bool {
    if event.event_type == RUNTIME_CONTEXT {
        return true;
    }
    if is_task_started_event(event) {
        return true;
    }
    if event.event_type == COLLAB_SPAWN_AGENT && event.spawn_agent.is_some() {
        return true;
    }
    if event.event_type == USER_INPUT_REQUEST && event.user_input_request.is_some() {
        return true;
    }
    let summary = event.summary.trim();
    summary.is_empty() || (event.summary_pairs.is_empty() && summary == event.event_type)
}

fn is_task_started_event(event: &EventEntry) -> bool {
    event.event_type == TASK_STARTED
        || (event.event_type == "agent.meta" && event.meta_type.as_deref() == Some("task_started"))
}

fn is_task_completed_event(event: &EventEntry) -> bool {
    event.event_type == TASK_COMPLETED
        || (event.event_type == "agent.meta" && event.meta_type.as_deref() == Some("task_complete"))
}

fn render_event_meta_row(event: &EventEntry) -> String {
    let mut items = Vec::new();
    if is_task_started_event(event) {
        if let Some(mode) = event
            .collaboration_mode_kind
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            items.push(render_event_meta_item("mode", &escape_html(mode)));
        }
        if let Some(turn_id) = event.turn_id.as_deref().filter(|value| !value.is_empty()) {
            items.push(render_event_meta_item("turn", &escape_html(turn_id)));
        }
        if let Some(window) = event
            .model_context_window
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            items.push(render_event_meta_item(
                "context window",
                &escape_html(window),
            ));
        }
    } else if is_task_completed_event(event) {
        if let Some(turn_id) = event.turn_id.as_deref().filter(|value| !value.is_empty()) {
            items.push(render_event_meta_item("turn", &escape_html(turn_id)));
        }
    }
    if event.event_type.starts_with("message.") {
        if let Some(role) = event
            .message_role
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            items.push(render_event_meta_item("role", &escape_html(role)));
        }
        if let Some(direction) = event
            .message_direction
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            items.push(render_event_meta_item("direction", &escape_html(direction)));
        }
        if let Some(phase) = event.phase.as_deref().filter(|value| !value.is_empty()) {
            items.push(render_event_meta_item("phase", &escape_html(phase)));
        }
    }
    if event.event_type == USER_INPUT_REQUEST {
        if let Some(request) = event.user_input_request.as_ref() {
            items.push(render_event_meta_item(
                "questions",
                &escape_html(&request.questions.len().to_string()),
            ));
            items.push(render_event_meta_item(
                "answers",
                &escape_html(&total_user_input_request_answer_count(request).to_string()),
            ));
        }
    }
    if let Some(body_size_bytes) = message_size_bytes_for_event(event) {
        items.push(render_event_meta_item(
            "message size",
            &escape_html(&format_body_size_bytes(body_size_bytes)),
        ));
    } else if let Some(body_size_bytes) = shell_output_size_bytes_for_event(event) {
        items.push(render_event_meta_item(
            "output size",
            &escape_html(&format_body_size_bytes(body_size_bytes)),
        ));
    }
    if event.parse_status != "parsed" {
        items.push(render_event_meta_item(
            "parse",
            &escape_html(&event.parse_status),
        ));
        items.push(render_event_meta_item("raw", &escape_html(&event.raw_type)));
    }

    if items.is_empty() {
        String::new()
    } else {
        format!("<div class=\"event-meta\">{}</div>", items.join(""))
    }
}

fn render_combined_shell_meta_row(call: &EventEntry, result: &EventEntry) -> String {
    let mut items = Vec::new();
    if let Some(body_size_bytes) = combined_shell_output_size_bytes(call, result) {
        items.push(render_event_meta_item(
            "output size",
            &escape_html(&format_body_size_bytes(body_size_bytes)),
        ));
    }
    if call.parse_status != "parsed" {
        items.push(render_event_meta_item(
            "call parse",
            &escape_html(&call.parse_status),
        ));
        items.push(render_event_meta_item(
            "call raw",
            &escape_html(&call.raw_type),
        ));
    }
    if result.parse_status != "parsed" {
        items.push(render_event_meta_item(
            "result parse",
            &escape_html(&result.parse_status),
        ));
        items.push(render_event_meta_item(
            "result raw",
            &escape_html(&result.raw_type),
        ));
    }

    if items.is_empty() {
        String::new()
    } else {
        format!("<div class=\"event-meta\">{}</div>", items.join(""))
    }
}

fn render_combined_user_input_request_meta_row(call: &EventEntry, result: &EventEntry) -> String {
    let mut items = Vec::new();
    if let Some(request) = merged_user_input_request_entry(call, result) {
        items.push(render_event_meta_item(
            "questions",
            &escape_html(&request.questions.len().to_string()),
        ));
        items.push(render_event_meta_item(
            "answers",
            &escape_html(&total_user_input_request_answer_count(&request).to_string()),
        ));
    }
    if call.parse_status != "parsed" {
        items.push(render_event_meta_item(
            "call parse",
            &escape_html(&call.parse_status),
        ));
        items.push(render_event_meta_item(
            "call raw",
            &escape_html(&call.raw_type),
        ));
    }
    if result.parse_status != "parsed" {
        items.push(render_event_meta_item(
            "result parse",
            &escape_html(&result.parse_status),
        ));
        items.push(render_event_meta_item(
            "result raw",
            &escape_html(&result.raw_type),
        ));
    }

    if items.is_empty() {
        String::new()
    } else {
        format!("<div class=\"event-meta\">{}</div>", items.join(""))
    }
}

fn render_combined_collab_operation_meta_row(call: &EventEntry, result: &EventEntry) -> String {
    let mut items = Vec::new();
    if !result.receiver_thread_ids.is_empty() {
        items.push(render_event_meta_item(
            "agents",
            &escape_html(&result.receiver_thread_ids.len().to_string()),
        ));
    }
    if collab_operation_state_count(result) > 0 {
        items.push(render_event_meta_item(
            "updates",
            &escape_html(&collab_operation_state_count(result).to_string()),
        ));
    }
    if call.parse_status != "parsed" {
        items.push(render_event_meta_item(
            "call parse",
            &escape_html(&call.parse_status),
        ));
        items.push(render_event_meta_item(
            "call raw",
            &escape_html(&call.raw_type),
        ));
    }
    if result.parse_status != "parsed" {
        items.push(render_event_meta_item(
            "result parse",
            &escape_html(&result.parse_status),
        ));
        items.push(render_event_meta_item(
            "result raw",
            &escape_html(&result.raw_type),
        ));
    }

    if items.is_empty() {
        String::new()
    } else {
        format!("<div class=\"event-meta\">{}</div>", items.join(""))
    }
}

fn render_event_meta_item(label: &str, value_html: &str) -> String {
    format!(
        "<span class=\"event-meta-item\"><span class=\"event-meta-label\">{}</span><span class=\"event-meta-value\">{}</span></span>",
        escape_html(label),
        value_html,
    )
}

fn message_size_bytes_for_event(event: &EventEntry) -> Option<usize> {
    if !event.event_type.starts_with("message.") {
        return None;
    }
    event
        .message_text
        .as_deref()
        .filter(|text| !text.is_empty())
        .map(|text| text.as_bytes().len())
}

fn shell_output_size_bytes_for_event(event: &EventEntry) -> Option<usize> {
    if !is_command_shell_event(event) {
        return None;
    }
    shell_output_size_bytes(event.aggregated_output.as_deref())
}

fn combined_shell_output_size_bytes(_call: &EventEntry, result: &EventEntry) -> Option<usize> {
    shell_output_size_bytes(result.aggregated_output.as_deref())
}

fn shell_output_size_bytes(output: Option<&str>) -> Option<usize> {
    output
        .filter(|value| !value.is_empty())
        .map(|value| value.as_bytes().len())
}

fn format_body_size_bytes(bytes: usize) -> String {
    format!("{} B", format_chart_number(bytes as u64))
}

fn render_shell_result_block(event: &EventEntry) -> Option<String> {
    if event.event_type != SHELL_RESULT {
        return None;
    }
    if !is_command_shell_event(event) {
        return None;
    }

    render_shell_operation_block(
        event.shell_command.as_deref(),
        event.aggregated_output.as_deref(),
        event.shell_exit_code,
    )
}

fn render_shell_operation_block(
    command: Option<&str>,
    output: Option<&str>,
    exit_code: Option<i32>,
) -> Option<String> {
    if command.is_none() && output.is_none() && exit_code.is_none() {
        return None;
    }

    let command = command
        .map(|command| {
            format!(
                "<div class=\"shell-block-command\"><strong>$ {}</strong></div>",
                escape_html(command)
            )
        })
        .unwrap_or_default();
    let output = match output {
        Some(output) if should_collapse_text_content(output) => {
            render_collapsible_text_block(output)
        }
        Some(output) => format!(
            "<div class=\"summary-text shell-block-output\">{}</div>",
            escape_html(output)
        ),
        None => "<div class=\"summary-text shell-block-output shell-block-empty\">Нет вывода</div>"
            .to_string(),
    };
    let footer = render_shell_result_status(exit_code);
    let body = format!("{command}<div class=\"shell-block-output-wrap\">{output}</div>");
    Some(render_inset_block(
        "Shell",
        &body,
        Some(&footer),
        "shell-block",
        true,
    ))
}

fn render_shell_result_status(exit_code: Option<i32>) -> String {
    match exit_code {
        Some(0) => {
            "<span class=\"shell-block-status is-success\">&#10003; Успех</span>".to_string()
        }
        Some(code) => format!(
            "<span class=\"shell-block-status is-failure\">&#10005; Код {}</span>",
            code
        ),
        None => "<span class=\"shell-block-status\">Код неизвестен</span>".to_string(),
    }
}

fn render_token_footnote(event: &EventEntry, last_token_usage: &mut TokenUsage) -> String {
    if event.event_type != INFO_TOKENS {
        return String::new();
    }

    let current_usage = token_usage_from_event(event);
    let diff = current_usage.diff_from(*last_token_usage);
    *last_token_usage = current_usage;

    let mut out = String::new();
    let _ = write!(
        out,
        "<div class=\"event-footnote\" data-footnote-event-type=\"{}\" data-footnote-event-id=\"{}\">\
         <div class=\"event-footnote-content\">\
         <span class=\"event-footnote-seq\">#{:04}</span>\
         <span class=\"event-footnote-title\">token consumption</span>",
        escape_html(&event.event_type),
        escape_html(&event.event_id),
        event.seq,
    );
    for (label, value, diff_value, pair_class) in [
        ("input", event.input_tokens, diff.input_tokens, ""),
        (
            "cached input",
            event.cached_input_tokens,
            diff.cached_input_tokens,
            "",
        ),
        ("output", event.output_tokens, diff.output_tokens, ""),
        (
            "reasoning output",
            event.reasoning_output_tokens,
            diff.reasoning_output_tokens,
            "",
        ),
        ("total", event.total_tokens, diff.total_tokens, " is-total"),
    ] {
        let Some(value) = value else {
            continue;
        };
        let _ = write!(
            out,
            "<span class=\"event-footnote-pair{}\"><span class=\"event-footnote-label\">{}</span><span class=\"event-footnote-diff {}\">Δ {}</span><span class=\"event-footnote-value\">{}</span></span>",
            pair_class,
            escape_html(label),
            diff_class(diff_value),
            escape_html(&format_signed_number(diff_value)),
            escape_html(&format_chart_number(value)),
        );
    }
    out.push_str("</div></div>");
    out
}

fn token_usage_from_event(event: &EventEntry) -> TokenUsage {
    TokenUsage {
        input_tokens: event.input_tokens.unwrap_or(0),
        cached_input_tokens: event.cached_input_tokens.unwrap_or(0),
        output_tokens: event.output_tokens.unwrap_or(0),
        reasoning_output_tokens: event.reasoning_output_tokens.unwrap_or(0),
        total_tokens: event.total_tokens.unwrap_or(0),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
struct TokenUsageDiff {
    input_tokens: i128,
    cached_input_tokens: i128,
    output_tokens: i128,
    reasoning_output_tokens: i128,
    total_tokens: i128,
}

impl TokenUsage {
    fn diff_from(self, previous: TokenUsage) -> TokenUsageDiff {
        TokenUsageDiff {
            input_tokens: self.input_tokens as i128 - previous.input_tokens as i128,
            cached_input_tokens: self.cached_input_tokens as i128
                - previous.cached_input_tokens as i128,
            output_tokens: self.output_tokens as i128 - previous.output_tokens as i128,
            reasoning_output_tokens: self.reasoning_output_tokens as i128
                - previous.reasoning_output_tokens as i128,
            total_tokens: self.total_tokens as i128 - previous.total_tokens as i128,
        }
    }
}

fn diff_class(value: i128) -> &'static str {
    if value > 0 {
        "diff-pos"
    } else if value < 0 {
        "diff-neg"
    } else {
        ""
    }
}

fn format_chart_number(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            out.push(' ');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

fn format_signed_number(value: i128) -> String {
    let abs = value.unsigned_abs() as u64;
    let rendered = format_chart_number(abs);
    if value > 0 {
        format!("+{rendered}")
    } else if value < 0 {
        format!("-{rendered}")
    } else {
        "0".to_string()
    }
}

fn render_subagent_badge(event: &EventEntry) -> String {
    if event.actor_type.as_deref() != Some("subagent") {
        return String::new();
    }
    let Some(label) = event
        .subagent_nickname
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return String::new();
    };

    format!(
        "<span class=\"badge cat-subagent\">{}</span>",
        escape_html(label)
    )
}

fn is_command_shell_event(event: &EventEntry) -> bool {
    matches!(event.event_type.as_str(), SHELL_CALL | SHELL_RESULT)
        && matches!(
            event.tool_name.as_deref(),
            Some("command_execution" | "exec_command")
        )
}

fn shell_operation_ids_match(call: &EventEntry, result: &EventEntry) -> bool {
    match (call.operation_id.as_deref(), result.operation_id.as_deref()) {
        (Some(left), Some(right)) => left == right,
        _ => true,
    }
}

fn category_class(category: EventSummaryCategory) -> &'static str {
    match category {
        EventSummaryCategory::Assistant => "cat-assistant",
        EventSummaryCategory::Command => "cat-command",
        EventSummaryCategory::Search => "cat-search",
        EventSummaryCategory::Subagent => "cat-subagent",
        EventSummaryCategory::File => "cat-file",
        EventSummaryCategory::Todo => "cat-todo",
        EventSummaryCategory::Error => "cat-error",
        EventSummaryCategory::Default => "cat-default",
    }
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::{Path, PathBuf};

    use codex_worker_rs::models::EventRecord;
    use serde_json::json;
    use tempfile::tempdir;

    use super::{default_output_path, render_html, run, Cli};
    use crate::events_tree_shared::{
        build_event_tree, build_event_tree_with_standalone_startup_metadata,
        is_rollout_jsonl_family, is_run_input, load_records_from_run_input, TimelineItem,
    };

    fn make_raw_event(
        event_type: &str,
        raw_type: &str,
        payload: serde_json::Value,
        seq: u64,
    ) -> EventRecord {
        EventRecord {
            schema_version: 1,
            ts: "2026-04-06T08:47:59Z".to_string(),
            task_id: "smoke-run".to_string(),
            run_id: "run-1".to_string(),
            seq,
            event_type: event_type.to_string(),
            raw_type: raw_type.to_string(),
            parse_status: "parsed".to_string(),
            payload,
        }
    }

    fn make_event(event_type: &str, payload: serde_json::Value, seq: u64) -> EventRecord {
        make_raw_event(event_type, event_type, payload, seq)
    }

    fn rollout_root_name(session_id: &str) -> String {
        format!("rollout-2026-04-06T22-54-37-{session_id}.jsonl")
    }

    fn count_occurrences(haystack: &str, needle: &str) -> usize {
        haystack.match_indices(needle).count()
    }

    #[test]
    fn render_html_contains_nested_thread_sections() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "collab.spawn_agent",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"spawn_agent",
                    "phase":"completed",
                    "status":"completed",
                    "receiver_thread_ids":["sub-1"],
                    "agents_states":{"sub-1":{"status":"pending_init"}}
                }),
                2,
            ),
            make_event(
                "agent.session",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "agent_role":"default",
                    "agent_nickname":"Hegel"
                }),
                3,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let html = render_html(&tree);

        assert!(html.contains("Дерево событий"));
        assert!(html.contains("root-thread"));
        assert!(html.contains("sub-1"));
        assert!(html.contains("thread-flow"));
        assert!(html.contains("cat-subagent"));
        assert!(html.contains("data-event-id=\"run-1:2\""));
    }

    #[test]
    fn render_html_includes_standalone_startup_metadata_cards() {
        let events = vec![make_event(
            "thread.started",
            json!({"thread_id":"root-thread"}),
            1,
        )];
        let startup = serde_json::Map::from_iter([
            ("id".to_string(), json!("sub1")),
            ("approval_policy".to_string(), json!("never")),
            ("model".to_string(), json!("gpt-5")),
            (
                "git".to_string(),
                json!({
                    "branch":"implementation-orchestrator/codex-worker-rs-mvp-stage-0-3",
                    "commit_hash":"0369291d3f2b5edb8c02a512abef201949b2d06b"
                }),
            ),
        ]);
        let tree = build_event_tree_with_standalone_startup_metadata(
            Path::new("/tmp/rollout-smoke-sub1.jsonl"),
            &events,
            Some(startup),
            120,
        );

        let html = render_html(&tree);

        assert!(html.contains("class=\"hero-grid\""));
        assert!(html.contains("class=\"hero-startup\""));
        assert!(html.contains("class=\"hero-startup-card\"><span class=\"hero-card-label\">id<"));
        assert!(html.contains(
            "class=\"hero-startup-card\"><span class=\"hero-card-label\">approval_policy<"
        ));
        assert!(html.contains("class=\"hero-startup-card\"><span class=\"hero-card-label\">model<"));
        assert!(html.contains("class=\"hero-startup-card\"><span class=\"hero-card-label\">git<"));
        assert!(html.contains("class=\"hero-startup-row-label\">branch<"));
        assert!(html.contains("class=\"hero-startup-row-label\">commit_hash<"));
        assert!(html.contains("implementation-orchestrator/codex-worker-rs-mvp-stage-0-3"));
        assert!(html.contains("0369291d3f2b5edb8c02a512abef201949b2d06b"));
        assert!(html.contains(">never<"));
    }

    #[test]
    fn render_html_collapses_startup_base_instructions() {
        let events = vec![make_event(
            "thread.started",
            json!({"thread_id":"root-thread"}),
            1,
        )];
        let startup = serde_json::Map::from_iter([
            (
                "base_instructions".to_string(),
                json!({"text": "instruction line\n".repeat(40)}),
            ),
            ("model".to_string(), json!("gpt-5")),
        ]);
        let tree = build_event_tree_with_standalone_startup_metadata(
            Path::new("/tmp/rollout-smoke-sub1.jsonl"),
            &events,
            Some(startup),
            120,
        );

        let html = render_html(&tree);

        assert!(html.contains(">base_instructions<"));
        assert!(html.contains("class=\"hero-base\""));
        assert!(html.contains("class=\"inset-block meta-inset-block inline-title\""));
        assert!(html.contains("class=\"inset-block-title inline-title\">base_instructions<"));
        assert!(html.contains("class=\"inset-block-body\"><div class=\"message-collapse\""));
        assert!(html.contains("instruction line"));
        assert!(html.contains(">see full<"));
        assert!(
            html.find("class=\"hero-base\"")
                .expect("base instructions block should exist")
                < html
                    .find("class=\"tree\"")
                    .expect("tree section should exist")
        );
    }

    #[test]
    fn render_html_shows_subagent_name_for_subagent_message_without_thread_id() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "agent.session",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "agent_role":"reviewer",
                    "agent_nickname":"Lovelace"
                }),
                2,
            ),
            make_event(
                "message.agent",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "text":"hello"
                }),
                3,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let html = render_html(&tree);

        assert!(html.contains("class=\"badge cat-subagent\">Lovelace<"));
        assert!(html.contains(">Lovelace<"));
        assert!(!html.contains("Lovelace [sub-1]"));
    }

    #[test]
    fn render_html_shows_plain_message_text_and_message_meta() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "message.system",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "role":"system",
                    "direction":"output_text",
                    "phase":"commentary",
                    "text":"hello"
                }),
                2,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let html = render_html(&tree);

        assert!(html.contains(">message.system<"));
        assert!(html.contains("<div class=\"summary-text\">hello</div>"));
        assert!(html.contains("event-meta-label\">role<"));
        assert!(html.contains("event-meta-value\">system<"));
        assert!(html.contains("event-meta-label\">direction<"));
        assert!(html.contains("event-meta-value\">output_text<"));
        assert!(html.contains("event-meta-label\">phase<"));
        assert!(html.contains("event-meta-value\">commentary<"));
        assert!(html.contains("event-meta-label\">message size<"));
        assert!(html.contains("event-meta-value\">5 B<"));
        assert!(!html.contains("system: hello"));
    }

    #[test]
    fn render_html_highlights_user_prompt_card() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "message.user",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "role":"user",
                    "direction":"input_text",
                    "text":"покажи дерево событий"
                }),
                2,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let html = render_html(&tree);

        assert!(html.contains(".event-card.is-user-prompt{"));
        assert!(html.contains("class=\"event-card is-user-prompt\""));
        assert!(html.contains("class=\"badge cat-subagent\">message.user<"));
        assert!(html.contains(">покажи дерево событий<"));
    }

    #[test]
    fn render_html_shows_runtime_context_requisites() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "runtime.context",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "session_path":"/tmp/sub-1.jsonl",
                    "cwd":"/workspace",
                    "current_date":"2026-04-07",
                    "timezone":"Europe/Moscow",
                    "approval_policy":"never",
                    "sandbox_policy":{"type":"workspace-write","network_access":true},
                    "model":"gpt-5.4",
                    "effort":"medium",
                    "summary":"ok",
                    "collaboration_mode":{"mode":"default"}
                }),
                2,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let html = render_html(&tree);

        assert!(html.contains(">runtime.context<"));
        assert!(html.contains("class=\"inset-block runtime-context-block inline-title\""));
        assert!(html.contains("class=\"inset-block-title inline-title\">runtime.context<"));
        assert!(html.contains("class=\"kv-table-label\">cwd<"));
        assert!(html.contains("class=\"kv-table-value\"><code>/workspace</code>"));
        assert!(html.contains("class=\"kv-table-label\">sandbox_policy<"));
        assert!(html.contains("workspace-write"));
        assert!(html.contains("network_access"));
        assert!(html.contains("class=\"kv-table-label\">collaboration_mode<"));
        assert!(html.contains("&quot;mode&quot;:&quot;default&quot;"));
        assert!(!html.contains("class=\"kv-table-label\">turn_id<"));
        assert!(!html.contains("context: cwd=/workspace model=gpt-5.4 mode=default"));
    }

    #[test]
    fn render_html_shows_plan_update_block() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "plan.update",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "tool_name":"update_plan",
                    "tool_use_id":"plan-1",
                    "phase":"started",
                    "input":{
                        "explanation":"Analysis approved; moving to branch decision and executable checklist before code changes.",
                        "plan":[
                            {"step":"Verify branch decision and working tree isolation for implementation branch","status":"in_progress"},
                            {"step":"Build approved executable TODO checklist from the plan","status":"pending"}
                        ]
                    }
                }),
                2,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let html = render_html(&tree);

        assert!(html.contains(">plan.update<"));
        assert!(html.contains("class=\"inset-block plan-block inline-title\""));
        assert!(html.contains("class=\"inset-block-title inline-title\">Plan<"));
        assert!(html.contains("Analysis approved; moving to branch decision and executable checklist before code changes."));
        assert!(html.contains(
            "Verify branch decision and working tree isolation for implementation branch"
        ));
        assert!(html.contains("Build approved executable TODO checklist from the plan"));
        assert!(html.contains("plan-step-status is-in-progress\">in progress<"));
        assert!(html.contains("plan-step-status is-pending\">pending<"));
    }

    #[test]
    fn render_html_shows_task_started_and_task_completed_fields() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "agent.session",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "agent_role":"reviewer",
                    "agent_nickname":"Ada"
                }),
                2,
            ),
            make_event(
                "task.started",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "turn_id":"019d6447-496b-7f73-b7cc-61a0cf42b81f",
                    "model_context_window":256000,
                    "collaboration_mode_kind":"default"
                }),
                3,
            ),
            make_event(
                "task.completed",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "turn_id":"019d6447-496b-7f73-b7cc-61a0cf42b81f",
                    "last_agent_message":"**Result**\n\n`APPROVED`\n\n- resolved A\n- resolved B\n- resolved C\n"
                }),
                4,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let html = render_html(&tree);

        assert!(!html.contains(
            "started: mode=default turn=019d6447-496b-7f73-b7cc-61a0cf42b81f window=256000"
        ));
        assert!(html.contains("event-meta-label\">mode<"));
        assert!(html.contains("event-meta-value\">default<"));
        assert!(html.contains("event-meta-label\">turn<"));
        assert!(html.contains("019d6447-496b-7f73-b7cc-61a0cf42b81f"));
        assert!(html.contains("event-meta-label\">context window<"));
        assert!(html.contains("event-meta-value\">256000<"));
        assert!(html.contains("class=\"inset-block task-message-block inline-title\""));
        assert!(html.contains("class=\"inset-block-title inline-title\">Last Agent Message<"));
        assert!(html.contains("class=\"message-collapse\""));
        assert!(html.contains("**Result**"));
        assert!(html.contains("resolved A"));
        assert!(html.contains("resolved C"));
    }

    #[test]
    fn render_html_collapses_request_user_input_call_and_result_into_single_card() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "user.input.request",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "tool_name":"request_user_input",
                    "tool_use_id":"rui-1",
                    "phase":"started",
                    "input":{
                        "questions":[
                            {
                                "header":"Поиск детей",
                                "id":"child_lookup",
                                "question":"Как искать дочерние session-файлы для HTML-дерева, когда вход — один session-файл?",
                                "options":[
                                    {
                                        "label":"Тот же каталог (Recommended)",
                                        "description":"Искать только рядом с исходным файлом по `receiver_thread_ids`, без глобального сканирования `CODEX_HOME`."
                                    },
                                    {
                                        "label":"Весь sessions root",
                                        "description":"Разрешить рекурсивный поиск по всему `.../sessions`, чтобы собрать дерево даже при разнесённых файлах."
                                    }
                                ]
                            }
                        ]
                    }
                }),
                2,
            ),
            make_event(
                "user.input.request",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "tool_name":"request_user_input",
                    "tool_use_id":"rui-1",
                    "phase":"completed",
                    "output":{
                        "answers":{
                            "child_lookup":{
                                "answers":["Тот же каталог (Recommended)"]
                            }
                        }
                    }
                }),
                3,
            ),
            make_event(
                "message.agent",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "text":"after request"
                }),
                4,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let html = render_html(&tree);

        assert!(html.contains("data-seq=\"2,3\""));
        assert!(html.contains("data-event-ids=\"run-1:2,run-1:3\""));
        assert!(html.contains(">#0002, #0003<"));
        assert!(html.contains("class=\"inset-block user-input-block inline-title\""));
        assert!(html.contains("class=\"inset-block-title inline-title\">User Input<"));
        assert!(html.contains("event-meta-label\">questions<"));
        assert!(html.contains("event-meta-value\">1<"));
        assert!(html.contains("event-meta-label\">answers<"));
        assert!(html.contains("class=\"user-input-question-tag-label\">header<"));
        assert!(html.contains(">Поиск детей<"));
        assert!(html.contains("class=\"user-input-question-tag-label\">id<"));
        assert!(html.contains("<code>child_lookup</code>"));
        assert!(html.contains(
            "Как искать дочерние session-файлы для HTML-дерева, когда вход — один session-файл?"
        ));
        assert!(html.contains("Тот же каталог (Recommended)"));
        assert!(html.contains("class=\"user-input-option is-selected\""));
        assert!(html.contains("class=\"user-input-answer-chip is-selected\">selected<"));
        assert!(!html.contains("data-seq=\"3\""));
        assert!(!html.contains("data-event-id=\"run-1:3\""));

        let request_pos = html
            .find("data-seq=\"2,3\"")
            .expect("collapsed request card should be rendered");
        let message_pos = html
            .find("data-seq=\"4\"")
            .expect("later message should be rendered");
        assert!(request_pos < message_pos);
    }

    #[test]
    fn render_html_merges_plan_item_completed_and_response_message_into_message_plan() {
        let plan_text = "# HTML-просмотр дерева событий по session-файлу\n\n## Summary\n\nДобавить поддержку session-файла.";
        let wrapped_plan = format!("<proposed_plan>\n{plan_text}\n</proposed_plan>");
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_raw_event(
                "plan.update",
                "event_msg",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "tool_name":"update_plan",
                    "tool_use_id":"turn-1-plan",
                    "phase":"completed",
                    "status":"completed",
                    "duplicate_of":"response_item.message",
                    "output":{
                        "item_type":"Plan",
                        "item_id":"turn-1-plan",
                        "text":plan_text,
                        "turn_id":"turn-1"
                    }
                }),
                2,
            ),
            make_raw_event(
                "message.assistant",
                "response_item",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "role":"assistant",
                    "direction":"output_text",
                    "phase":"final_answer",
                    "text":wrapped_plan
                }),
                3,
            ),
            make_event(
                "message.assistant",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "role":"assistant",
                    "direction":"output_text",
                    "phase":"commentary",
                    "text":"after plan"
                }),
                4,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let html = render_html(&tree);

        assert!(html.contains("badge cat-assistant\">message.plan<"));
        assert!(html.contains("event-meta-label\">role<"));
        assert!(html.contains("event-meta-value\">assistant<"));
        assert!(html.contains("event-meta-label\">phase<"));
        assert!(html.contains("event-meta-value\">final_answer<"));
        assert!(html.contains("HTML-просмотр дерева событий по session-файлу"));
        assert!(!html.contains("&lt;proposed_plan&gt;"));
        assert!(!html.contains(">plan.update<"));
        assert!(!html.contains("data-event-id=\"run-1:3\""));
        assert!(html.contains("data-event-id=\"run-1:2\""));
    }

    #[test]
    fn render_html_moves_runtime_context_tail_fields_last_and_collapses_them() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "runtime.context",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "session_path":"/tmp/sub-1.jsonl",
                    "system_instructions":"instruction line\n".repeat(40),
                    "cwd":"/workspace",
                    "model":"gpt-5.4",
                    "collaboration_mode":{"mode":"default","review":"strict"}
                }),
                2,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let html = render_html(&tree);

        let cwd_pos = html.find("class=\"kv-table-label\">cwd<").expect("cwd row");
        let model_pos = html
            .find("class=\"kv-table-label\">model<")
            .expect("model row");
        let collaboration_mode_pos = html
            .find("class=\"kv-table-label\">collaboration_mode<")
            .expect("collaboration_mode row");
        let instructions_pos = html
            .find("class=\"kv-table-label\">system_instructions<")
            .expect("system_instructions row");

        assert!(cwd_pos < collaboration_mode_pos);
        assert!(model_pos < collaboration_mode_pos);
        assert!(model_pos < instructions_pos);
        assert!(html.contains("instruction line"));
        assert!(html.contains("class=\"message-collapse\""));
        assert!(html.contains("class=\"kv-table-value\"><div class=\"message-collapse\""));
        assert!(html.contains(">see full<"));
    }

    #[test]
    fn render_html_hides_redundant_summary_for_status_like_events() {
        let events = vec![make_event(
            "thread.started",
            json!({"thread_id":"root-thread"}),
            1,
        )];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let html = render_html(&tree);

        assert!(!html.contains(">summary<"));
        assert!(html.contains("class=\"badge cat-default\">thread.started<"));
    }

    #[test]
    fn render_html_hides_raw_type_and_parse_for_cleanly_parsed_events() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "message.agent",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "text":"hello"
                }),
                2,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let html = render_html(&tree);

        assert!(!html.contains(">raw_type<"));
        assert!(!html.contains(">parse<"));
        assert!(html.contains("data-raw-type=\"message.agent\""));
        assert!(html.contains("data-parse-status=\"parsed\""));
    }

    #[test]
    fn render_html_styles_info_tokens_pairs() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "info.tokens",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "input_tokens":11877,
                    "cached_input_tokens":4480,
                    "output_tokens":758,
                    "reasoning_output_tokens":516,
                    "total_tokens":12635
                }),
                2,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let html = render_html(&tree);

        assert!(!html.contains("class=\"badge cat-default\">info.tokens<"));
        assert!(!html.contains("class=\"summary-structured\""));
        assert!(html.contains("event-footnote-title\">token consumption<"));
        assert!(html.contains("event-footnote-label\">input<"));
        assert!(html.contains("event-footnote-label\">cached input<"));
        assert!(html.contains("event-footnote-value\">11 877<"));
        assert!(html.contains("event-footnote-value\">12 635<"));
    }

    #[test]
    fn render_html_renders_info_tokens_footnotes() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "message.agent",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "text":"first"
                }),
                2,
            ),
            make_event(
                "info.tokens",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "input_tokens":700,
                    "cached_input_tokens":120,
                    "output_tokens":340,
                    "reasoning_output_tokens":74,
                    "total_tokens":1234
                }),
                3,
            ),
            make_event(
                "collab.spawn_agent",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"spawn_agent",
                    "phase":"completed",
                    "status":"completed",
                    "receiver_thread_ids":["sub-1"],
                    "agents_states":{"sub-1":{"status":"pending_init"}}
                }),
                4,
            ),
            make_event(
                "agent.session",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "agent_role":"reviewer",
                    "agent_nickname":"Ada"
                }),
                5,
            ),
            make_event(
                "message.agent",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "text":"second"
                }),
                6,
            ),
            make_event(
                "info.tokens",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "input_tokens":900,
                    "cached_input_tokens":300,
                    "output_tokens":280,
                    "reasoning_output_tokens":60,
                    "total_tokens":1500
                }),
                7,
            ),
            make_event(
                "message.agent",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "text":"child"
                }),
                8,
            ),
            make_event(
                "info.tokens",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "input_tokens":111,
                    "cached_input_tokens":22,
                    "output_tokens":33,
                    "reasoning_output_tokens":4,
                    "total_tokens":170
                }),
                9,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let html = render_html(&tree);

        assert!(!html.contains("Token Bar Chart"));
        assert_eq!(
            count_occurrences(&html, "data-footnote-event-type=\"info.tokens\""),
            3
        );
        assert!(html.contains("data-footnote-event-id=\"run-1:3\""));
        assert!(html.contains("data-footnote-event-id=\"run-1:7\""));
        assert!(html.contains("data-footnote-event-id=\"run-1:9\""));
        assert!(html.contains("event-footnote-title\">token consumption<"));
        assert!(!html.contains("class=\"badge cat-default\">info.tokens<"));
        assert!(html.contains("event-footnote-label\">input<"));
        assert!(html.contains("class=\"event-footnote-pair is-total\""));
        assert!(html.contains("event-footnote-value\">1 234<"));
        assert!(html.contains("event-footnote-value\">1 500<"));
        assert!(html.contains("event-footnote-value\">170<"));
        assert!(html.contains("Δ +1 234"));
        assert!(html.contains("Δ +266"));
        assert!(html.contains("Δ +170"));
    }

    #[test]
    fn render_html_collapses_large_message_summary_with_toggle() {
        let long_text = "assistant-message-without-truncation ".repeat(20);
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "message.agent",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "text": long_text
                }),
                2,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let html = render_html(&tree);

        assert!(html.contains(
            "assistant-message-without-truncation assistant-message-without-truncation assistant-message-without-truncation"
        ));
        assert!(html.contains("class=\"message-collapse\""));
        assert!(html.contains("class=\"summary-text message-preview\""));
        assert!(html.contains("onclick=\"toggleMessageBlock(this)\""));
        assert!(html.contains(">see full<"));
        assert!(html.contains("..."));
    }

    #[test]
    fn render_html_collapses_large_shell_result_output_with_toggle() {
        let long_output = "shell-output-line\n".repeat(40);
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "shell.result",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"command_execution",
                    "tool_use_id":"cmd-1",
                    "input":{"command":"printf 'shell-output-line\\n'"},
                    "exit_code":0,
                    "output": long_output
                }),
                2,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let html = render_html(&tree);

        assert!(html.contains("class=\"inset-block shell-block inline-title\""));
        assert!(html.contains("class=\"inset-block-title inline-title\">Shell<"));
        assert!(html.contains("$ printf &#39;shell-output-line\\n&#39;"));
        assert!(html.contains("class=\"summary-text message-preview\""));
        assert!(html.contains("onclick=\"toggleMessageBlock(this)\""));
        assert!(html.contains(">see full<"));
        assert!(html.contains("shell-output-line"));
        assert!(!html.contains(
            "<span class=\"event-key\">summary</span><div class=\"summary-text\">command ok"
        ));
    }

    #[test]
    fn render_html_includes_command_aggregated_output_block() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "shell.result",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"command_execution",
                    "tool_use_id":"cmd-1",
                    "input":{"command":"printf 'a\\nb\\n'"},
                    "exit_code":0,
                    "output":"a\nb\n"
                }),
                2,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let html = render_html(&tree);

        assert!(html.contains("class=\"inset-block shell-block inline-title\""));
        assert!(html.contains("class=\"inset-block-title inline-title\">Shell<"));
        assert!(html.contains("$ printf &#39;a\\nb\\n&#39;"));
        assert!(html.contains("a\nb"));
        assert!(html.contains("&#10003; Успех"));
        assert!(!html.contains(
            "<span class=\"event-key\">summary</span><div class=\"summary-text\">command ok"
        ));
    }

    #[test]
    fn render_html_shows_shell_block_without_output() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "shell.result",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"command_execution",
                    "tool_use_id":"cmd-1",
                    "input":{"command":"git diff --stat"},
                    "exit_code":0,
                    "output":""
                }),
                2,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let html = render_html(&tree);

        assert!(html.contains("class=\"inset-block shell-block inline-title\""));
        assert!(html.contains("$ git diff --stat"));
        assert!(html.contains("Нет вывода"));
        assert!(html.contains("&#10003; Успех"));
    }

    #[test]
    fn render_html_collapses_shell_call_and_result_into_single_card() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "shell.call",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"command_execution",
                    "tool_use_id":"cmd-1"
                }),
                2,
            ),
            make_event(
                "shell.result",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"command_execution",
                    "tool_use_id":"cmd-1",
                    "input":{"command":"printf 'merged\\n'"},
                    "exit_code":0,
                    "output":"merged\n"
                }),
                3,
            ),
            make_event(
                "message.agent",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "text":"after shell"
                }),
                4,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let html = render_html(&tree);

        assert!(html.contains("data-seq=\"2,3\""));
        assert!(html.contains("data-event-ids=\"run-1:2,run-1:3\""));
        assert!(html.contains(">#0002, #0003<"));
        assert!(html.contains("event-meta-label\">output size<"));
        assert!(html.contains("event-meta-value\">6 B<"));
        assert!(html.contains("<strong>$ printf &#39;merged\\n&#39;</strong>"));
        assert!(!html.contains("data-seq=\"3\""));
        assert!(!html.contains("data-event-id=\"run-1:3\""));
        assert_eq!(count_occurrences(&html, "class=\"event-card\""), 3);

        let shell_pos = html
            .find("data-seq=\"2,3\"")
            .expect("collapsed shell card should be rendered");
        let message_pos = html
            .find("data-seq=\"4\"")
            .expect("later message should be rendered");
        assert!(shell_pos < message_pos);
    }

    #[test]
    fn render_html_hides_response_item_function_call_output_when_attached_shell_result_exists() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "shell.call",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"command_execution",
                    "tool_use_id":"cmd-1",
                    "input":{"command":"printf 'rich\\n'"}
                }),
                2,
            ),
            make_raw_event(
                "shell.result",
                "response_item",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"command_execution",
                    "tool_use_id":"cmd-1",
                    "output":{"exit_code":0}
                }),
                3,
            ),
            make_raw_event(
                "shell.result",
                "event_msg",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"command_execution",
                    "tool_use_id":"cmd-1",
                    "duplicate_of":"response_item.function_call_output",
                    "input":{"command":"printf 'rich\\n'"},
                    "exit_code":0,
                    "output":"rich\n"
                }),
                4,
            ),
            make_event(
                "message.agent",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "text":"after rich shell"
                }),
                5,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let html = render_html(&tree);

        assert!(html.contains("data-seq=\"2,4\""));
        assert!(html.contains(">#0002, #0004<"));
        assert!(html.contains("<strong>$ printf &#39;rich\\n&#39;</strong>"));
        assert!(html.contains("class=\"summary-text shell-block-output\">rich"));
        assert!(!html.contains("data-seq=\"3\""));
        assert!(!html.contains("data-event-id=\"run-1:3\""));
        assert_eq!(
            count_occurrences(&html, "class=\"inset-block shell-block inline-title\""),
            1
        );
    }

    #[test]
    fn render_html_collapses_spawn_agent_call_and_result_into_single_card() {
        let long_prompt =
            "Review this implementation plan against the user request and repository rules.\n"
                .repeat(8);
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "collab.spawn_agent",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"spawn_agent",
                    "tool_use_id":"spawn-1",
                    "phase":"started",
                    "input":{"agent_type":"reviewer","message":long_prompt,"model":"gpt-5.3-codex","reasoning_effort":"high"},
                    "prompt":"Review this implementation plan against the user request and repository rules.\n".repeat(8),
                    "requested_agent_type":"reviewer",
                    "model":"gpt-5.3-codex",
                    "reasoning_effort":"high"
                }),
                2,
            ),
            make_raw_event(
                "collab.spawn_agent",
                "event_msg",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"spawn_agent",
                    "tool_use_id":"spawn-1",
                    "phase":"completed",
                    "status":"pending_init",
                    "receiver_thread_ids":["sub-1"],
                    "prompt":"Review this implementation plan against the user request and repository rules.\n".repeat(8),
                    "agents_states":{"sub-1":{"status":"pending_init","agent_nickname":"Halley","agent_role":"reviewer","model":"gpt-5.3-codex","reasoning_effort":"high"}},
                    "new_thread_id":"sub-1",
                    "new_agent_nickname":"Halley",
                    "new_agent_role":"reviewer",
                    "model":"gpt-5.3-codex",
                    "reasoning_effort":"high",
                    "duplicate_of":"response_item.function_call_output"
                }),
                3,
            ),
            make_raw_event(
                "collab.spawn_agent",
                "response_item",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"spawn_agent",
                    "tool_use_id":"spawn-1",
                    "phase":"completed",
                    "output":{"agent_id":"sub-1","nickname":"Halley"},
                    "new_thread_id":"sub-1",
                    "new_agent_nickname":"Halley",
                    "receiver_thread_ids":["sub-1"],
                    "agents_states":{"sub-1":{"agent_nickname":"Halley"}}
                }),
                4,
            ),
            make_event(
                "agent.session",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "agent_role":"reviewer",
                    "agent_nickname":"Halley"
                }),
                5,
            ),
            make_event(
                "message.agent",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "text":"after spawn"
                }),
                6,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let html = render_html(&tree);

        assert!(html.contains("data-seq=\"2,3\""));
        assert!(html.contains("data-event-ids=\"run-1:2,run-1:3\""));
        assert!(html.contains(">#0002, #0003<"));
        assert!(html.contains("class=\"inset-block spawn-agent-block inline-title\""));
        assert!(html.contains("class=\"inset-block-title inline-title\">Spawn Agent<"));
        assert!(html.contains("event-meta-label\">agent type<"));
        assert!(html.contains("event-meta-value\">reviewer<"));
        assert!(html.contains("event-meta-label\">model<"));
        assert!(html.contains("event-meta-value\">gpt-5.3-codex<"));
        assert!(html.contains("event-meta-label\">effort<"));
        assert!(html.contains("event-meta-value\">high<"));
        assert!(html.contains("event-meta-label\">status<"));
        assert!(html.contains("event-meta-value\">pending_init<"));
        assert!(html.contains("class=\"message-collapse\""));
        assert!(html.contains("onclick=\"toggleMessageBlock(this)\""));
        assert!(html.contains(">see full<"));
        assert!(html.contains("class=\"kv-table-label\">thread id<"));
        assert!(html.contains("class=\"kv-table-value\"><code>sub-1</code>"));
        assert!(html.contains("class=\"kv-table-label\">nickname<"));
        assert!(html.contains("class=\"kv-table-value\"><code>Halley</code>"));
        assert!(html.contains("class=\"kv-table-label\">role<"));
        assert!(html.contains("class=\"kv-table-value\"><code>reviewer</code>"));
        assert!(!html.contains("data-seq=\"3\""));
        assert!(!html.contains("data-seq=\"4\""));
        assert!(!html.contains("data-event-id=\"run-1:4\""));

        let spawn_pos = html
            .find("data-seq=\"2,3\"")
            .expect("collapsed spawn card should be rendered");
        let message_pos = html
            .find("data-seq=\"6\"")
            .expect("later message should be rendered");
        assert!(spawn_pos < message_pos);
    }

    #[test]
    fn render_html_collapses_close_agent_call_and_result_into_single_card() {
        let status_text =
            "- Scope issues\n  - План расширяет scope.\n\n- Missing verification\n  - Нет теста.";
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "collab.close_agent",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"close_agent",
                    "tool_use_id":"close-1",
                    "phase":"started",
                    "input":{"target":"sub-1"},
                    "receiver_thread_ids":["sub-1"]
                }),
                2,
            ),
            make_raw_event(
                "collab.close_agent",
                "response_item",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"close_agent",
                    "tool_use_id":"close-1",
                    "phase":"completed",
                    "output":{"previous_status":{"completed":status_text}}
                }),
                3,
            ),
            make_raw_event(
                "collab.close_agent",
                "event_msg",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"close_agent",
                    "tool_use_id":"close-1",
                    "phase":"completed",
                    "status":"completed",
                    "receiver_thread_ids":["sub-1"],
                    "duplicate_of":"response_item.function_call_output",
                    "output":{
                        "type":"collab_close_end",
                        "receiver_thread_id":"sub-1",
                        "status":{"completed":status_text}
                    }
                }),
                4,
            ),
            make_event(
                "message.agent",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "text":"after close"
                }),
                5,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let html = render_html(&tree);

        assert!(html.contains("data-seq=\"2,4\""));
        assert!(html.contains("data-event-ids=\"run-1:2,run-1:4\""));
        assert!(html.contains(">#0002, #0004<"));
        assert!(html.contains("event-meta-label\">agents<"));
        assert!(html.contains("event-meta-value\">1<"));
        assert!(html.contains("event-meta-label\">updates<"));
        assert!(html.contains("class=\"inset-block user-input-block inline-title\""));
        assert!(html.contains("class=\"inset-block-title inline-title\">Subagent Close<"));
        assert!(html.contains("class=\"user-input-question-tag-label\">thread<"));
        assert!(html.contains("<code>sub-1</code>"));
        assert!(html.contains("class=\"user-input-question-tag-label\">status<"));
        assert!(html.contains(">completed<"));
        assert!(html.contains("Scope issues"));
        assert!(html.contains("Missing verification"));
        assert!(!html.contains("data-seq=\"3\""));
        assert!(!html.contains("data-event-id=\"run-1:3\""));

        let close_pos = html
            .find("data-seq=\"2,4\"")
            .expect("collapsed close card should be rendered");
        let message_pos = html
            .find("data-seq=\"5\"")
            .expect("later message should be rendered");
        assert!(close_pos < message_pos);
    }

    #[test]
    fn render_html_inserts_child_subtree_after_creation_step() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "collab.spawn_agent",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"spawn_agent",
                    "phase":"completed",
                    "status":"completed",
                    "receiver_thread_ids":["sub-1"],
                    "agents_states":{"sub-1":{"status":"pending_init"}}
                }),
                2,
            ),
            make_event(
                "agent.session",
                json!({
                    "actor_type":"subagent",
                    "thread_id":"sub-1",
                    "parent_thread_id":"root-thread",
                    "agent_role":"default",
                    "agent_nickname":"Hegel"
                }),
                3,
            ),
            make_event(
                "message.agent",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "text":"root continues"
                }),
                10,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let html = render_html(&tree);

        let spawn_pos = html
            .find("data-seq=\"2\"")
            .expect("spawn event should be rendered");
        let child_pos = html
            .find("data-thread-id=\"sub-1\"")
            .expect("child subtree should be rendered");
        let root_continue_pos = html
            .find("data-seq=\"10\"")
            .expect("later root event should be rendered");

        assert!(spawn_pos < child_pos);
        assert!(child_pos < root_continue_pos);
    }

    #[test]
    fn build_tree_keeps_agent_message_top_level_before_render() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "shell.result",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "tool_name":"command_execution"
                }),
                2,
            ),
            make_event(
                "message.agent",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "text":"command summary"
                }),
                3,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let root = &tree.roots[0];
        let command_result = match &root.items[1] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected event node"),
        };
        assert!(command_result.children.is_empty());

        let message_event = match &root.items[2] {
            TimelineItem::Event(node) => node,
            TimelineItem::Thread(_) => panic!("expected top-level message event"),
        };

        assert_eq!(message_event.event.parent_event_id.as_deref(), None);
    }

    #[test]
    fn raw_run_loader_replays_run_for_events_tree_html() {
        let run_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
            "target/manual-smoke/.codex-worker/tasks/all-operation-emulation--07f4203b/runs/20260406T145552Z--18a3cc56f6037e36-1",
        );
        let events_path = run_dir.join("events.jsonl");

        let (source_path, events) =
            load_records_from_run_input(&events_path).expect("tree input should load");

        assert_eq!(source_path, run_dir);
        assert_common_event_counts(
            &event_counts(&events),
            &[
                ("message.agent", 9),
                ("agent.completed", 1),
                ("agent.started", 1),
                ("file.change", 4),
                ("stderr.line", 1),
                ("thread.started", 1),
                ("todo.update", 4),
                ("web.search", 32),
                ("mcp.call", 3),
                ("mcp.result", 3),
            ],
            7,
            7,
        );
    }

    #[test]
    fn default_output_path_uses_run_dir_when_source_is_directory() {
        let tmp = tempdir().expect("temp dir should exist");
        let path = tmp.path();
        assert_eq!(default_output_path(path), path.join("events.tree.html"));
    }

    #[test]
    fn run_rejects_non_run_non_rollout_input_with_explicit_invalid_input() {
        let tmp = tempdir().expect("temp dir should exist");
        let cli = Cli {
            input_path: tmp.path().join("events.jsonl"),
            output_file: Some(tmp.path().join("out.html")),
            text_limit: 180,
            no_open: true,
        };

        let err = run(cli).expect_err("invalid path should be rejected before loading");
        assert!(err.to_string().contains("invalid input"));
    }

    #[test]
    fn run_accepts_rollout_jsonl_as_standalone_candidate() {
        let tmp = tempdir().expect("temp dir should exist");
        let input = tmp.path().join(rollout_root_name("sub1"));
        fs::write(
            &input,
            "{\"type\":\"session_meta\",\"schema_version\":1,\"ts\":\"2026-04-06T08:47:59Z\",\"task_id\":\"smoke-run\",\"run_id\":\"run-1\",\"seq\":1,\"event_type\":\"thread.started\",\"raw_type\":\"thread.started\",\"parse_status\":\"parsed\",\"payload\":{\"id\":\"sub1\",\"thread_id\":\"root-thread\"}}\n",
        )
        .expect("rollout jsonl should be written");
        let output = tmp.path().join("out.html");
        let cli = Cli {
            input_path: input,
            output_file: Some(output.clone()),
            text_limit: 180,
            no_open: true,
        };

        run(cli).expect("rollout input should be accepted");
        assert!(output.is_file());
    }

    #[test]
    fn run_rejects_standalone_rollout_when_first_line_is_not_session_meta() {
        let tmp = tempdir().expect("temp dir should exist");
        let input = tmp.path().join(rollout_root_name("sub1"));
        fs::write(
            &input,
            "{\"schema_version\":1,\"ts\":\"2026-04-06T08:47:59Z\",\"task_id\":\"smoke-run\",\"run_id\":\"run-1\",\"seq\":1,\"event_type\":\"thread.started\",\"raw_type\":\"thread.started\",\"parse_status\":\"parsed\",\"payload\":{\"thread_id\":\"root-thread\"}}\n",
        )
        .expect("rollout jsonl should be written");
        let cli = Cli {
            input_path: input,
            output_file: Some(tmp.path().join("out.html")),
            text_limit: 180,
            no_open: true,
        };

        let err = run(cli).expect_err("standalone rollout must fail on invalid root line");
        let message = err.to_string();
        assert!(message.contains("standalone rollout"));
        assert!(!message.contains("invalid input"));
    }

    #[test]
    fn run_rejects_standalone_rollout_when_session_id_mismatches_filename_suffix() {
        let tmp = tempdir().expect("temp dir should exist");
        let input = tmp
            .path()
            .join(rollout_root_name("019d645c-816c-7761-a34e-9db1ca764618"));
        fs::write(
            &input,
            "{\"type\":\"session_meta\",\"schema_version\":1,\"ts\":\"2026-04-06T08:47:59Z\",\"task_id\":\"smoke-run\",\"run_id\":\"run-1\",\"seq\":1,\"event_type\":\"thread.started\",\"raw_type\":\"thread.started\",\"parse_status\":\"parsed\",\"payload\":{\"id\":\"sub2\",\"thread_id\":\"root-thread\"}}\n",
        )
        .expect("rollout jsonl should be written");
        let cli = Cli {
            input_path: input,
            output_file: Some(tmp.path().join("out.html")),
            text_limit: 180,
            no_open: true,
        };

        let err = run(cli).expect_err("standalone rollout must fail on mismatched id");
        let message = err.to_string();
        assert!(message.contains("не совпадает"));
        assert!(!message.contains("invalid input"));
    }

    #[test]
    fn run_rejects_standalone_rollout_with_empty_filename_session_id_suffix() {
        let tmp = tempdir().expect("temp dir should exist");
        let input = tmp.path().join("rollout-2026-04-06T22-54-37-.jsonl");
        fs::write(
            &input,
            "{\"type\":\"session_meta\",\"payload\":{\"id\":\"sub1\"}}\n",
        )
        .expect("rollout jsonl should be written");
        let cli = Cli {
            input_path: input,
            output_file: Some(tmp.path().join("out.html")),
            text_limit: 180,
            no_open: true,
        };

        let err = run(cli).expect_err("empty suffix after last '-' must fail");
        let message = err.to_string();
        assert!(message.contains("session-id"));
        assert!(!message.contains("invalid input"));
    }

    #[test]
    fn run_prefers_run_input_for_run_subagent_path_even_when_name_looks_like_rollout() {
        let run_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
            "target/manual-smoke/.codex-worker/tasks/all-operation-emulation--07f4203b/runs/20260406T145552Z--18a3cc56f6037e36-1",
        );
        let input = run_dir.join("subagents/rollout-shadow.jsonl");
        let tmp = tempdir().expect("temp dir should exist");
        let output = tmp.path().join("out.html");

        assert!(is_run_input(&input));
        assert!(is_rollout_jsonl_family(&input));

        let cli = Cli {
            input_path: input.clone(),
            output_file: Some(output.clone()),
            text_limit: 180,
            no_open: true,
        };

        run(cli).expect("run-path should win and replay run dir");
        assert!(output.is_file());

        let html = fs::read_to_string(&output).expect("rendered html should be readable");
        let run_source_marker = format!(
            "<div class=\"hero-source-path\"><code>{}</code></div>",
            super::escape_html(&run_dir.display().to_string())
        );
        let rollout_source_marker = format!(
            "<div class=\"hero-source-path\"><code>{}</code></div>",
            super::escape_html(&input.display().to_string())
        );

        assert!(
            html.contains(&run_source_marker),
            "expected run source marker in html: {run_source_marker}"
        );
        assert!(
            !html.contains(&rollout_source_marker),
            "standalone rollout source marker must be absent when run-path wins: {rollout_source_marker}"
        );
    }

    fn event_counts(events: &[EventRecord]) -> BTreeMap<String, u64> {
        let mut counts = BTreeMap::new();
        for event in events {
            *counts.entry(event.event_type.clone()).or_insert(0) += 1;
        }
        counts
    }

    fn assert_common_event_counts(
        actual: &BTreeMap<String, u64>,
        stable_counts: &[(&str, u64)],
        expected_call_total: u64,
        expected_result_total: u64,
    ) {
        for (event_type, expected) in stable_counts {
            assert_eq!(actual.get(*event_type).copied(), Some(*expected));
        }

        let total_calls = actual.get("tool.call").copied().unwrap_or(0)
            + actual.get("shell.call").copied().unwrap_or(0)
            + actual.get("mcp.call").copied().unwrap_or(0);
        let total_results = actual.get("tool.result").copied().unwrap_or(0)
            + actual.get("shell.result").copied().unwrap_or(0)
            + actual.get("mcp.result").copied().unwrap_or(0);

        assert_eq!(total_calls, expected_call_total);
        assert_eq!(total_results, expected_result_total);
    }
}
