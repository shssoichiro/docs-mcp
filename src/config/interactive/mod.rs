#[cfg(test)]
mod tests;

use anyhow::{Context, Result};
use console::style;
use dialoguer::{Confirm, Input, Select};

use super::{Config, ConfigError, OllamaConfig};

#[inline]
pub fn run_interactive_config() -> Result<()> {
    eprintln!("{}", style("🔧 Docs MCP Configuration Setup").bold().cyan());
    eprintln!();

    let mut config = load_existing_config()?;

    eprintln!("{}", style("Ollama Configuration").bold().yellow());
    eprintln!("Configure your local Ollama instance for embedding generation.");
    eprintln!();

    configure_ollama(&mut config.ollama)?;

    eprintln!();
    eprintln!("{}", style("Testing configuration...").yellow());

    if test_ollama_connection(&config.ollama)? {
        eprintln!("{}", style("✓ Ollama connection successful!").green());
    } else {
        eprintln!(
            "{}",
            style("⚠ Warning: Could not connect to Ollama").yellow()
        );
        eprintln!("You can continue, but make sure Ollama is running before indexing.");
    }

    eprintln!();
    if Confirm::new()
        .with_prompt("Save configuration?")
        .default(true)
        .interact()?
    {
        config.save().context("Failed to save configuration")?;
        eprintln!("{}", style("✓ Configuration saved successfully!").green());

        let config_path = Config::config_file_path().context("Failed to get config file path")?;
        eprintln!(
            "Configuration saved to: {}",
            style(config_path.display()).cyan()
        );
    } else {
        eprintln!("Configuration not saved.");
    }

    Ok(())
}

#[inline]
pub fn show_config() -> Result<()> {
    let config = Config::load().context("Failed to load configuration")?;

    eprintln!("{}", style("📋 Current Configuration").bold().cyan());
    eprintln!();

    eprintln!("{}", style("Ollama Settings:").bold().yellow());
    eprintln!("  Host: {}", style(&config.ollama.host).cyan());
    eprintln!("  Port: {}", style(config.ollama.port).cyan());
    eprintln!("  Model: {}", style(&config.ollama.model).cyan());
    eprintln!("  Batch Size: {}", style(config.ollama.batch_size).cyan());

    eprintln!();
    match config.ollama_url() {
        Ok(url) => eprintln!("  Ollama URL: {}", style(url).cyan()),
        Err(e) => eprintln!("  Ollama URL: {} ({})", style("Invalid").red(), e),
    }

    let config_path = Config::config_file_path().context("Failed to get config file path")?;
    eprintln!();
    eprintln!("Config file: {}", style(config_path.display()).dim());

    Ok(())
}

fn load_existing_config() -> Result<Config> {
    Config::load().map_or_else(
        |_| {
            eprintln!(
                "{}",
                style("No existing configuration found. Using defaults.").yellow()
            );
            Ok(Config::default())
        },
        |config| {
            eprintln!("{}", style("Found existing configuration.").green());
            Ok(config)
        },
    )
}

fn configure_ollama(ollama: &mut OllamaConfig) -> Result<()> {
    let protocols = &["http", "https"];
    let default_index = protocols
        .iter()
        .position(|&p| p == ollama.protocol)
        .unwrap_or(0);

    let protocol_index = Select::new()
        .with_prompt("Ollama protocol")
        .default(default_index)
        .items(protocols)
        .interact()?;

    let protocol = protocols[protocol_index].to_string();

    let host: String = Input::new()
        .with_prompt("Ollama host")
        .default(ollama.host.clone())
        .validate_with(|input: &String| -> Result<(), ConfigError> {
            let temp_config = OllamaConfig {
                protocol: protocol.clone(),
                host: input.clone(),
                port: 11434, // Use default port for validation
                model: "test".to_string(),
                batch_size: 32,
            };
            temp_config.validate()?;
            Ok(())
        })
        .interact_text()?;

    let port: u16 = Input::new()
        .with_prompt("Ollama port")
        .default(ollama.port)
        .validate_with(|input: &u16| -> Result<(), &str> {
            if *input == 0 {
                Err("Port must be greater than 0")
            } else {
                Ok(())
            }
        })
        .interact_text()?;

    let model: String = Input::new()
        .with_prompt("Embedding model")
        .default(ollama.model.clone())
        .validate_with(|input: &String| -> Result<(), &str> {
            if input.trim().is_empty() {
                Err("Model name cannot be empty")
            } else {
                Ok(())
            }
        })
        .interact_text()?;

    let batch_size: u32 = Input::new()
        .with_prompt("Batch size for embedding generation")
        .default(ollama.batch_size)
        .validate_with(|input: &u32| -> Result<(), &str> {
            if *input == 0 {
                Err("Batch size must be greater than 0")
            } else if *input > 1000 {
                Err("Batch size must be 1000 or less")
            } else {
                Ok(())
            }
        })
        .interact_text()?;

    ollama.set_protocol(protocol)?;
    ollama.set_host(host)?;
    ollama.set_port(port)?;
    ollama.set_model(model)?;
    ollama.set_batch_size(batch_size)?;

    Ok(())
}

fn test_ollama_connection(ollama: &OllamaConfig) -> Result<bool> {
    let url = format!(
        "{}://{}:{}/api/version",
        ollama.protocol, ollama.host, ollama.port
    );

    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(5)))
        .build()
        .into();

    match agent.get(&url).call() {
        Ok(_) => Ok(true),
        Err(ureq::Error::StatusCode(code)) if (400..500).contains(&code) => Ok(true),
        Err(_) => Ok(false),
    }
}
