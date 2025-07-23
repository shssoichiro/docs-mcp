#[cfg(test)]
mod tests;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use thiserror::Error;
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    pub ollama: OllamaConfig,
    #[serde(skip)]
    pub base_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OllamaConfig {
    pub host: String,
    pub port: u16,
    pub model: String,
    pub batch_size: u32,
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
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("TOML parsing error: {0}")]
    TomlParse(#[from] toml::de::Error),
    #[error("TOML serialization error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),
}

impl Default for Config {
    #[inline]
    fn default() -> Self {
        Self {
            ollama: OllamaConfig {
                host: "localhost".to_string(),
                port: 11434,
                model: "nomic-embed-text:latest".to_string(),
                batch_size: 64,
            },
            base_dir: None,
        }
    }
}

impl Config {
    #[inline]
    pub fn config_dir() -> Result<PathBuf, ConfigError> {
        dirs::home_dir()
            .map(|home| home.join(".docs-mcp"))
            .or({
                #[cfg(windows)]
                {
                    dirs::data_dir().map(|data| data.join("docs-mcp"))
                }
                #[cfg(not(windows))]
                {
                    None
                }
            })
            .ok_or(ConfigError::DirectoryError)
    }

    /// Get the base directory for the application, using override if set
    #[inline]
    pub fn get_base_dir(&self) -> Result<PathBuf, ConfigError> {
        self.base_dir
            .as_ref()
            .map_or_else(Self::config_dir, |base_dir| Ok(base_dir.clone()))
    }

    #[inline]
    pub fn config_file_path() -> Result<PathBuf, ConfigError> {
        Ok(Self::config_dir()?.join("config.toml"))
    }

    #[inline]
    pub fn load() -> Result<Self> {
        let config_path =
            Self::config_file_path().context("Failed to determine config file path")?;

        if !config_path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config file: {}", config_path.display()))?;

        let config: Config = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", config_path.display()))?;

        config
            .validate()
            .with_context(|| "Configuration validation failed")?;

        Ok(config)
    }

    #[inline]
    pub fn save(&self) -> Result<()> {
        self.validate()
            .context("Configuration validation failed before saving")?;

        let config_dir = Self::config_dir().context("Failed to determine config directory")?;

        fs::create_dir_all(&config_dir).with_context(|| {
            format!(
                "Failed to create config directory: {}",
                config_dir.display()
            )
        })?;

        let config_path = config_dir.join("config.toml");
        let content = toml::to_string_pretty(self).context("Failed to serialize config to TOML")?;

        fs::write(&config_path, content)
            .with_context(|| format!("Failed to write config file: {}", config_path.display()))?;

        Ok(())
    }

    #[inline]
    pub fn validate(&self) -> Result<(), ConfigError> {
        self.ollama.validate()
    }

    #[inline]
    pub fn ollama_url(&self) -> Result<Url, ConfigError> {
        let url_str = format!("http://{}:{}", self.ollama.host, self.ollama.port);
        Url::parse(&url_str).map_err(|_| ConfigError::InvalidUrl(url_str))
    }

    /// Get the path for the SQLite database
    #[inline]
    pub fn database_path(&self) -> String {
        let base_dir = self.get_base_dir().unwrap_or_else(|_| PathBuf::from("."));
        base_dir.join("metadata.db").to_string_lossy().to_string()
    }

    /// Get the path for the vector database directory
    #[inline]
    pub fn vector_database_path(&self) -> String {
        let base_dir = self.get_base_dir().unwrap_or_else(|_| PathBuf::from("."));
        base_dir.join("vectors").to_string_lossy().to_string()
    }

    /// Get the config directory as an instance method
    #[inline]
    pub fn config_dir_path(&self) -> PathBuf {
        self.get_base_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
}

impl OllamaConfig {
    #[inline]
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.port == 0 {
            return Err(ConfigError::InvalidPort(self.port));
        }

        if self.model.trim().is_empty() {
            return Err(ConfigError::InvalidModel(self.model.clone()));
        }

        if self.batch_size == 0 || self.batch_size > 1000 {
            return Err(ConfigError::InvalidBatchSize(self.batch_size));
        }

        let url_str = format!("http://{}:{}", self.host, self.port);
        Url::parse(&url_str).map_err(|_| ConfigError::InvalidUrl(url_str))?;

        Ok(())
    }

    #[inline]
    pub fn set_host(&mut self, host: String) -> Result<(), ConfigError> {
        let temp_config = OllamaConfig {
            host: host.clone(),
            ..self.clone()
        };
        temp_config.validate()?;
        self.host = host;
        Ok(())
    }

    #[inline]
    pub fn set_port(&mut self, port: u16) -> Result<(), ConfigError> {
        if port == 0 {
            return Err(ConfigError::InvalidPort(port));
        }
        self.port = port;
        Ok(())
    }

    #[inline]
    pub fn set_model(&mut self, model: String) -> Result<(), ConfigError> {
        if model.trim().is_empty() {
            return Err(ConfigError::InvalidModel(model));
        }
        self.model = model;
        Ok(())
    }

    #[inline]
    pub fn set_batch_size(&mut self, batch_size: u32) -> Result<(), ConfigError> {
        if batch_size == 0 || batch_size > 1000 {
            return Err(ConfigError::InvalidBatchSize(batch_size));
        }
        self.batch_size = batch_size;
        Ok(())
    }
}
