use anyhow::{Context, Result};
use console::style;
use dialoguer::{Confirm, Input};

use super::{Config, ConfigError, OllamaConfig};

#[inline]
pub fn run_interactive_config() -> Result<()> {
    println!("{}", style("🔧 Docs MCP Configuration Setup").bold().cyan());
    println!();

    let mut config = load_existing_config()?;

    println!("{}", style("Ollama Configuration").bold().yellow());
    println!("Configure your local Ollama instance for embedding generation.");
    println!();

    configure_ollama(&mut config.ollama)?;

    println!();
    println!("{}", style("Testing configuration...").yellow());

    if test_ollama_connection(&config.ollama)? {
        println!("{}", style("✓ Ollama connection successful!").green());
    } else {
        println!(
            "{}",
            style("⚠ Warning: Could not connect to Ollama").yellow()
        );
        println!("You can continue, but make sure Ollama is running before indexing.");
    }

    println!();
    if Confirm::new()
        .with_prompt("Save configuration?")
        .default(true)
        .interact()?
    {
        config.save().context("Failed to save configuration")?;
        println!("{}", style("✓ Configuration saved successfully!").green());

        let config_path = Config::config_file_path().context("Failed to get config file path")?;
        println!(
            "Configuration saved to: {}",
            style(config_path.display()).cyan()
        );
    } else {
        println!("Configuration not saved.");
    }

    Ok(())
}

#[inline]
pub fn show_config() -> Result<()> {
    let config = Config::load().context("Failed to load configuration")?;

    println!("{}", style("📋 Current Configuration").bold().cyan());
    println!();

    println!("{}", style("Ollama Settings:").bold().yellow());
    println!("  Host: {}", style(&config.ollama.host).cyan());
    println!("  Port: {}", style(config.ollama.port).cyan());
    println!("  Model: {}", style(&config.ollama.model).cyan());
    println!("  Batch Size: {}", style(config.ollama.batch_size).cyan());

    println!();
    match config.ollama_url() {
        Ok(url) => println!("  Ollama URL: {}", style(url).cyan()),
        Err(e) => println!("  Ollama URL: {} ({})", style("Invalid").red(), e),
    }

    let config_path = Config::config_file_path().context("Failed to get config file path")?;
    println!();
    println!("Config file: {}", style(config_path.display()).dim());

    Ok(())
}

fn load_existing_config() -> Result<Config> {
    Config::load().map_or_else(
        |_| {
            println!(
                "{}",
                style("No existing configuration found. Using defaults.").yellow()
            );
            Ok(Config::default())
        },
        |config| {
            println!("{}", style("Found existing configuration.").green());
            Ok(config)
        },
    )
}

fn configure_ollama(ollama: &mut OllamaConfig) -> Result<()> {
    let host: String = Input::new()
        .with_prompt("Ollama host")
        .default(ollama.host.clone())
        .validate_with(|input: &String| -> Result<(), ConfigError> {
            let temp_config = OllamaConfig {
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

    ollama.set_host(host)?;
    ollama.set_port(port)?;
    ollama.set_model(model)?;
    ollama.set_batch_size(batch_size)?;

    Ok(())
}

fn test_ollama_connection(ollama: &OllamaConfig) -> Result<bool> {
    let url = format!("http://{}:{}/api/version", ollama.host, ollama.port);

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

#[cfg(test)]
mod tests {
    #[test]
    fn load_existing_config() {
        let config = super::load_existing_config().expect("config loaded successfully");
        assert!(!config.ollama.host.is_empty());
        assert!(config.ollama.port > 0);
        assert!(!config.ollama.model.is_empty());
        assert!(config.ollama.batch_size > 0);
    }
}
