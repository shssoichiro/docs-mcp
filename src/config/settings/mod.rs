#[cfg(test)]
mod tests;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use url::Url;

use crate::embeddings::chunking::ChunkingConfig;
use crate::embeddings::ollama::DEFAULT_EMBEDDING_DIMENSION;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub ollama: OllamaConfig,
    #[serde(default)]
    pub chunking: ChunkingConfig,
    #[serde(skip)]
    pub base_dir: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct OllamaConfig {
    pub protocol: String,
    pub host: String,
    pub port: u16,
    pub model: String,
    pub batch_size: u32,
    pub embedding_dimension: u32,
}

impl Default for OllamaConfig {
    fn default() -> Self {
        Self {
            protocol: "http".to_string(),
            host: "localhost".to_string(),
            port: 11434,
            model: "nomic-embed-text:latest".to_string(),
            batch_size: 16,
            embedding_dimension: DEFAULT_EMBEDDING_DIMENSION,
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Configuration directory not found or could not be created")]
    DirectoryError,
    #[error("Invalid URL format: {0}")]
    InvalidUrl(String),
    #[error("Invalid port: {0} (must be between 1 and 65535)")]
    InvalidPort(u16),
    #[error("Invalid batch size: {0} (must be between 1 and 1000)")]
    InvalidBatchSize(u32),
    #[error("Invalid model name: {0} (cannot be empty)")]
    InvalidModel(String),
    #[error("Invalid browser timeout: {0} (must be between 1 and 300 seconds)")]
    InvalidBrowserTimeout(u64),
    #[error("Invalid browser pool size: {0} (must be between 1 and 10)")]
    InvalidBrowserPoolSize(usize),
    #[error("Invalid window dimensions: {0}x{1} (must be between 100 and 4000)")]
    InvalidWindowDimensions(u32, u32),
    #[error("Invalid protocol: {0} (must be 'http' or 'https')")]
    InvalidProtocol(String),
    #[error("Invalid embedding dimension: {0} (must be between 64 and 4096)")]
    InvalidEmbeddingDimension(u32),
    #[error("Invalid target chunk size: {0} (must be between 100 and 2048)")]
    InvalidTargetChunkSize(usize),
    #[error("Invalid max chunk size: {0} (must be between 200 and 4096)")]
    InvalidMaxChunkSize(usize),
    #[error("Invalid min chunk size: {0} (must be between 50 and 1024)")]
    InvalidMinChunkSize(usize),
    #[error("Invalid overlap size: {0} (must be between 0 and 512)")]
    InvalidOverlapSize(usize),
    #[error("Max chunk size ({0}) must be greater than target chunk size ({1})")]
    MaxChunkSizeTooSmall(usize, usize),
    #[error("Target chunk size ({0}) must be greater than min chunk size ({1})")]
    TargetChunkSizeTooSmall(usize, usize),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parsing error: {0}")]
    TomlParse(#[from] toml::de::Error),
    #[error("TOML serialization error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),
}

impl Config {
    #[inline]
    pub fn load<P: AsRef<Path>>(config_dir: P) -> Result<Self> {
        let config_path = config_dir.as_ref().join("config.toml");

        if !config_path.exists() {
            return Ok(Self {
                ollama: OllamaConfig::default(),
                chunking: ChunkingConfig::default(),
                base_dir: config_dir.as_ref().to_path_buf(),
            });
        }

        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

        let mut config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", config_path.display()))?;
        config.base_dir = config_dir.as_ref().to_path_buf();

        config
            .validate()
            .with_context(|| "Configuration validation failed")?;

        Ok(config)
    }

    #[inline]
    pub fn save(&self) -> Result<()> {
        self.validate()
            .context("Configuration validation failed before saving")?;

        let config_dir = self.get_base_dir();

        fs::create_dir_all(config_dir).with_context(|| {
            format!(
                "Failed to create config directory: {}",
                config_dir.display()
            )
        })?;

        let config_path = self.config_file_path()?;
        let content = toml::to_string_pretty(self).context("Failed to serialize config to TOML")?;

        fs::write(&config_path, content)
            .with_context(|| format!("Failed to write config file: {}", config_path.display()))?;

        Ok(())
    }

    /// Get the base directory for the application
    #[inline]
    pub fn get_base_dir(&self) -> &Path {
        &self.base_dir
    }

    #[inline]
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.ollama.validate()?;
        self.validate_chunking_config()?;
        Ok(())
    }

    fn validate_chunking_config(&self) -> Result<(), ConfigError> {
        let config = &self.chunking;

        // Validate individual bounds
        if !(100..=2048).contains(&config.target_chunk_size) {
            return Err(ConfigError::InvalidTargetChunkSize(
                config.target_chunk_size,
            ));
        }

        if !(200..=4096).contains(&config.max_chunk_size) {
            return Err(ConfigError::InvalidMaxChunkSize(config.max_chunk_size));
        }

        if !(50..=1024).contains(&config.min_chunk_size) {
            return Err(ConfigError::InvalidMinChunkSize(config.min_chunk_size));
        }

        if config.overlap_size > 512 {
            return Err(ConfigError::InvalidOverlapSize(config.overlap_size));
        }

        // Validate relationships between sizes
        if config.max_chunk_size <= config.target_chunk_size {
            return Err(ConfigError::MaxChunkSizeTooSmall(
                config.max_chunk_size,
                config.target_chunk_size,
            ));
        }

        if config.target_chunk_size <= config.min_chunk_size {
            return Err(ConfigError::TargetChunkSizeTooSmall(
                config.target_chunk_size,
                config.min_chunk_size,
            ));
        }

        Ok(())
    }

    #[inline]
    pub fn config_file_path(&self) -> Result<PathBuf> {
        Ok(self.get_base_dir().join("config.toml"))
    }

    /// Get the path for the SQLite database
    #[inline]
    pub fn database_path(&self) -> Result<PathBuf> {
        Ok(self.get_base_dir().join("metadata.db"))
    }

    /// Get the path for the vector database directory
    #[inline]
    pub fn vector_database_path(&self) -> Result<PathBuf> {
        Ok(self.get_base_dir().join("vectors"))
    }

    /// Get the cache directory as an instance method
    #[inline]
    pub fn cache_dir_path(&self) -> Result<PathBuf> {
        Ok(self.get_base_dir().join("cache"))
    }
}

impl OllamaConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.protocol != "http" && self.protocol != "https" {
            return Err(ConfigError::InvalidProtocol(self.protocol.clone()));
        }

        let url_str = format!("{}://{}:{}", self.protocol, self.host, self.port);
        Url::parse(&url_str).map_err(|_| ConfigError::InvalidUrl(url_str))?;

        if self.port == 0 {
            return Err(ConfigError::InvalidPort(self.port));
        }

        if self.model.trim().is_empty() {
            return Err(ConfigError::InvalidModel(self.model.clone()));
        }

        if self.batch_size == 0 || self.batch_size > 1000 {
            return Err(ConfigError::InvalidBatchSize(self.batch_size));
        }

        if !(64..=4096).contains(&self.embedding_dimension) {
            return Err(ConfigError::InvalidEmbeddingDimension(
                self.embedding_dimension,
            ));
        }

        Ok(())
    }

    pub fn ollama_url(&self) -> Result<Url, ConfigError> {
        let url_str = format!("{}://{}:{}", self.protocol, self.host, self.port);
        Url::parse(&url_str).map_err(|_| ConfigError::InvalidUrl(url_str))
    }

    pub fn set_protocol(&mut self, protocol: String) -> Result<(), ConfigError> {
        if protocol != "http" && protocol != "https" {
            return Err(ConfigError::InvalidProtocol(protocol));
        }
        self.protocol = protocol;
        Ok(())
    }

    pub fn set_host(&mut self, host: String) -> Result<(), ConfigError> {
        let temp_config = OllamaConfig {
            host: host.clone(),
            ..self.clone()
        };
        temp_config.validate()?;
        self.host = host;
        Ok(())
    }

    pub fn set_port(&mut self, port: u16) -> Result<(), ConfigError> {
        if port == 0 {
            return Err(ConfigError::InvalidPort(port));
        }
        self.port = port;
        Ok(())
    }

    pub fn set_model(&mut self, model: String) -> Result<(), ConfigError> {
        if model.trim().is_empty() {
            return Err(ConfigError::InvalidModel(model));
        }
        self.model = model;
        Ok(())
    }

    pub fn set_batch_size(&mut self, batch_size: u32) -> Result<(), ConfigError> {
        if batch_size == 0 || batch_size > 1000 {
            return Err(ConfigError::InvalidBatchSize(batch_size));
        }
        self.batch_size = batch_size;
        Ok(())
    }

    pub fn set_embedding_dimension(&mut self, dimension: u32) -> Result<(), ConfigError> {
        if !(64..=4096).contains(&dimension) {
            return Err(ConfigError::InvalidEmbeddingDimension(dimension));
        }
        self.embedding_dimension = dimension;
        Ok(())
    }
}
