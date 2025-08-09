pub(crate) mod commands;
pub(crate) mod config;
pub(crate) mod crawler;
pub(crate) mod database;
pub(crate) mod embeddings;
pub(crate) mod indexer;
pub(crate) mod mcp;
pub(crate) mod turndown;

use thiserror::Error;

pub use self::commands::{add_site, delete_site, list_sites, serve_mcp, show_status, update_site};
pub use self::config::{Config, ConfigError, run_interactive_config, show_config};
pub use self::indexer::Indexer;

pub type Result<T> = std::result::Result<T, DocsError>;

#[derive(Error, Debug)]
pub enum DocsError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Embedding error: {0}")]
    Embedding(String),

    #[error("Crawler error: {0}")]
    Crawler(String),

    #[error("MCP error: {0}")]
    Mcp(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Other error: {0}")]
    Other(#[from] anyhow::Error),
}
