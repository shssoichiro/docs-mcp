// Configuration management module
// This module will handle TOML configuration management and settings

pub mod interactive;
pub mod settings;

#[cfg(test)]
mod tests;

pub use interactive::{run_interactive_config, show_config};
pub use settings::{Config, ConfigError, OllamaConfig};
