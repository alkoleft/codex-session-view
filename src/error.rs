use std::path::PathBuf;

use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("cli parse error: {0}")]
    CliParse(String),

    #[error("invalid value for `{field}`: {reason}")]
    Validation { field: &'static str, reason: String },

    #[error("{0}")]
    Runner(String),

    #[error("not implemented yet: {0}")]
    NotImplemented(&'static str),

    #[error("path is empty: {0}")]
    EmptyPath(PathBuf),
}
