use clap::{Parser, Subcommand};
use docs_mcp::Result;
use tracing::info;

#[derive(Parser)]
#[command(name = "docs-mcp")]
#[command(about = "A documentation indexing and search system with MCP server")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Configure Ollama connection and settings
    Config {
        /// Show current configuration
        #[arg(long)]
        show: bool,
    },
    /// Add a new documentation site to index
    Add {
        /// Base URL of the documentation site
        url: String,
        /// Optional name for the site
        #[arg(long)]
        name: Option<String>,
    },
    /// List all indexed documentation sites
    List,
    /// Delete a documentation site
    Delete {
        /// Site ID or name to delete
        site: String,
    },
    /// Update/re-index a documentation site
    Update {
        /// Site ID or name to update
        site: String,
    },
    /// Start MCP server and background indexer
    Serve {
        /// Port to run MCP server on
        #[arg(long, default_value = "3000")]
        port: u16,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Config { show } => {
            if show {
                info!("Configuration display not implemented yet");
            } else {
                info!("Interactive configuration setup not implemented yet");
            }
            println!("Configuration command not implemented yet");
        }
        Commands::Add { url, name } => {
            info!(
                "Add site command called with URL: {}, name: {:?}",
                url, name
            );
            println!("Add site command not implemented yet");
        }
        Commands::List => {
            info!("List sites command called");
            println!("List sites command not implemented yet");
        }
        Commands::Delete { site } => {
            info!("Delete site command called for: {}", site);
            println!("Delete site command not implemented yet");
        }
        Commands::Update { site } => {
            info!("Update site command called for: {}", site);
            println!("Update site command not implemented yet");
        }
        Commands::Serve { port } => {
            info!("Serve command called on port: {}", port);
            println!("MCP server not implemented yet");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    #[test]
    fn cli_parsing() {
        let cli = Cli::try_parse_from(["docs-mcp", "list"]);
        assert!(cli.is_ok());

        if let Ok(parsed) = cli {
            matches!(parsed.command, Commands::List);
        }
    }

    #[test]
    fn add_command_with_url() {
        let cli = Cli::try_parse_from(["docs-mcp", "add", "https://example.com"]);
        assert!(cli.is_ok());

        if let Ok(parsed) = cli {
            if let Commands::Add { url, name } = parsed.command {
                assert_eq!(url, "https://example.com");
                assert_eq!(name, None);
            }
        }
    }

    #[test]
    fn add_command_with_name() {
        let cli = Cli::try_parse_from([
            "docs-mcp",
            "add",
            "https://example.com",
            "--name",
            "Example Docs",
        ]);
        assert!(cli.is_ok());

        if let Ok(parsed) = cli {
            if let Commands::Add { url, name } = parsed.command {
                assert_eq!(url, "https://example.com");
                assert_eq!(name, Some("Example Docs".to_string()));
            }
        }
    }

    #[test]
    fn serve_command_default_port() {
        let cli = Cli::try_parse_from(["docs-mcp", "serve"]);
        assert!(cli.is_ok());

        if let Ok(parsed) = cli {
            if let Commands::Serve { port } = parsed.command {
                assert_eq!(port, 3000);
            }
        }
    }

    #[test]
    fn serve_command_custom_port() {
        let cli = Cli::try_parse_from(["docs-mcp", "serve", "--port", "8080"]);
        assert!(cli.is_ok());

        if let Ok(parsed) = cli {
            if let Commands::Serve { port } = parsed.command {
                assert_eq!(port, 8080);
            }
        }
    }

    #[test]
    fn config_show_flag() {
        let cli = Cli::try_parse_from(["docs-mcp", "config", "--show"]);
        assert!(cli.is_ok());

        if let Ok(parsed) = cli {
            if let Commands::Config { show } = parsed.command {
                assert!(show);
            }
        }
    }

    #[test]
    fn invalid_command() {
        let cli = Cli::try_parse_from(["docs-mcp", "invalid"]);
        assert!(cli.is_err());

        if let Err(err) = cli {
            assert_eq!(err.kind(), ErrorKind::InvalidSubcommand);
        }
    }

    #[test]
    fn help_message() {
        let cli = Cli::try_parse_from(["docs-mcp", "--help"]);
        assert!(cli.is_err());

        if let Err(err) = cli {
            assert_eq!(err.kind(), ErrorKind::DisplayHelp);
        }
    }
}
