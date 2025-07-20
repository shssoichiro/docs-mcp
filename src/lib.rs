use thiserror::Error;

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

pub mod config;
pub mod crawler;
pub mod database;
pub mod embeddings;
pub mod indexer;
pub mod mcp;
