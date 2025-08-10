use anyhow::{Context, Result, bail};
use modelcontextprotocol_server::ServerBuilder;
use modelcontextprotocol_server::transport::StdioTransport;
use serde_json::from_value;
use tokio::runtime::Handle;
use tokio::task::block_in_place;
use tracing::{error, info, warn};

use crate::config::Config;
use crate::crawler::{CrawlerConfig, SiteCrawler};
use crate::database::lancedb::vector_store::VectorStore;
use crate::database::sqlite::Database;
use crate::database::sqlite::models::{NewSite, Site, SiteStatus, SiteUpdate};
use crate::database::sqlite::queries::SiteQueries;
use crate::mcp::tools::{CallToolParams, ToolHandler};

/// Validation functions for CLI commands
pub mod validation {
    use anyhow::{Result, anyhow};
    use url::Url;

    /// Validate that a site identifier is either a valid ID or a non-empty name
    pub fn validate_site_identifier(identifier: &str) -> Result<()> {
        if identifier.trim().is_empty() {
            return Err(anyhow!("Site identifier cannot be empty"));
        }

        // If it's a number, validate it's positive
        if let Ok(id) = identifier.parse::<i64>() {
            if id <= 0 {
                return Err(anyhow!("Site ID must be a positive number, got: {}", id));
            }
        }

        Ok(())
    }

    /// Validate URL format and accessibility
    pub fn validate_documentation_url(url: &str) -> Result<Url> {
        if url.trim().is_empty() {
            return Err(anyhow!("URL cannot be empty"));
        }

        let parsed_url = Url::parse(url).map_err(|e| anyhow!("Invalid URL format: {}", e))?;

        // Must be HTTP or HTTPS
        if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
            return Err(anyhow!(
                "URL must use HTTP or HTTPS protocol, got: {}",
                parsed_url.scheme()
            ));
        }

        // Must have a host
        if parsed_url.host().is_none() {
            return Err(anyhow!("URL must have a valid host"));
        }

        Ok(parsed_url)
    }

    /// Validate site name format
    pub fn validate_site_name(name: &str) -> Result<()> {
        let name = name.trim();

        if name.is_empty() {
            return Err(anyhow!("Site name cannot be empty"));
        }

        if name.len() > 100 {
            return Err(anyhow!("Site name must be 100 characters or less"));
        }

        // Check for invalid characters that might cause issues
        if name.contains('\n') || name.contains('\r') || name.contains('\t') {
            return Err(anyhow!("Site name cannot contain newlines or tabs"));
        }

        Ok(())
    }

    /// Validate site version format
    ///
    /// No need to be strict, as this could be semver or it could be a git hash
    /// or possibly some arbitrary identifier like Jessie.
    pub fn validate_site_version(version: &str) -> Result<()> {
        let version = version.trim();

        if version.is_empty() {
            return Err(anyhow!("Site version cannot be empty"));
        }

        if version.len() > 40 {
            return Err(anyhow!("Site version must be 40 characters or less"));
        }

        // Check for invalid characters that might cause issues
        if version.contains('\n') || version.contains('\r') || version.contains('\t') {
            return Err(anyhow!("Site version cannot contain newlines or tabs"));
        }

        Ok(())
    }
}

/// Add a new documentation site for indexing with comprehensive progress display
#[inline]
pub async fn add_site(
    url: &str,
    name: Option<String>,
    version: Option<String>,
    base_url: &str,
    config: &Config,
    verbose: bool,
) -> Result<Site> {
    eprintln!("🚀 Adding new documentation site");
    eprintln!("   URL: {}", url);

    info!("Adding documentation site: {}", url);

    // Validate inputs
    eprint!("🔍 Validating inputs... ");
    use std::io::{self, Write};
    io::stdout().flush().context("Failed to flush stdout")?;

    let parsed_url = validation::validate_documentation_url(url).context("Invalid URL provided")?;

    if let Some(ref site_name) = name {
        validation::validate_site_name(site_name).context("Invalid site name provided")?;
    }
    if let Some(ref site_version) = version {
        validation::validate_site_version(site_version).context("Invalid site version provided")?;
    }

    eprintln!("✅");

    // Generate name if not provided
    let site_name = name.unwrap_or_else(|| {
        let host = parsed_url.host_str().unwrap_or("unknown");
        let path_segments: Vec<&str> = parsed_url
            .path_segments()
            .map(|segments| segments.collect())
            .unwrap_or_default();

        if path_segments.is_empty() {
            host.to_string()
        } else {
            format!("{} {}", host, path_segments.join(" "))
        }
    });
    // Default site version to "latest" if not provided
    let site_version = version.as_deref().unwrap_or("latest");

    eprintln!("   Name: {}", site_name);
    eprintln!("   Version: {}", site_version);
    eprintln!();

    // Initialize database
    eprint!("🗄️ Connecting to database... ");
    io::stdout().flush().context("Failed to flush stdout")?;

    let db_path = config.database_path()?;
    let database = Database::new(db_path.to_string_lossy().as_ref())
        .await
        .context("Failed to initialize database")?;
    eprintln!("✅");

    // Check if site already exists
    eprint!("🔍 Checking for existing site... ");
    io::stdout().flush().context("Failed to flush stdout")?;

    let site =
        if let Some(existing_site) = SiteQueries::get_by_index_url(database.pool(), url).await? {
            eprintln!("⚠️  Found existing site!");
            eprintln!();
            eprintln!(
                "📚 Site already exists: {} (ID: {})",
                existing_site.name, existing_site.id
            );
            eprintln!("   URL: {}", existing_site.index_url);
            eprintln!("   Status: {}", existing_site.status);

            if existing_site.progress_percent > 0 {
                eprintln!("   Progress: {}%", existing_site.progress_percent);
            }

            // Show statistics if available
            if let Ok(Some(stats)) =
                SiteQueries::get_statistics(database.pool(), existing_site.id).await
            {
                eprintln!("   Content Chunks: {}", stats.total_chunks);
                if stats.pending_crawl_items > 0 {
                    eprintln!("   Pending Pages: {}", stats.pending_crawl_items);
                }
            }

            eprintln!();
            eprintln!(
                "💡 Use 'docs-mcp update {}' to re-index this site",
                existing_site.id
            );
            eprintln!("💡 Use 'docs-mcp list' to see all indexed sites");

            if existing_site.is_completed() || existing_site.progress_percent == 100 {
                return Ok(existing_site);
            }

            existing_site
        } else {
            eprintln!("✅");

            // Create new site entry
            eprint!("📝 Creating site entry... ");
            io::stdout().flush().context("Failed to flush stdout")?;

            let new_site = NewSite {
                index_url: url.to_string(),
                base_url: base_url.to_string(),
                name: site_name.clone(),
                version: site_version.to_string(),
            };

            let site = SiteQueries::create(database.pool(), new_site)
                .await
                .context("Failed to create site entry")?;
            eprintln!("✅");

            eprintln!();
            eprintln!("✅ Site created successfully!");
            eprintln!("   📚 Name: {}", site.name);
            eprintln!("   🆔 ID: {}", site.id);
            eprintln!("   🌐 URL: {}", site.index_url);
            eprintln!();

            site
        };

    // Start crawling
    eprintln!("🕷️ Starting web crawling...");
    eprintln!("   This may take several minutes depending on site size.");
    eprintln!("   Respecting robots.txt and rate limiting (250ms between requests)");
    eprintln!();

    info!("Starting crawl for site {}", site.id);

    let mut crawler = SiteCrawler::new(
        database.pool().clone(),
        CrawlerConfig::default(),
        config.clone(),
        verbose,
    );

    match crawler.crawl_site(site.id, url, base_url).await {
        Ok(stats) => {
            eprintln!("✅ Crawling completed successfully!");
            eprintln!();
            eprintln!("📊 Crawl Statistics:");
            eprintln!("   🔍 Total URLs discovered: {}", stats.total_urls);
            eprintln!("   ✅ Successfully crawled: {}", stats.successful_crawls);

            if stats.failed_crawls > 0 {
                eprintln!("   ❌ Failed crawls: {}", stats.failed_crawls);
            }
            if stats.robots_blocked > 0 {
                eprintln!("   🚫 Blocked by robots.txt: {}", stats.robots_blocked);
            }

            eprintln!("   ⏱️  Duration: {:?}", stats.duration);

            // Show content statistics
            if let Ok(Some(content_stats)) =
                SiteQueries::get_statistics(database.pool(), site.id).await
            {
                eprintln!(
                    "   📄 Content chunks created: {}",
                    content_stats.total_chunks
                );
            }

            eprintln!();
            eprintln!("🎉 Site successfully added and crawled!");
            eprintln!("💡 The indexer will now generate embeddings for search");

            Ok(site)
        }
        Err(e) => {
            error!("Crawl failed: {}", e);
            eprintln!("❌ Crawling failed: {}", e);
            eprintln!();
            eprintln!("📝 Site entry has been created but crawling was unsuccessful.");
            eprintln!(
                "💡 You can try updating the site later with 'docs-mcp update {}'",
                site.id
            );
            eprintln!("💡 Check the site URL and your internet connection");
            Err(e)
        }
    }
}

/// List all indexed documentation sites with comprehensive information
#[inline]
pub async fn list_sites(config: &Config) -> Result<()> {
    let db_path = config.database_path()?;
    let database = Database::new(db_path.to_string_lossy().as_ref())
        .await
        .context("Failed to initialize database")?;

    let sites = SiteQueries::list_all(database.pool())
        .await
        .context("Failed to list sites")?;

    if sites.is_empty() {
        eprintln!("No documentation sites have been added yet.");
        eprintln!("Use 'docs-mcp add <url>' to add a site.");
        return Ok(());
    }

    eprintln!("Documentation Sites ({} total):", sites.len());
    eprintln!();

    for site in &sites {
        eprintln!("📚 {} (ID: {})", site.name, site.id);
        eprintln!("   URL: {}", site.index_url);
        eprintln!("   Status: {}", site.status);

        // Show crawl progress
        if site.total_pages > 0 {
            eprintln!(
                "   Crawl Progress: {}/{} pages ({}%)",
                site.indexed_pages, site.total_pages, site.progress_percent
            );
        }

        // Get comprehensive statistics
        match SiteQueries::get_statistics(database.pool(), site.id).await {
            Ok(Some(stats)) => {
                eprintln!("   Content Chunks: {}", stats.total_chunks);

                if stats.pending_crawl_items > 0 {
                    eprintln!("   Pending Pages: {}", stats.pending_crawl_items);
                }

                if stats.failed_crawl_items > 0 {
                    eprintln!("   Failed Pages: {}", stats.failed_crawl_items);
                }
            }
            Ok(None) => eprintln!("   Statistics: Not available"),
            Err(e) => eprintln!("   Statistics: Error - {}", e),
        }

        // Show indexing dates
        if let Some(indexed_date) = site.indexed_date {
            eprintln!(
                "   Last Indexed: {}",
                indexed_date.format("%Y-%m-%d %H:%M:%S")
            );
        }

        if let Some(heartbeat) = site.last_heartbeat {
            let elapsed = chrono::Utc::now()
                .naive_utc()
                .signed_duration_since(heartbeat)
                .num_seconds();

            if elapsed < 120 {
                eprintln!("   Indexer: Active ({}s ago)", elapsed);
            } else {
                eprintln!("   Indexer: Inactive ({}s ago)", elapsed);
            }
        }

        // Show errors
        if let Some(error) = &site.error_message {
            eprintln!("   ⚠️  Error: {}", error);
        }

        // Show creation date
        eprintln!(
            "   Created: {}",
            site.created_date.format("%Y-%m-%d %H:%M:%S")
        );

        eprintln!();
    }

    // Show summary statistics
    let total_sites = sites.len();
    let completed_sites = sites.iter().filter(|s| s.is_completed()).count();
    let indexing_sites = sites.iter().filter(|s| s.is_indexing()).count();
    let failed_sites = sites.iter().filter(|s| s.is_failed()).count();

    eprintln!("Summary:");
    eprintln!("  Total Sites: {}", total_sites);
    eprintln!("  Completed: {}", completed_sites);
    eprintln!("  Currently Indexing: {}", indexing_sites);
    eprintln!("  Failed: {}", failed_sites);

    Ok(())
}

/// Delete a documentation site with proper cleanup
#[inline]
pub async fn delete_site(site_identifier: String, config: &Config) -> Result<()> {
    // Validate input
    validation::validate_site_identifier(&site_identifier).context("Invalid site identifier")?;

    let db_path = config.database_path()?;
    let database = Database::new(db_path.to_string_lossy().as_ref())
        .await
        .context("Failed to initialize database")?;

    // Try to find site by ID first, then by name
    let site = if let Ok(id) = site_identifier.parse::<i64>() {
        SiteQueries::get_by_id(database.pool(), id).await?
    } else {
        // Search by name (find first match)
        let sites = SiteQueries::list_all(database.pool()).await?;
        sites.into_iter().find(|s| {
            s.name
                .to_lowercase()
                .contains(&site_identifier.to_lowercase())
        })
    };

    let site = site.ok_or_else(|| anyhow::anyhow!("Site not found: {}", site_identifier))?;

    eprintln!("📚 Found site: {} (ID: {})", site.name, site.id);
    eprintln!("   URL: {}", site.index_url);
    eprintln!("   Status: {}", site.status);

    // Get statistics before deletion
    if let Ok(Some(stats)) = SiteQueries::get_statistics(database.pool(), site.id).await {
        eprintln!("   Content Chunks: {}", stats.total_chunks);
        if stats.pending_crawl_items > 0 {
            eprintln!("   Pending Pages: {}", stats.pending_crawl_items);
        }
        if stats.failed_crawl_items > 0 {
            eprintln!("   Failed Pages: {}", stats.failed_crawl_items);
        }
    }

    eprintln!();
    eprintln!("⚠️  This will permanently delete:");
    eprintln!("   • Site metadata and configuration");
    eprintln!("   • All crawl queue entries");
    eprintln!("   • All indexed content chunks");
    eprintln!("   • All vector embeddings");
    eprintln!();
    eprintln!("❌ This action cannot be undone!");
    eprintln!();

    // Get user confirmation
    eprint!("Type 'DELETE' to confirm deletion: ");
    use std::io::{self, Write};
    io::stdout().flush().context("Failed to flush stdout")?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("Failed to read input")?;
    let input = input.trim();

    if input != "DELETE" {
        eprintln!("❌ Deletion cancelled. No changes were made.");
        return Ok(());
    }

    eprintln!();
    eprintln!("🗑️  Deleting site and all associated data...");

    // Initialize vector store for cleanup
    let vector_store = VectorStore::new(config)
        .await
        .context("Failed to initialize vector store")?;

    // Delete vector embeddings first
    eprint!("   Deleting vector embeddings... ");
    io::stdout().flush().context("Failed to flush stdout")?;

    match vector_store
        .delete_site_embeddings(&site.id.to_string())
        .await
    {
        Ok(_) => eprintln!("✅"),
        Err(e) => {
            eprintln!("⚠️  Warning: Failed to delete vector embeddings: {}", e);
            eprintln!("   Some vector data may remain in LanceDB");
        }
    }

    // Delete from SQLite database (this will cascade to delete crawl_queue and indexed_chunks)
    eprint!("   Deleting site metadata and chunks... ");
    io::stdout().flush().context("Failed to flush stdout")?;

    let deleted = SiteQueries::delete(database.pool(), site.id)
        .await
        .context("Failed to delete site from database")?;

    if deleted {
        eprintln!("✅");
        eprintln!();
        eprintln!(
            "✅ Site successfully deleted: {} (ID: {})",
            site.name, site.id
        );
        eprintln!("   All associated data has been removed.");
    } else {
        eprintln!("❌");
        return Err(anyhow::anyhow!(
            "Failed to delete site - site may have already been removed"
        ));
    }

    // Optimize database after deletion
    eprint!("   Optimizing database... ");
    io::stdout().flush().context("Failed to flush stdout")?;

    if let Err(e) = database.optimize().await {
        eprintln!("⚠️  Warning: Failed to optimize database: {}", e);
    } else {
        eprintln!("✅");
    }

    eprintln!();
    eprintln!("💡 Use 'docs-mcp list' to see remaining sites");
    eprintln!("💡 Use 'docs-mcp add <url>' to index a new site");

    Ok(())
}

/// Update/re-index a documentation site with proper cleanup
#[inline]
pub async fn update_site(site_identifier: String, config: &Config, verbose: bool) -> Result<Site> {
    // Validate input
    validation::validate_site_identifier(&site_identifier).context("Invalid site identifier")?;

    let db_path = config.database_path()?;
    let database = Database::new(db_path.to_string_lossy().as_ref())
        .await
        .context("Failed to initialize database")?;

    // Try to find site by ID first, then by name
    let site = if let Ok(id) = site_identifier.parse::<i64>() {
        SiteQueries::get_by_id(database.pool(), id).await?
    } else {
        // Search by name (find first match)
        let sites = SiteQueries::list_all(database.pool()).await?;
        sites.into_iter().find(|s| {
            s.name
                .to_lowercase()
                .contains(&site_identifier.to_lowercase())
        })
    };

    let site = site.ok_or_else(|| anyhow::anyhow!("Site not found: {}", site_identifier))?;

    eprintln!("🔄 Updating site: {} (ID: {})", site.name, site.id);
    eprintln!("   URL: {}", site.index_url);
    eprintln!("   Current Status: {}", site.status);

    // Get statistics before update
    if let Ok(Some(stats)) = SiteQueries::get_statistics(database.pool(), site.id).await {
        eprintln!("   Current Content: {} chunks", stats.total_chunks);
        if stats.pending_crawl_items > 0 {
            eprintln!("   Pending Pages: {}", stats.pending_crawl_items);
        }
    }

    eprintln!();
    eprintln!("⚠️  This will:");
    eprintln!("   • Clear all existing crawl queue entries");
    eprintln!("   • Clear all existing indexed content and embeddings");
    eprintln!("   • Re-crawl the entire site from scratch");
    eprintln!("   • Re-generate all embeddings");
    eprintln!();

    // Get user confirmation for destructive operation
    eprint!("Continue with re-indexing? [y/N]: ");
    use std::io::{self, Write};
    io::stdout().flush().context("Failed to flush stdout")?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("Failed to read input")?;
    let input = input.trim().to_lowercase();

    if input != "y" && input != "yes" {
        bail!("❌ Update cancelled. No changes were made.");
    }

    eprintln!();
    eprintln!("🧹 Preparing for re-indexing...");

    // Initialize vector store for cleanup
    let vector_store = VectorStore::new(config)
        .await
        .context("Failed to initialize vector store")?;

    // Clear existing embeddings
    eprint!("   Clearing old embeddings... ");
    io::stdout().flush().context("Failed to flush stdout")?;

    match vector_store
        .delete_site_embeddings(&site.id.to_string())
        .await
    {
        Ok(_) => eprintln!("✅"),
        Err(e) => {
            eprintln!("⚠️  Warning: Failed to clear embeddings: {}", e);
            eprintln!("   Proceeding with update anyway...");
        }
    }

    // Clear crawl queue and chunks (they will be recreated)
    eprint!("   Clearing crawl queue and chunks... ");
    io::stdout().flush().context("Failed to flush stdout")?;

    // Clear crawl queue entries for this site
    sqlx::query!("DELETE FROM crawl_queue WHERE site_id = ?", site.id)
        .execute(database.pool())
        .await
        .context("Failed to clear crawl queue")?;

    // Clear indexed chunks for this site
    sqlx::query!("DELETE FROM indexed_chunks WHERE site_id = ?", site.id)
        .execute(database.pool())
        .await
        .context("Failed to clear indexed chunks")?;

    eprintln!("✅");

    // Reset site status and progress
    eprint!("   Resetting site status... ");
    io::stdout().flush().context("Failed to flush stdout")?;

    let update = SiteUpdate {
        status: Some(SiteStatus::Pending),
        progress_percent: Some(0),
        total_pages: Some(0),
        indexed_pages: Some(0),
        error_message: None,
        last_heartbeat: None,
        indexed_date: None,
    };

    SiteQueries::update(database.pool(), site.id, update)
        .await
        .context("Failed to reset site status")?;

    eprintln!("✅");
    eprintln!();

    // Start re-crawling
    info!("Starting re-crawl for site {}", site.id);
    eprintln!("🚀 Starting re-crawl and re-indexing...");
    eprintln!("   This may take several minutes depending on site size.");
    eprintln!();

    let mut crawler = SiteCrawler::new(
        database.pool().clone(),
        CrawlerConfig::default(),
        config.clone(),
        verbose,
    );

    match crawler
        .crawl_site(site.id, &site.index_url, &site.base_url)
        .await
    {
        Ok(stats) => {
            eprintln!();
            eprintln!("✅ Re-indexing completed successfully!");
            eprintln!("   📄 Total URLs discovered: {}", stats.total_urls);
            eprintln!("   ✅ Successfully crawled: {}", stats.successful_crawls);
            eprintln!("   ❌ Failed crawls: {}", stats.failed_crawls);
            eprintln!("   🚫 Blocked by robots.txt: {}", stats.robots_blocked);
            eprintln!("   ⏱️  Duration: {:?}", stats.duration);

            // Show new statistics
            if let Ok(Some(stats)) = SiteQueries::get_statistics(database.pool(), site.id).await {
                eprintln!("   📊 Content chunks: {}", stats.total_chunks);
            }

            eprintln!();
            eprintln!("💡 The indexer will generate embeddings for new content");
            eprintln!("💡 Use 'docs-mcp status' to monitor embedding generation progress");
            Ok(site)
        }
        Err(e) => {
            error!("Re-indexing failed: {}", e);
            eprintln!("❌ Re-indexing failed: {}", e);
            eprintln!();
            eprintln!("💡 The site has been reset to pending status");
            eprintln!("💡 You can try running the update command again");
            Err(e)
        }
    }
}

/// Show detailed status of the indexing pipeline
#[inline]
pub async fn show_status(config: &Config) -> Result<()> {
    eprintln!("📊 Docs-MCP Status Report");
    eprintln!("{}", "=".repeat(50));
    eprintln!();

    // Database connectivity
    eprintln!("🗄️  Database Status:");
    let database = match Database::new(&config.database_path()?).await {
        Ok(db) => {
            eprintln!("   ✅ SQLite: Connected");
            Some(db)
        }
        Err(e) => {
            eprintln!("   ❌ SQLite: Failed to connect - {}", e);
            None
        }
    };

    // Ollama connectivity
    eprintln!("🤖 Ollama Status:");
    match crate::embeddings::ollama::OllamaClient::new(config.ollama.clone()) {
        Ok(client) => match client.health_check() {
            Ok(()) => {
                eprintln!(
                    "   ✅ Ollama: Connected ({}:{})",
                    config.ollama.host, config.ollama.port
                );
                eprintln!("   📋 Model: {}", config.ollama.model);
                eprintln!("   🔢 Batch Size: {}", config.ollama.batch_size);
            }
            Err(e) => {
                eprintln!("   ⚠️  Ollama: Connected but unhealthy - {}", e);
            }
        },
        Err(e) => {
            eprintln!("   ❌ Ollama: Failed to connect - {}", e);
        }
    }

    // Vector database status
    eprintln!("🔍 Vector Database Status:");
    match VectorStore::new(config).await {
        Ok(_store) => {
            eprintln!("   ✅ LanceDB: Connected");
        }
        Err(e) => {
            eprintln!("   ❌ LanceDB: Failed to connect - {}", e);
        }
    }

    if let Some(database) = database {
        eprintln!();
        eprintln!("🔄 Indexer Status:");

        // Show site statistics
        eprintln!();
        eprintln!("📚 Site Overview:");
        match SiteQueries::list_all(database.pool()).await {
            Ok(sites) => {
                if sites.is_empty() {
                    eprintln!("   📭 No sites indexed yet");
                } else {
                    let total_sites = sites.len();
                    let completed_sites = sites.iter().filter(|s| s.is_completed()).count();
                    let indexing_sites = sites.iter().filter(|s| s.is_indexing()).count();
                    let failed_sites = sites.iter().filter(|s| s.is_failed()).count();
                    let pending_sites = sites
                        .iter()
                        .filter(|s| s.status == SiteStatus::Pending)
                        .count();

                    eprintln!("   📊 Total Sites: {}", total_sites);
                    eprintln!("   ✅ Completed: {}", completed_sites);
                    eprintln!("   🔄 Currently Indexing: {}", indexing_sites);
                    eprintln!("   ⏳ Pending: {}", pending_sites);
                    eprintln!("   ❌ Failed: {}", failed_sites);

                    // Show total chunks across all sites
                    let mut total_chunks = 0;
                    for site in &sites {
                        if let Ok(Some(stats)) =
                            SiteQueries::get_statistics(database.pool(), site.id).await
                        {
                            total_chunks += stats.total_chunks;
                        }
                    }
                    eprintln!("   📄 Total Chunks Indexed: {}", total_chunks);
                }
            }
            Err(e) => {
                eprintln!("   ❌ Failed to load site statistics: {}", e);
            }
        }
    }

    eprintln!();
    eprintln!("💡 Next Steps:");
    eprintln!("   • Use 'docs-mcp add <url>' to index a new documentation site");
    eprintln!("   • Use 'docs-mcp list' to see detailed site information");
    eprintln!("   • Use 'docs-mcp serve' to start the MCP server for AI assistants");

    Ok(())
}

/// Start MCP server
#[inline]
pub async fn serve_mcp(config: &Config) -> Result<()> {
    info!("Starting MCP server with stdio transport");

    // Verify Ollama connectivity before starting
    match crate::embeddings::ollama::OllamaClient::new(config.ollama.clone()) {
        Ok(client) => match client.health_check() {
            Ok(()) => {
                info!(
                    "✅ Ollama connected at {}:{} with model {}",
                    config.ollama.host, config.ollama.port, config.ollama.model
                );
            }
            Err(e) => {
                warn!("⚠️  Ollama is reachable but unhealthy: {}", e);
                eprintln!("Warning: Ollama may not be ready. Background indexing may fail.");
            }
        },
        Err(e) => {
            error!("❌ Failed to connect to Ollama: {}", e);
            eprintln!(
                "Error: Cannot connect to Ollama at {}:{}",
                config.ollama.host, config.ollama.port
            );
            eprintln!("Please ensure Ollama is running and accessible.");
            eprintln!("Use 'docs-mcp config' to update connection settings.");
            return Err(e);
        }
    }

    // Initialize MCP server components
    eprintln!("🌐 Initializing MCP server...");

    let db_path = config.database_path()?;
    let sqlite_db = std::sync::Arc::new(
        crate::database::sqlite::Database::initialize_from_config_dir(&db_path)
            .await
            .context("Failed to initialize SQLite database")?,
    );

    let vector_store = std::sync::Arc::new(
        VectorStore::new(config)
            .await
            .context("Failed to initialize vector store")?,
    );

    let ollama_client = std::sync::Arc::new(
        crate::embeddings::ollama::OllamaClient::new(config.ollama.clone())
            .context("Failed to create Ollama client")?,
    );

    // Register tools
    let search_definition = crate::mcp::tools::SearchDocsHandler::tool_definition();
    let list_definition = crate::mcp::tools::ListSitesHandler::tool_definition();

    // Create MCP server
    let server = ServerBuilder::new("docs-mcp", env!("CARGO_PKG_VERSION"))
        .with_transport(StdioTransport::new())
        .with_tool(
            &search_definition.name,
            search_definition.description.as_deref(),
            search_definition.input_schema,
            {
                let sqlite_db = std::sync::Arc::clone(&sqlite_db);
                let vector_store = std::sync::Arc::clone(&vector_store);
                let ollama_client = std::sync::Arc::clone(&ollama_client);
                move |args| {
                    let handler = crate::mcp::tools::SearchDocsHandler::new(
                        std::sync::Arc::clone(&sqlite_db),
                        std::sync::Arc::clone(&vector_store),
                        std::sync::Arc::clone(&ollama_client),
                    );
                    let params: CallToolParams = from_value(args)?;
                    block_in_place(move || {
                        Handle::current().block_on(async move { handler.handle(params).await })
                    })
                }
            },
        )
        .with_tool(
            &list_definition.name,
            list_definition.description.as_deref(),
            list_definition.input_schema,
            {
                let sqlite_db = std::sync::Arc::clone(&sqlite_db);
                move |args| {
                    let handler =
                        crate::mcp::tools::ListSitesHandler::new(std::sync::Arc::clone(&sqlite_db));
                    let params: CallToolParams = from_value(args)?;
                    block_in_place(move || {
                        Handle::current().block_on(async move { handler.handle(params).await })
                    })
                }
            },
        )
        .build()?;

    eprintln!("✅ MCP server initialized with tools: search_docs, list_sites");
    eprintln!("🌐 Starting MCP server with stdio transport...");
    eprintln!("📊 Use 'docs-mcp status' to monitor indexing progress");
    eprintln!("📚 Use 'docs-mcp list' to see indexed sites");
    eprintln!();
    eprintln!("Note: Server ready for MCP client connections via stdio.");

    server.run().await?;

    info!("Server shutting down");
    eprintln!("✅ Shutdown complete");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validation::*;

    #[test]
    fn validate_site_identifier_works() {
        // Valid cases
        assert!(validate_site_identifier("1").is_ok());
        assert!(validate_site_identifier("123").is_ok());
        assert!(validate_site_identifier("rust-docs").is_ok());
        assert!(validate_site_identifier("Python Documentation").is_ok());

        // Invalid cases
        assert!(validate_site_identifier("").is_err());
        assert!(validate_site_identifier("   ").is_err());
        assert!(validate_site_identifier("0").is_err());
        assert!(validate_site_identifier("-1").is_err());
        assert!(validate_site_identifier("-123").is_err());
    }

    #[test]
    fn validate_documentation_url_works() {
        // Valid cases
        assert!(validate_documentation_url("https://docs.rust-lang.org").is_ok());
        assert!(validate_documentation_url("http://localhost:8080/docs").is_ok());
        assert!(validate_documentation_url("https://python.org/docs/3.9/").is_ok());

        // Invalid cases
        assert!(validate_documentation_url("").is_err());
        assert!(validate_documentation_url("   ").is_err());
        assert!(validate_documentation_url("not-a-url").is_err());
        assert!(validate_documentation_url("ftp://example.com").is_err());
        assert!(validate_documentation_url("file:///local/path").is_err());
        assert!(validate_documentation_url("https://").is_err());
    }

    #[test]
    fn validate_site_name_works() {
        // Valid cases
        assert!(validate_site_name("Rust Documentation").is_ok());
        assert!(validate_site_name("Python 3.9 Docs").is_ok());
        assert!(validate_site_name("API Reference").is_ok());
        assert!(validate_site_name("a").is_ok()); // Single character

        // Invalid cases
        assert!(validate_site_name("").is_err());
        assert!(validate_site_name("   ").is_err());
        assert!(validate_site_name("Name with\nnewline").is_err());
        assert!(validate_site_name("Name with\ttab").is_err());
        assert!(validate_site_name("Name with\rcarriage return").is_err());

        // Test maximum length (101 characters)
        let long_name = "a".repeat(101);
        assert!(validate_site_name(&long_name).is_err());

        // Test exactly 100 characters (should be OK)
        let max_name = "a".repeat(100);
        assert!(validate_site_name(&max_name).is_ok());
    }

    // Integration tests would go in tests/ directory for cross-module testing
    // These are unit tests for validation functions only

    #[test]
    fn url_parsing_edge_cases() {
        // Test various URL formats that should be accepted
        let valid_urls = vec![
            "https://docs.example.com",
            "http://127.0.0.1:8080",
            "https://sub.domain.example.com/path/to/docs",
            "http://localhost:3000/docs/v1",
            "https://example.com:8443/documentation",
        ];

        for url in valid_urls {
            assert!(
                validate_documentation_url(url).is_ok(),
                "URL should be valid: {}",
                url
            );
        }

        // Test URLs that should be rejected
        let invalid_urls = vec![
            "javascript:alert('xss')",
            "data:text/html,<script>alert('xss')</script>",
            "mailto:admin@example.com",
            "tel:+1234567890",
            "",
            "   ",
            "not a url at all",
        ];

        for url in invalid_urls {
            assert!(
                validate_documentation_url(url).is_err(),
                "URL should be invalid: {}",
                url
            );
        }
    }

    #[test]
    fn site_identifier_parsing() {
        // Test that numeric IDs are properly validated
        assert!(validate_site_identifier("1").is_ok());
        assert!(validate_site_identifier("999999").is_ok());

        // Test that string names are accepted
        assert!(validate_site_identifier("my-docs").is_ok());
        assert!(validate_site_identifier("Python Documentation").is_ok());
        assert!(validate_site_identifier("docs with spaces").is_ok());

        // Test edge cases
        assert!(validate_site_identifier("0").is_err()); // Zero ID not allowed
        assert!(validate_site_identifier("-1").is_err()); // Negative ID not allowed
        assert!(validate_site_identifier("").is_err()); // Empty string not allowed
        assert!(validate_site_identifier("   ").is_err()); // Whitespace only not allowed
    }
}
