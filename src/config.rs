use std::path::PathBuf;

use crate::error::{AppError, AppResult};
use crate::models::WorkerConfig;
use crate::util::{normalize_optional_path, normalize_path};

#[derive(Debug, Clone)]
pub struct RunNextConfigInput {
    pub task_file: PathBuf,
    pub codex_bin: String,
    pub codex_home: Option<PathBuf>,
    pub logs_dir: Option<PathBuf>,
    pub default_cwd: Option<PathBuf>,
    pub model: Option<String>,
    pub sandbox: Option<String>,
    pub approval_policy: Option<String>,
    pub prompt_template: Option<PathBuf>,
    pub poll_interval: f64,
    pub stale_after: f64,
    pub dry_run: bool,
    pub quiet: bool,
}

pub fn build_worker_config(input: RunNextConfigInput) -> AppResult<WorkerConfig> {
    if input.task_file.as_os_str().is_empty() {
        return Err(AppError::EmptyPath(input.task_file));
    }

    if input.codex_bin.trim().is_empty() {
        return Err(AppError::Validation {
            field: "codex-bin",
            reason: "must not be empty".to_string(),
        });
    }

    for (field, value) in [
        ("codex-home", input.codex_home.as_ref()),
        ("logs-dir", input.logs_dir.as_ref()),
        ("default-cwd", input.default_cwd.as_ref()),
        ("prompt-template", input.prompt_template.as_ref()),
    ] {
        if let Some(path) = value {
            if path.as_os_str().is_empty() {
                return Err(AppError::Validation {
                    field,
                    reason: "must not be empty".to_string(),
                });
            }
        }
    }

    if !(input.poll_interval.is_finite() && input.poll_interval > 0.0) {
        return Err(AppError::Validation {
            field: "poll-interval",
            reason: "must be > 0".to_string(),
        });
    }

    if !(input.stale_after.is_finite() && input.stale_after > 0.0) {
        return Err(AppError::Validation {
            field: "stale-after",
            reason: "must be > 0".to_string(),
        });
    }

    Ok(WorkerConfig {
        task_file: normalize_path(&input.task_file),
        codex_bin: input.codex_bin,
        codex_home: normalize_optional_path(input.codex_home),
        logs_dir: normalize_optional_path(input.logs_dir),
        default_cwd: normalize_optional_path(input.default_cwd),
        model: input.model,
        sandbox: input.sandbox,
        approval_policy: input.approval_policy,
        prompt_template: normalize_optional_path(input.prompt_template),
        poll_interval: input.poll_interval,
        stale_after: input.stale_after,
        dry_run: input.dry_run,
        log_to_stdout: !input.quiet,
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::error::AppError;

    use super::{build_worker_config, RunNextConfigInput};

    fn sample_input() -> RunNextConfigInput {
        RunNextConfigInput {
            task_file: PathBuf::from("./tasks.md"),
            codex_bin: "codex".to_string(),
            codex_home: None,
            logs_dir: None,
            default_cwd: None,
            model: None,
            sandbox: None,
            approval_policy: None,
            prompt_template: None,
            poll_interval: 2.0,
            stale_after: 30.0,
            dry_run: false,
            quiet: false,
        }
    }

    #[test]
    fn build_worker_config_expands_tilde_for_codex_home() {
        let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
            return;
        };

        let mut input = sample_input();
        input.codex_home = Some(PathBuf::from("~/codex"));
        let config = build_worker_config(input).expect("config should build");

        assert_eq!(config.codex_home, Some(home.join("codex")));
    }

    #[test]
    fn build_worker_config_preserves_model_and_quiet_flag() {
        let mut input = sample_input();
        input.model = Some("gpt-5".to_string());
        input.quiet = true;

        let config = build_worker_config(input).expect("config should build");
        assert_eq!(config.model.as_deref(), Some("gpt-5"));
        assert!(!config.log_to_stdout);
    }

    #[test]
    fn build_worker_config_rejects_empty_optional_paths() {
        let mut codex_home = sample_input();
        codex_home.codex_home = Some(PathBuf::new());
        let codex_home_err =
            build_worker_config(codex_home).expect_err("empty codex_home should fail");
        assert!(matches!(
            codex_home_err,
            AppError::Validation {
                field: "codex-home",
                ..
            }
        ));

        let mut logs_dir = sample_input();
        logs_dir.logs_dir = Some(PathBuf::new());
        let logs_dir_err = build_worker_config(logs_dir).expect_err("empty logs_dir should fail");
        assert!(matches!(
            logs_dir_err,
            AppError::Validation {
                field: "logs-dir",
                ..
            }
        ));

        let mut default_cwd = sample_input();
        default_cwd.default_cwd = Some(PathBuf::new());
        let default_cwd_err =
            build_worker_config(default_cwd).expect_err("empty default_cwd should fail");
        assert!(matches!(
            default_cwd_err,
            AppError::Validation {
                field: "default-cwd",
                ..
            }
        ));

        let mut prompt_template = sample_input();
        prompt_template.prompt_template = Some(PathBuf::new());
        let prompt_template_err =
            build_worker_config(prompt_template).expect_err("empty prompt_template should fail");
        assert!(matches!(
            prompt_template_err,
            AppError::Validation {
                field: "prompt-template",
                ..
            }
        ));
    }

    #[test]
    fn build_worker_config_rejects_empty_codex_bin() {
        let mut input = sample_input();
        input.codex_bin = String::new();

        let err = build_worker_config(input).expect_err("empty codex_bin should fail");
        assert!(matches!(
            err,
            AppError::Validation {
                field: "codex-bin",
                ..
            }
        ));
    }

    #[test]
    fn build_worker_config_rejects_whitespace_only_codex_bin() {
        let mut input = sample_input();
        input.codex_bin = "   \t  ".to_string();

        let err = build_worker_config(input).expect_err("whitespace-only codex_bin should fail");
        assert!(matches!(
            err,
            AppError::Validation {
                field: "codex-bin",
                ..
            }
        ));
    }

    #[test]
    fn build_worker_config_rejects_non_positive_poll_interval() {
        for value in [0.0, -1.0] {
            let mut input = sample_input();
            input.poll_interval = value;

            let err =
                build_worker_config(input).expect_err("non-positive poll_interval should fail");
            assert!(matches!(
                err,
                AppError::Validation {
                    field: "poll-interval",
                    ..
                }
            ));
        }
    }

    #[test]
    fn build_worker_config_rejects_non_positive_stale_after() {
        for value in [0.0, -1.0] {
            let mut input = sample_input();
            input.stale_after = value;

            let err = build_worker_config(input).expect_err("non-positive stale_after should fail");
            assert!(matches!(
                err,
                AppError::Validation {
                    field: "stale-after",
                    ..
                }
            ));
        }
    }

    #[test]
    fn build_worker_config_rejects_non_finite_intervals() {
        for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut poll_input = sample_input();
            poll_input.poll_interval = value;
            let poll_err =
                build_worker_config(poll_input).expect_err("non-finite poll_interval should fail");
            assert!(matches!(
                poll_err,
                AppError::Validation {
                    field: "poll-interval",
                    ..
                }
            ));

            let mut stale_input = sample_input();
            stale_input.stale_after = value;
            let stale_err =
                build_worker_config(stale_input).expect_err("non-finite stale_after should fail");
            assert!(matches!(
                stale_err,
                AppError::Validation {
                    field: "stale-after",
                    ..
                }
            ));
        }
    }
}
