use std::ffi::OsString;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::config::{build_worker_config, RunNextConfigInput};
use crate::error::{AppError, AppResult};
use crate::runner::CodexWorker;

#[derive(Debug, Parser)]
#[command(
    name = "codex-worker",
    bin_name = "codex-worker",
    version,
    about = "codex-worker Rust scaffold"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    RunNext(RunNextArgs),
}

#[derive(Debug, Args)]
pub struct RunNextArgs {
    #[arg(long)]
    pub task_file: PathBuf,

    #[arg(long, default_value = "codex")]
    pub codex_bin: String,

    #[arg(long)]
    pub codex_home: Option<PathBuf>,

    #[arg(long)]
    pub logs_dir: Option<PathBuf>,

    #[arg(long)]
    pub default_cwd: Option<PathBuf>,

    #[arg(long)]
    pub model: Option<String>,

    #[arg(long)]
    pub sandbox: Option<String>,

    #[arg(long)]
    pub approval_policy: Option<String>,

    #[arg(long)]
    pub prompt_template: Option<PathBuf>,

    #[arg(long, default_value_t = 2.0)]
    pub poll_interval: f64,

    #[arg(long, default_value_t = 30.0)]
    pub stale_after: f64,

    #[arg(long)]
    pub dry_run: bool,

    #[arg(long)]
    pub quiet: bool,
}

impl RunNextArgs {
    pub fn into_input(self) -> RunNextConfigInput {
        RunNextConfigInput {
            task_file: self.task_file,
            codex_bin: self.codex_bin,
            codex_home: self.codex_home,
            logs_dir: self.logs_dir,
            default_cwd: self.default_cwd,
            model: self.model,
            sandbox: self.sandbox,
            approval_policy: self.approval_policy,
            prompt_template: self.prompt_template,
            poll_interval: self.poll_interval,
            stale_after: self.stale_after,
            dry_run: self.dry_run,
            quiet: self.quiet,
        }
    }
}

pub fn parse_from<I, T>(args: I) -> AppResult<Cli>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    Cli::try_parse_from(args).map_err(|err| AppError::CliParse(err.to_string()))
}

pub fn main() -> i32 {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("codex-worker error: {err}");
            1
        }
    }
}

fn run(cli: Cli) -> AppResult<i32> {
    match cli.command {
        Command::RunNext(args) => {
            let config = build_worker_config(args.into_input())?;
            let mut worker = CodexWorker::new(config);
            worker.run_next()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_from, Cli};
    use clap::CommandFactory;

    #[test]
    fn parses_run_next_required_and_defaults() {
        let cli = parse_from(["codex-worker", "run-next", "--task-file", "./tasks.md"])
            .expect("cli should parse");

        let run_next = match cli.command {
            super::Command::RunNext(args) => args,
        };

        assert_eq!(run_next.codex_bin, "codex");
        assert_eq!(run_next.poll_interval, 2.0);
        assert_eq!(run_next.stale_after, 30.0);
        assert!(!run_next.dry_run);
        assert!(!run_next.quiet);
    }

    #[test]
    fn help_usage_uses_codex_worker_contract_name() {
        let usage = Cli::command().render_usage().to_string();
        assert!(
            usage.contains("codex-worker"),
            "usage should contain contract binary name, got: {usage}"
        );
    }
}
