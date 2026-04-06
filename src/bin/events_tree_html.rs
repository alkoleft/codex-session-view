use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use clap::Parser;
use codex_worker_rs::error::{AppError, AppResult};
use codex_worker_rs::events::projector::EventSummaryCategory;
use codex_worker_rs::events::types::{INFO_TOKENS, SHELL_RESULT};

#[path = "events_tree_shared.rs"]
mod events_tree_shared;

use events_tree_shared::{
    build_event_tree, build_event_tree_with_standalone_startup_metadata, is_rollout_jsonl_family,
    is_run_input, load_records_from_run_input, load_records_from_standalone_rollout,
    validate_standalone_rollout_root, EventEntry, EventNode, EventTree, ThreadNode, TimelineItem,
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

fn render_html(tree: &EventTree) -> String {
    let mut out = String::new();
    out.push_str("<!doctype html><html lang=\"ru\"><head><meta charset=\"utf-8\">");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    out.push_str("<title>Events Tree</title><style>");
    out.push_str(
        "body{margin:0;font:14px/1.5 ui-monospace,SFMono-Regular,Menlo,Monaco,Consolas,monospace;background:#f4f1ea;color:#1f2937;}\
        .page{max-width:1440px;margin:0 auto;padding:24px;}\
        .hero{display:flex;flex-direction:column;gap:18px;background:linear-gradient(135deg,#fdf6e3,#e7f0ff);border:1px solid #d8dee9;border-radius:18px;padding:24px;box-shadow:0 12px 32px rgba(15,23,42,.08);}\
        .hero-head{display:flex;justify-content:space-between;align-items:flex-start;gap:18px;flex-wrap:wrap;}\
        .hero-title{display:flex;flex-direction:column;gap:6px;min-width:0;}\
        h1{margin:0;font-size:28px;line-height:1.2;}\
        .hero-lead{color:#475569;max-width:72ch;}\
        .hero-source{display:flex;flex-direction:column;gap:6px;min-width:min(420px,100%);max-width:100%;padding:12px 14px;border:1px solid #d8dee9;border-radius:16px;background:rgba(255,255,255,.72);}\
        .hero-source-label{font-size:11px;text-transform:uppercase;letter-spacing:.08em;font-weight:700;color:#64748b;}\
        .hero-source-path{white-space:pre-wrap;word-break:break-word;color:#334155;}\
        .hero-source-path code{background:none;border:none;padding:0;color:inherit;}\
        .hero-grid{display:grid;grid-template-columns:repeat(4,minmax(0,1fr));gap:12px;}\
        .hero-card{display:flex;flex-direction:column;gap:8px;min-width:0;padding:14px 16px;border:1px solid #d8dee9;border-radius:16px;background:rgba(255,255,255,.78);}\
        .hero-card-label{font-size:11px;text-transform:uppercase;letter-spacing:.08em;font-weight:700;color:#64748b;}\
        .hero-card-value{font-size:16px;font-weight:700;color:#0f172a;white-space:pre-wrap;word-break:break-word;}\
        .hero-card-value code{background:none;border:none;padding:0;color:inherit;font-size:inherit;}\
        .hero-metrics{display:grid;grid-template-columns:repeat(2,minmax(0,1fr));gap:10px;}\
        .hero-metric{display:flex;flex-direction:column;gap:2px;padding-top:4px;border-top:1px dashed #d8dee9;}\
        .hero-metric-label{font-size:11px;text-transform:uppercase;letter-spacing:.04em;color:#64748b;}\
        .hero-metric-value{font-size:22px;font-weight:800;color:#0f172a;line-height:1.1;}\
        .hero-startup{display:grid;grid-template-columns:repeat(auto-fit,minmax(220px,1fr));gap:10px;}\
        .hero-base{display:flex;flex-direction:column;gap:8px;}\
        .hero-base .inset-block{margin-top:0;}\
        .hero-startup-card{display:flex;flex-direction:column;gap:8px;min-width:0;padding:12px 14px;border:1px solid #d8dee9;border-radius:14px;background:rgba(255,255,255,.62);}\
        .hero-startup-value{color:#334155;white-space:pre-wrap;word-break:break-word;}\
        .hero-startup-value code{background:none;border:none;padding:0;color:inherit;font-size:inherit;}\
        .hero-startup-rows{display:flex;flex-direction:column;gap:8px;}\
        .hero-startup-row{display:flex;justify-content:space-between;align-items:flex-start;gap:12px;padding-top:8px;border-top:1px dashed #d8dee9;}\
        .hero-startup-row:first-child{padding-top:0;border-top:none;}\
        .hero-startup-row-label{font-size:11px;text-transform:uppercase;letter-spacing:.04em;color:#64748b;flex:0 0 auto;}\
        .hero-startup-row-value{min-width:0;text-align:right;color:#0f172a;white-space:pre-wrap;word-break:break-word;}\
        .hero-startup-row-value code{background:none;border:none;padding:0;color:inherit;font-size:inherit;}\
        .pill{display:inline-flex;align-items:center;gap:6px;padding:4px 10px;border-radius:999px;background:#fff;border:1px solid #d8dee9;color:#334155;}\
        .inset-block{position:relative;margin-top:8px;padding:18px 14px 12px;border:1px solid #d8dee9;border-radius:14px;background:rgba(255,255,255,.94);box-shadow:inset 0 1px 0 rgba(255,255,255,.85);}\
        .inset-block.inline-title{padding-top:14px;}\
        .inset-block-title{position:absolute;top:0;left:14px;transform:translateY(-50%);display:inline-flex;align-items:center;padding:3px 10px;border-radius:999px;background:#ece8df;border:1px solid #d8dee9;color:#475569;font-size:12px;font-weight:600;}\
        .inset-block-title.inline-title{display:block;position:static;transform:none;margin-bottom:14px;padding:0;border:none;border-radius:0;background:none;font-size:11px;text-transform:uppercase;letter-spacing:.08em;font-weight:700;color:#64748b;}\
        .inset-block-body{display:flex;flex-direction:column;gap:10px;min-width:0;}\
        .inset-block-footer{display:flex;justify-content:flex-end;align-items:center;margin-top:8px;font-size:12px;color:#64748b;}\
        .inset-block .message-collapse{width:100%;}\
        .shell-block-command{white-space:pre-wrap;word-break:break-word;font-size:15px;line-height:1.45;color:#0f172a;}\
        .shell-block-output{color:#475569;}\
        .shell-block-empty{color:#94a3b8;}\
        .shell-block-status{display:inline-flex;align-items:center;gap:6px;}\
        .shell-block-status.is-failure{color:#b91c1c;}\
        .tree{margin-top:24px;}\
        .children{margin:16px 0 0 28px;padding-left:18px;border-left:3px solid #d8dee9;}\
        details.thread{margin:14px 0;border:1px solid #d8dee9;border-radius:16px;background:#fff;box-shadow:0 10px 24px rgba(15,23,42,.05);}\
        details.thread[open]{background:#fffdf8;}\
        summary{cursor:pointer;list-style:none;padding:16px 18px;display:flex;flex-wrap:wrap;gap:10px 12px;align-items:center;}\
        summary::-webkit-details-marker{display:none;}\
        .thread-id{font-size:16px;font-weight:700;color:#0f172a;}\
        .thread-body{padding:0 18px 18px;}\
        .thread-flow{display:flex;flex-direction:column;gap:10px;margin-top:12px;}\
        .event-footnote{margin:8px calc(50% - 50vw) 0;padding:0 24px;background:linear-gradient(90deg,rgba(253,246,227,.96),rgba(231,240,255,.96));border-top:1px dashed #d8dee9;border-bottom:1px solid #d8dee9;}\
        .event-footnote-content{display:flex;flex-wrap:wrap;gap:10px 14px;align-items:baseline;padding:10px 0 12px;}\
        .event-footnote-seq{display:inline-flex;align-items:center;padding:2px 8px;border-radius:999px;background:#fff;border:1px solid #d8dee9;color:#334155;font-size:12px;}\
        .event-footnote-title{font-size:11px;text-transform:uppercase;letter-spacing:.08em;font-weight:700;color:#475569;}\
        .event-footnote-pair{display:inline-flex;align-items:baseline;gap:6px;}\
        .event-footnote-label{font-size:11px;text-transform:uppercase;letter-spacing:.04em;color:#64748b;}\
        .event-footnote-value{font-size:13px;font-weight:400;color:#0f172a;}\
        .event-footnote-diff{font-size:14px;font-weight:800;color:#64748b;}\
        .event-footnote-pair.is-total .event-footnote-label{font-size:11px;font-weight:700;}\
        .event-footnote-pair.is-total .event-footnote-value{font-size:13px;font-weight:400;}\
        .event-footnote-pair.is-total .event-footnote-diff{font-size:14px;font-weight:800;}\
        .diff-pos{color:#166534;}\
        .diff-neg{color:#b91c1c;}\
        .event-card{display:grid;grid-template-columns:88px 168px 170px 150px 110px;gap:10px;align-items:start;padding:12px 14px;border:1px solid #e5e7eb;border-radius:14px;background:#fff;box-shadow:0 6px 18px rgba(15,23,42,.04);}\
        .event-card.has-subagent{grid-template-columns:88px 168px 180px 170px 150px 110px;}\
        .event-cell{min-width:0;}\
        .event-key{display:block;font-size:11px;text-transform:uppercase;letter-spacing:.05em;color:#94a3b8;margin-bottom:4px;}\
        .seq-chip{display:inline-flex;align-items:center;padding:4px 8px;border-radius:999px;background:#0f172a;color:#fff;font-weight:700;}\
        .summary-text{white-space:pre-wrap;word-break:break-word;color:#0f172a;}\
        .message-collapse{display:flex;flex-direction:column;align-items:flex-start;gap:8px;}\
        .message-toggle{padding:5px 10px;border-radius:999px;border:1px solid #cbd5e1;background:#f8fafc;color:#334155;font:inherit;font-size:12px;cursor:pointer;}\
        .message-toggle:hover{background:#eef2f7;}\
        .message-collapse[data-expanded=\"false\"] .message-full{display:none;}\
        .message-collapse[data-expanded=\"true\"] .message-preview{display:none;}\
        .summary-structured{display:flex;flex-wrap:wrap;gap:8px 12px;align-items:baseline;color:#0f172a;}\
        .summary-title{font-weight:700;color:#334155;}\
        .summary-pair{display:inline-flex;align-items:baseline;gap:6px;}\
        .summary-label{font-size:12px;text-transform:uppercase;letter-spacing:.04em;color:#64748b;}\
        .summary-value{font-weight:700;color:#111827;}\
        .event-summary{grid-column:1 / -1;padding-top:8px;margin-top:2px;border-top:1px dashed #e5e7eb;}\
        .event-detail{grid-column:1 / -1;padding-top:8px;margin-top:2px;border-top:1px dashed #e5e7eb;}\
        .badge{display:inline-flex;align-items:center;padding:3px 8px;border-radius:999px;font-size:12px;border:1px solid transparent;}\
        .cat-default{background:#eef2f7;color:#334155;border-color:#d8dee9;}\
        .cat-assistant{background:#e8f7ec;color:#166534;border-color:#b7e4c7;}\
        .cat-command{background:#e6f0ff;color:#1d4ed8;border-color:#bfdbfe;}\
        .cat-search{background:#e6fffb;color:#0f766e;border-color:#99f6e4;}\
        .cat-subagent{background:#f8e8ff;color:#86198f;border-color:#f0abfc;}\
        .cat-file{background:#fff7d6;color:#92400e;border-color:#fde68a;}\
        .cat-todo{background:#ecfeff;color:#155e75;border-color:#a5f3fc;}\
        .cat-error{background:#fee2e2;color:#b91c1c;border-color:#fecaca;}\
        .empty{margin-top:10px;padding:14px;border:1px dashed #d8dee9;border-radius:12px;color:#64748b;background:#fafaf9;}\
        @media (max-width:1200px){.hero-grid{grid-template-columns:repeat(2,minmax(0,1fr));}.event-card{grid-template-columns:88px 140px 150px 130px 100px;}.event-card.has-subagent{grid-template-columns:88px 140px 160px 150px 130px 100px;}}\
        @media (max-width:980px){.hero-source{min-width:0;width:100%;}.event-footnote{padding:0 16px;}.event-card{grid-template-columns:1fr;}.event-key{margin-bottom:2px;}}\
        @media (max-width:640px){.page{padding:16px;}.hero{padding:18px;}.hero-grid{grid-template-columns:1fr;}}\
        code{background:#f8fafc;padding:2px 6px;border-radius:6px;border:1px solid #e2e8f0;}",
    );
    out.push_str("</style><script>");
    out.push_str(
        "function toggleMessageBlock(button){var block=button.closest('.message-collapse');if(!block){return;}var expanded=block.getAttribute('data-expanded')==='true';var nextState=expanded?'false':'true';block.setAttribute('data-expanded',nextState);button.setAttribute('aria-expanded',nextState);button.textContent=expanded?'see full':'collapse';}",
    );
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
    for item in items {
        match item {
            TimelineItem::Event(node) => render_event_node(out, node, depth, last_token_usage),
            TimelineItem::Thread(thread) => render_thread_html(out, thread, depth + 1),
        }
    }
    out.push_str("</div>");
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

fn render_event_node(
    out: &mut String,
    node: &EventNode,
    depth: usize,
    last_token_usage: &mut TokenUsage,
) {
    render_event_card(out, &node.event, last_token_usage);
    if !node.children.is_empty() {
        let _ = depth;
        out.push_str("<div class=\"children\">");
        render_timeline_items(out, &node.children, depth + 1, last_token_usage);
        out.push_str("</div>");
    }
}

const TEXT_COLLAPSE_CHAR_LIMIT: usize = 420;
const TEXT_COLLAPSE_LINE_LIMIT: usize = 6;

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
                    (entry_key.clone(), render_startup_metadata_value(entry_value))
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
        _ => serde_json::to_string_pretty(value)
            .unwrap_or_else(|_| "<invalid-json>".to_string()),
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
    matches_collapse_event_type(event)
        && should_collapse_text_content(&event.summary)
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
    let subagent_block = render_subagent_block(event);
    let event_card_class = if subagent_block.is_empty() {
        "event-card"
    } else {
        "event-card has-subagent"
    };
    let summary_cell = render_event_summary_cell(event);
    let aggregated_output_block = render_event_detail_block(event);
    let token_footnote = render_token_footnote(event, last_token_usage);
    let _ = write!(
        out,
        "<article class=\"{}\" data-seq=\"{}\" data-event-id=\"{}\" data-parent-event-id=\"{}\">\
         <div class=\"event-cell\"><span class=\"event-key\">seq</span><span class=\"seq-chip\">#{:04}</span></div>\
         <div class=\"event-cell\"><span class=\"event-key\">ts</span>{}</div>\
         {}\
         <div class=\"event-cell\"><span class=\"event-key\">event_type</span><span class=\"badge {}\">{}</span></div>\
         <div class=\"event-cell\"><span class=\"event-key\">raw_type</span>{}</div>\
         <div class=\"event-cell\"><span class=\"event-key\">parse</span>{}</div>\
         {}{}\
         </article>{}",
        event_card_class,
        event.seq,
        escape_html(&event.event_id),
        escape_html(event.parent_event_id.as_deref().unwrap_or("")),
        event.seq,
        escape_html(&event.ts),
        subagent_block,
        category_class(event.category),
        escape_html(&event.event_type),
        escape_html(&event.raw_type),
        escape_html(&event.parse_status),
        summary_cell,
        aggregated_output_block,
        token_footnote,
    );
}

fn render_event_summary_cell(event: &EventEntry) -> String {
    if event.event_type == SHELL_RESULT {
        return String::new();
    }

    format!(
        "<div class=\"event-cell event-summary\"><span class=\"event-key\">summary</span>{}</div>",
        render_summary_block(event)
    )
}

fn render_event_detail_block(event: &EventEntry) -> String {
    if let Some(shell_block) = render_shell_result_block(event) {
        return format!("<div class=\"event-cell event-detail\">{shell_block}</div>");
    }

    event.aggregated_output
        .as_deref()
        .map(|output| {
            format!(
                "<div class=\"event-cell event-detail\"><span class=\"event-key\">aggregated_output</span><div class=\"summary-text\">{}</div></div>",
                escape_html(output)
            )
        })
        .unwrap_or_default()
}

fn render_shell_result_block(event: &EventEntry) -> Option<String> {
    if event.event_type != SHELL_RESULT {
        return None;
    }
    if !matches!(
        event.tool_name.as_deref(),
        Some("command_execution" | "exec_command")
    ) {
        return None;
    }

    let command = event
        .shell_command
        .as_deref()
        .map(|command| format!("<div class=\"shell-block-command\">$ {}</div>", escape_html(command)))
        .unwrap_or_default();
    let output = match event.aggregated_output.as_deref() {
        Some(output) if should_collapse_text_content(output) => render_collapsible_text_block(output),
        Some(output) => format!("<div class=\"summary-text shell-block-output\">{}</div>", escape_html(output)),
        None => "<div class=\"summary-text shell-block-output shell-block-empty\">Нет вывода</div>".to_string(),
    };
    let footer = render_shell_result_status(event.shell_exit_code);
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
        Some(0) => "<span class=\"shell-block-status is-success\">&#10003; Успех</span>".to_string(),
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

fn render_subagent_block(event: &EventEntry) -> String {
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
        "<div class=\"event-cell\"><span class=\"event-key\">subagent</span><span class=\"badge cat-subagent\">{}</span></div>",
        escape_html(label)
    )
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

    fn make_event(event_type: &str, payload: serde_json::Value, seq: u64) -> EventRecord {
        EventRecord {
            schema_version: 1,
            ts: "2026-04-06T08:47:59Z".to_string(),
            task_id: "smoke-run".to_string(),
            run_id: "run-1".to_string(),
            seq,
            event_type: event_type.to_string(),
            raw_type: event_type.to_string(),
            parse_status: "parsed".to_string(),
            payload,
        }
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
        assert!(html.contains("class=\"hero-startup-card\"><span class=\"hero-card-label\">approval_policy<"));
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
        assert!(html.find("class=\"hero-base\"").expect("base instructions block should exist")
            < html.find("class=\"tree\"").expect("tree section should exist"));
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

        assert!(html.contains("<span class=\"event-key\">subagent</span>"));
        assert!(html.contains(">Lovelace<"));
        assert!(!html.contains("Lovelace [sub-1]"));
    }

    #[test]
    fn render_html_uses_message_role_prefix_when_present() {
        let events = vec![
            make_event("thread.started", json!({"thread_id":"root-thread"}), 1),
            make_event(
                "message.agent",
                json!({
                    "actor_type":"agent",
                    "thread_id":"root-thread",
                    "role":"system",
                    "text":"hello"
                }),
                2,
            ),
        ];

        let tree = build_event_tree(Path::new("/tmp/events.jsonl"), &events, 120);
        let html = render_html(&tree);

        assert!(html.contains("system: hello"));
        assert!(!html.contains("assistant: hello"));
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

        assert!(html.contains("summary-structured"));
        assert!(html.contains("summary-label\">input<"));
        assert!(html.contains("summary-label\">cached input<"));
        assert!(html.contains("summary-value\">11 877<"));
        assert!(html.contains("summary-value\">12 635<"));
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
            "assistant: assistant-message-without-truncation assistant-message-without-truncation assistant-message-without-truncation"
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
        assert!(!html.contains("<span class=\"event-key\">summary</span><div class=\"summary-text\">command ok"));
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
        assert!(!html.contains("<span class=\"event-key\">summary</span><div class=\"summary-text\">command ok"));
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
