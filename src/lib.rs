pub mod cache;
pub mod config;
pub mod executor;
pub mod llm;
pub mod parser;
pub mod shell;
pub mod tui;

pub use config::Config;
pub use parser::CommandSpec;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum QuocliError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Cache error: {0}")]
    Cache(#[from] sqlx::Error),

    #[error("LLM error: {0}")]
    Llm(String),

    #[error("Parser error: {0}")]
    Parser(String),

    #[error("Execution error: {0}")]
    Execution(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Command not found: {0}")]
    CommandNotFound(String),

    #[error("Help text not available for: {0}")]
    NoHelpText(String),
}

pub type Result<T> = std::result::Result<T, QuocliError>;
