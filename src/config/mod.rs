// Configuration management module
// This module will handle TOML configuration management and settings

#[cfg(test)]
mod tests;

pub mod interactive;
pub mod settings;

pub use self::interactive::{run_interactive_config, show_config};
pub use self::settings::{Config, ConfigError};
