use clap::{Parser, Subcommand};
use docs_mcp::Result;
use docs_mcp::commands::{add_site, delete_site, list_sites, serve_mcp, show_status, update_site};
use docs_mcp::config::{Config, run_interactive_config, show_config};
use docs_mcp::indexer::Indexer;

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
        /// Index URL of the documentation site
        url: String,
        /// Optional name for the site
        #[arg(long)]
        name: Option<String>,
        /// Optional version number for the site, e.g. "16" or "16.0.2" for React 16
        #[arg(long)]
        version: Option<String>,
        /// Override the base URL of the documentation site. Useful for cases where the index URL has additional paths.
        #[arg(long)]
        base_url: Option<String>,
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
    /// Start MCP server on stdio
    Serve,
    /// Show detailed status of the indexing pipeline
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Config { show } => {
            if show {
                show_config()?;
            } else {
                run_interactive_config()?;
            }
        }
        Commands::Add {
            url,
            name,
            base_url,
            version,
        } => {
            let base_url = base_url.as_deref().unwrap_or(url.as_str());
            let site = add_site(&url, name, version, base_url).await?;
            Indexer::new(Config::load()?)
                .await?
                .process_site_embeddings(&site)
                .await?;
        }
        Commands::List => {
            list_sites().await?;
        }
        Commands::Delete { site } => {
            delete_site(site).await?;
        }
        Commands::Update { site } => {
            let site = update_site(site).await?;
            Indexer::new(Config::load()?)
                .await?
                .process_site_embeddings(&site)
                .await?;
        }
        Commands::Serve => {
            serve_mcp().await?;
        }
        Commands::Status => {
            show_status().await?;
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
            if let Commands::Add { url, name, .. } = parsed.command {
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
            if let Commands::Add { url, name, .. } = parsed.command {
                assert_eq!(url, "https://example.com");
                assert_eq!(name, Some("Example Docs".to_string()));
            }
        }
    }

    #[test]
    fn serve_command() {
        let cli = Cli::try_parse_from(["docs-mcp", "serve"]);
        assert!(cli.is_ok());

        if let Ok(parsed) = cli {
            matches!(parsed.command, Commands::Serve);
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
