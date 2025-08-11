use std::path::PathBuf;

use clap::{Parser, Subcommand};
use docs_mcp::Indexer;
use docs_mcp::{Config, ConfigError, run_interactive_config, show_config};
use docs_mcp::{DocsError, Result as DocsResult};
use docs_mcp::{add_site, delete_site, list_sites, serve_mcp, show_status, update_site};

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
        /// Output additional information during processing
        #[arg(long, short)]
        verbose: bool,
    },
    /// List all indexed documentation sites
    List {
        /// Output additional information
        #[arg(long, short)]
        verbose: bool,
    },
    /// Delete a documentation site
    Delete {
        /// Site ID or name to delete
        site: String,
    },
    /// Update/re-index a documentation site
    Update {
        /// Site ID or name to update
        site: String,
        /// Output additional information during processing
        #[arg(long, short)]
        verbose: bool,
    },
    /// Start MCP server on stdio
    Serve,
    /// Show detailed status of the indexing pipeline
    Status,
}

#[tokio::main]
async fn main() -> DocsResult<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    let config_dir = default_config_dir().map_err(|e| DocsError::Config(e.to_string()))?;
    let config = Config::load(&config_dir)?;

    match cli.command {
        Commands::Config { show } => {
            if show {
                show_config(&config)?;
            } else {
                run_interactive_config(config)?;
            }
        }
        Commands::Add {
            url,
            name,
            base_url,
            version,
            verbose,
        } => {
            let base_url = base_url.as_deref().unwrap_or(url.as_str());
            let site = add_site(&url, name, version, base_url, &config, verbose).await?;
            Indexer::new(config, verbose)
                .await?
                .process_site_embeddings(&site)
                .await?;
        }
        Commands::List { verbose } => {
            list_sites(&config, verbose).await?;
        }
        Commands::Delete { site } => {
            delete_site(site, &config).await?;
        }
        Commands::Update { site, verbose } => {
            let site = update_site(site, &config, verbose).await?;
            Indexer::new(config, verbose)
                .await?
                .process_site_embeddings(&site)
                .await?;
        }
        Commands::Serve => {
            serve_mcp(&config).await?;
        }
        Commands::Status => {
            show_status(&config).await?;
        }
    }

    Ok(())
}

fn default_config_dir() -> Result<PathBuf, ConfigError> {
    #[cfg(windows)]
    {
        dirs::data_dir()
            .map(|data| data.join("docs-mcp"))
            .ok_or(ConfigError::DirectoryError)
    }

    #[cfg(not(windows))]
    {
        dirs::home_dir()
            .map(|home| home.join(".docs-mcp"))
            .ok_or(ConfigError::DirectoryError)
    }
}
