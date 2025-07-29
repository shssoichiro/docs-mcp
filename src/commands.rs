use anyhow::{Context, Result};
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::config::{Config, get_config_dir};
use crate::crawler::{CrawlerConfig, SiteCrawler};
use crate::database::sqlite::{Database, NewSite, SiteQueries};
use crate::indexer::BackgroundIndexer;

/// Common formatting utilities for consistent CLI output
pub mod formatting {
    use std::io::{self, Write};

    /// Print a step indicator with consistent formatting
    #[inline]
    pub fn print_step(message: &str) -> io::Result<()> {
        print!("{} ", message);
        io::stdout().flush()
    }

    /// Print success indicator
    #[inline]
    pub fn print_success() {
        println!("✅");
    }

    /// Print warning indicator
    #[inline]
    pub fn print_warning() {
        println!("⚠️");
    }

    /// Print error indicator
    #[inline]
    pub fn print_error() {
        println!("❌");
    }

    /// Print a section header with consistent formatting
    #[inline]
    pub fn print_section_header(title: &str) {
        println!();
        println!("{}", title);
    }

    /// Print a subsection with consistent indentation
    #[inline]
    pub fn print_subsection(label: &str, value: &str) {
        println!("   {}: {}", label, value);
    }

    /// Print a status line with emoji and text
    #[inline]
    pub fn print_status(emoji: &str, message: &str) {
        println!("{} {}", emoji, message);
    }

    /// Print a tip/suggestion
    #[inline]
    pub fn print_tip(message: &str) {
        println!("💡 {}", message);
    }

    /// Print a completion message
    #[inline]
    pub fn print_completion(message: &str) {
        println!();
        println!("🎉 {}", message);
    }

    /// Print an error message with consistent formatting
    #[inline]
    pub fn print_error_message(context: &str, error: &str) {
        println!("❌ {}: {}", context, error);
    }

    /// Print help suggestions
    #[inline]
    pub fn print_help_suggestions(suggestions: &[&str]) {
        println!();
        for suggestion in suggestions {
            print_tip(suggestion);
        }
    }
}

/// Validation functions for CLI commands
pub mod validation {
    use anyhow::{Result, anyhow};
    use url::Url;

    /// Validate that a site identifier is either a valid ID or a non-empty name
    #[inline]
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
    #[inline]
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
    #[inline]
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

    /// Validate port number
    #[inline]
    pub fn validate_port(port: u16) -> Result<()> {
        if port == 0 {
            return Err(anyhow!("Port number cannot be 0"));
        }

        if port < 1024 {
            eprintln!(
                "⚠️  Warning: Using port {} (below 1024) may require administrator privileges",
                port
            );
        }

        Ok(())
    }
}

/// Add a new documentation site for indexing with comprehensive progress display
#[inline]
pub async fn add_site(url: String, name: Option<String>) -> Result<()> {
    println!("🚀 Adding new documentation site");
    println!("   URL: {}", url);

    info!("Adding documentation site: {}", url);

    // Validate inputs
    print!("🔍 Validating inputs... ");
    use std::io::{self, Write};
    io::stdout().flush().context("Failed to flush stdout")?;

    let parsed_url =
        validation::validate_documentation_url(&url).context("Invalid URL provided")?;

    if let Some(ref site_name) = name {
        validation::validate_site_name(site_name).context("Invalid site name provided")?;
    }

    println!("✅");

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

    println!("   Name: {}", site_name);
    println!();

    // Initialize database
    print!("🗄️  Connecting to database... ");
    io::stdout().flush().context("Failed to flush stdout")?;

    let config_dir = get_config_dir()?;
    let db_path = config_dir.join("docs.db");
    let database = Database::new(db_path.to_string_lossy().as_ref())
        .await
        .context("Failed to initialize database")?;
    println!("✅");

    // Check if site already exists
    print!("🔍 Checking for existing site... ");
    io::stdout().flush().context("Failed to flush stdout")?;

    if let Some(existing_site) = SiteQueries::get_by_base_url(database.pool(), &url).await? {
        println!("⚠️  Found existing site!");
        println!();
        println!(
            "📚 Site already exists: {} (ID: {})",
            existing_site.name, existing_site.id
        );
        println!("   URL: {}", existing_site.base_url);
        println!("   Status: {}", existing_site.status);

        if existing_site.progress_percent > 0 {
            println!("   Progress: {}%", existing_site.progress_percent);
        }

        // Show statistics if available
        if let Ok(Some(stats)) =
            SiteQueries::get_statistics(database.pool(), existing_site.id).await
        {
            println!("   Content Chunks: {}", stats.total_chunks);
            if stats.pending_crawl_items > 0 {
                println!("   Pending Pages: {}", stats.pending_crawl_items);
            }
        }

        println!();
        println!(
            "💡 Use 'docs-mcp update {}' to re-index this site",
            existing_site.id
        );
        println!("💡 Use 'docs-mcp list' to see all indexed sites");
        return Ok(());
    }
    println!("✅");

    // Create new site entry
    print!("📝 Creating site entry... ");
    io::stdout().flush().context("Failed to flush stdout")?;

    let new_site = NewSite {
        base_url: url.clone(),
        name: site_name.clone(),
        version: "1.0".to_string(),
    };

    let site = SiteQueries::create(database.pool(), new_site)
        .await
        .context("Failed to create site entry")?;
    println!("✅");

    println!();
    println!("✅ Site created successfully!");
    println!("   📚 Name: {}", site.name);
    println!("   🆔 ID: {}", site.id);
    println!("   🌐 URL: {}", site.base_url);
    println!();

    // Start crawling
    println!("🕷️  Starting web crawling...");
    println!("   This may take several minutes depending on site size.");
    println!("   Respecting robots.txt and rate limiting (250ms between requests)");
    println!();

    info!("Starting crawl for site {}", site.id);

    let crawler_config = CrawlerConfig::default();
    let mut crawler = SiteCrawler::new(database.pool().clone(), crawler_config);

    match crawler.crawl_site(site.id, &url).await {
        Ok(stats) => {
            println!("✅ Crawling completed successfully!");
            println!();
            println!("📊 Crawl Statistics:");
            println!("   🔍 Total URLs discovered: {}", stats.total_urls);
            println!("   ✅ Successfully crawled: {}", stats.successful_crawls);

            if stats.failed_crawls > 0 {
                println!("   ❌ Failed crawls: {}", stats.failed_crawls);
            }
            if stats.robots_blocked > 0 {
                println!("   🚫 Blocked by robots.txt: {}", stats.robots_blocked);
            }

            println!("   ⏱️  Duration: {:?}", stats.duration);

            // Show content statistics
            if let Ok(Some(content_stats)) =
                SiteQueries::get_statistics(database.pool(), site.id).await
            {
                println!(
                    "   📄 Content chunks created: {}",
                    content_stats.total_chunks
                );
            }

            println!();
            println!("🎉 Site successfully added and crawled!");
            println!("💡 The background indexer will now generate embeddings for search");
            println!("💡 Use 'docs-mcp status' to monitor embedding generation progress");
            println!("💡 Use 'docs-mcp serve' to start the MCP server for AI assistants");
        }
        Err(e) => {
            error!("Crawl failed: {}", e);
            println!("❌ Crawling failed: {}", e);
            println!();
            println!("📝 Site entry has been created but crawling was unsuccessful.");
            println!(
                "💡 You can try updating the site later with 'docs-mcp update {}'",
                site.id
            );
            println!("💡 Check the site URL and your internet connection");
            return Err(e);
        }
    }

    Ok(())
}

/// List all indexed documentation sites with comprehensive information
#[inline]
pub async fn list_sites() -> Result<()> {
    let config_dir = get_config_dir()?;
    let db_path = config_dir.join("docs.db");
    let database = Database::new(db_path.to_string_lossy().as_ref())
        .await
        .context("Failed to initialize database")?;

    let sites = SiteQueries::list_all(database.pool())
        .await
        .context("Failed to list sites")?;

    if sites.is_empty() {
        println!("No documentation sites have been added yet.");
        println!("Use 'docs-mcp add <url>' to add a site.");
        return Ok(());
    }

    println!("Documentation Sites ({} total):", sites.len());
    println!();

    for site in &sites {
        println!("📚 {} (ID: {})", site.name, site.id);
        println!("   URL: {}", site.base_url);
        println!("   Status: {}", site.status);

        // Show crawl progress
        if site.total_pages > 0 {
            println!(
                "   Crawl Progress: {}/{} pages ({}%)",
                site.indexed_pages, site.total_pages, site.progress_percent
            );
        }

        // Get comprehensive statistics
        match SiteQueries::get_statistics(database.pool(), site.id).await {
            Ok(Some(stats)) => {
                println!("   Content Chunks: {}", stats.total_chunks);

                if stats.pending_crawl_items > 0 {
                    println!("   Pending Pages: {}", stats.pending_crawl_items);
                }

                if stats.failed_crawl_items > 0 {
                    println!("   Failed Pages: {}", stats.failed_crawl_items);
                }
            }
            Ok(None) => println!("   Statistics: Not available"),
            Err(e) => println!("   Statistics: Error - {}", e),
        }

        // Show indexing dates
        if let Some(indexed_date) = site.indexed_date {
            println!(
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
                println!("   Indexer: Active ({}s ago)", elapsed);
            } else {
                println!("   Indexer: Inactive ({}s ago)", elapsed);
            }
        }

        // Show errors
        if let Some(error) = &site.error_message {
            println!("   ⚠️  Error: {}", error);
        }

        // Show creation date
        println!(
            "   Created: {}",
            site.created_date.format("%Y-%m-%d %H:%M:%S")
        );

        println!();
    }

    // Show summary statistics
    let total_sites = sites.len();
    let completed_sites = sites.iter().filter(|s| s.is_completed()).count();
    let indexing_sites = sites.iter().filter(|s| s.is_indexing()).count();
    let failed_sites = sites.iter().filter(|s| s.is_failed()).count();

    println!("Summary:");
    println!("  Total Sites: {}", total_sites);
    println!("  Completed: {}", completed_sites);
    println!("  Currently Indexing: {}", indexing_sites);
    println!("  Failed: {}", failed_sites);

    Ok(())
}

/// Delete a documentation site with proper cleanup
#[inline]
pub async fn delete_site(site_identifier: String) -> Result<()> {
    // Validate input
    validation::validate_site_identifier(&site_identifier).context("Invalid site identifier")?;

    let config_dir = get_config_dir()?;
    let db_path = config_dir.join("docs.db");
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

    println!("📚 Found site: {} (ID: {})", site.name, site.id);
    println!("   URL: {}", site.base_url);
    println!("   Status: {}", site.status);

    // Get statistics before deletion
    if let Ok(Some(stats)) = SiteQueries::get_statistics(database.pool(), site.id).await {
        println!("   Content Chunks: {}", stats.total_chunks);
        if stats.pending_crawl_items > 0 {
            println!("   Pending Pages: {}", stats.pending_crawl_items);
        }
        if stats.failed_crawl_items > 0 {
            println!("   Failed Pages: {}", stats.failed_crawl_items);
        }
    }

    println!();
    println!("⚠️  This will permanently delete:");
    println!("   • Site metadata and configuration");
    println!("   • All crawl queue entries");
    println!("   • All indexed content chunks");
    println!("   • All vector embeddings");
    println!();
    println!("❌ This action cannot be undone!");
    println!();

    // Get user confirmation
    print!("Type 'DELETE' to confirm deletion: ");
    use std::io::{self, Write};
    io::stdout().flush().context("Failed to flush stdout")?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("Failed to read input")?;
    let input = input.trim();

    if input != "DELETE" {
        println!("❌ Deletion cancelled. No changes were made.");
        return Ok(());
    }

    println!();
    println!("🗑️  Deleting site and all associated data...");

    // Initialize vector store for cleanup
    let config = Config::load().unwrap_or_default();
    let mut vector_store = crate::database::lancedb::VectorStore::new(&config)
        .await
        .context("Failed to initialize vector store")?;

    // Delete vector embeddings first
    print!("   Deleting vector embeddings... ");
    io::stdout().flush().context("Failed to flush stdout")?;

    match vector_store
        .delete_site_embeddings(&site.id.to_string())
        .await
    {
        Ok(_) => println!("✅"),
        Err(e) => {
            println!("⚠️  Warning: Failed to delete vector embeddings: {}", e);
            println!("   Some vector data may remain in LanceDB");
        }
    }

    // Delete from SQLite database (this will cascade to delete crawl_queue and indexed_chunks)
    print!("   Deleting site metadata and chunks... ");
    io::stdout().flush().context("Failed to flush stdout")?;

    let deleted = SiteQueries::delete(database.pool(), site.id)
        .await
        .context("Failed to delete site from database")?;

    if deleted {
        println!("✅");
        println!();
        println!(
            "✅ Site successfully deleted: {} (ID: {})",
            site.name, site.id
        );
        println!("   All associated data has been removed.");
    } else {
        println!("❌");
        return Err(anyhow::anyhow!(
            "Failed to delete site - site may have already been removed"
        ));
    }

    // Optimize database after deletion
    print!("   Optimizing database... ");
    io::stdout().flush().context("Failed to flush stdout")?;

    if let Err(e) = database.optimize().await {
        println!("⚠️  Warning: Failed to optimize database: {}", e);
    } else {
        println!("✅");
    }

    println!();
    println!("💡 Use 'docs-mcp list' to see remaining sites");
    println!("💡 Use 'docs-mcp add <url>' to index a new site");

    Ok(())
}

/// Update/re-index a documentation site with proper cleanup
#[inline]
pub async fn update_site(site_identifier: String) -> Result<()> {
    // Validate input
    validation::validate_site_identifier(&site_identifier).context("Invalid site identifier")?;

    let config_dir = get_config_dir()?;
    let db_path = config_dir.join("docs.db");
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

    println!("🔄 Updating site: {} (ID: {})", site.name, site.id);
    println!("   URL: {}", site.base_url);
    println!("   Current Status: {}", site.status);

    // Get statistics before update
    if let Ok(Some(stats)) = SiteQueries::get_statistics(database.pool(), site.id).await {
        println!("   Current Content: {} chunks", stats.total_chunks);
        if stats.pending_crawl_items > 0 {
            println!("   Pending Pages: {}", stats.pending_crawl_items);
        }
    }

    println!();
    println!("⚠️  This will:");
    println!("   • Clear all existing crawl queue entries");
    println!("   • Clear all existing indexed content and embeddings");
    println!("   • Re-crawl the entire site from scratch");
    println!("   • Re-generate all embeddings");
    println!();

    // Get user confirmation for destructive operation
    print!("Continue with re-indexing? [y/N]: ");
    use std::io::{self, Write};
    io::stdout().flush().context("Failed to flush stdout")?;

    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .context("Failed to read input")?;
    let input = input.trim().to_lowercase();

    if input != "y" && input != "yes" {
        println!("❌ Update cancelled. No changes were made.");
        return Ok(());
    }

    println!();
    println!("🧹 Preparing for re-indexing...");

    // Initialize vector store for cleanup
    let config = Config::load().unwrap_or_default();
    let mut vector_store = crate::database::lancedb::VectorStore::new(&config)
        .await
        .context("Failed to initialize vector store")?;

    // Clear existing embeddings
    print!("   Clearing old embeddings... ");
    io::stdout().flush().context("Failed to flush stdout")?;

    match vector_store
        .delete_site_embeddings(&site.id.to_string())
        .await
    {
        Ok(_) => println!("✅"),
        Err(e) => {
            println!("⚠️  Warning: Failed to clear embeddings: {}", e);
            println!("   Proceeding with update anyway...");
        }
    }

    // Clear crawl queue and chunks (they will be recreated)
    print!("   Clearing crawl queue and chunks... ");
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

    println!("✅");

    // Reset site status and progress
    print!("   Resetting site status... ");
    io::stdout().flush().context("Failed to flush stdout")?;

    let update = crate::database::sqlite::SiteUpdate {
        status: Some(crate::database::sqlite::SiteStatus::Pending),
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

    println!("✅");
    println!();

    // Start re-crawling
    info!("Starting re-crawl for site {}", site.id);
    println!("🚀 Starting re-crawl and re-indexing...");
    println!("   This may take several minutes depending on site size.");
    println!();

    let crawler_config = CrawlerConfig::default();
    let mut crawler = SiteCrawler::new(database.pool().clone(), crawler_config);

    match crawler.crawl_site(site.id, &site.base_url).await {
        Ok(stats) => {
            println!();
            println!("✅ Re-indexing completed successfully!");
            println!("   📄 Total URLs discovered: {}", stats.total_urls);
            println!("   ✅ Successfully crawled: {}", stats.successful_crawls);
            println!("   ❌ Failed crawls: {}", stats.failed_crawls);
            println!("   🚫 Blocked by robots.txt: {}", stats.robots_blocked);
            println!("   ⏱️  Duration: {:?}", stats.duration);

            // Show new statistics
            if let Ok(Some(stats)) = SiteQueries::get_statistics(database.pool(), site.id).await {
                println!("   📊 Content chunks: {}", stats.total_chunks);
            }

            println!();
            println!("💡 The background indexer will generate embeddings for new content");
            println!("💡 Use 'docs-mcp status' to monitor embedding generation progress");
        }
        Err(e) => {
            error!("Re-indexing failed: {}", e);
            println!("❌ Re-indexing failed: {}", e);
            println!();
            println!("💡 The site has been reset to pending status");
            println!("💡 You can try running the update command again");
            return Err(e);
        }
    }

    Ok(())
}

/// Show detailed status of the indexing pipeline
#[inline]
pub async fn show_status() -> Result<()> {
    let config = Config::load().unwrap_or_default();

    println!("📊 Docs-MCP Status Report");
    println!("{}", "=".repeat(50));
    println!();

    // Database connectivity
    println!("🗄️  Database Status:");
    let database = match Database::new(&config.database_path()).await {
        Ok(db) => {
            println!("   ✅ SQLite: Connected");
            Some(db)
        }
        Err(e) => {
            println!("   ❌ SQLite: Failed to connect - {}", e);
            None
        }
    };

    // Ollama connectivity
    println!("🤖 Ollama Status:");
    match crate::embeddings::ollama::OllamaClient::new(&config) {
        Ok(client) => match client.health_check() {
            Ok(()) => {
                println!(
                    "   ✅ Ollama: Connected ({}:{})",
                    config.ollama.host, config.ollama.port
                );
                println!("   📋 Model: {}", config.ollama.model);
                println!("   🔢 Batch Size: {}", config.ollama.batch_size);
            }
            Err(e) => {
                println!("   ⚠️  Ollama: Connected but unhealthy - {}", e);
            }
        },
        Err(e) => {
            println!("   ❌ Ollama: Failed to connect - {}", e);
        }
    }

    // Vector database status
    println!("🔍 Vector Database Status:");
    match crate::database::lancedb::VectorStore::new(&config).await {
        Ok(_store) => {
            println!("   ✅ LanceDB: Connected");
        }
        Err(e) => {
            println!("   ❌ LanceDB: Failed to connect - {}", e);
        }
    }

    if let Some(database) = database {
        println!();
        println!("🔄 Indexer Status:");

        // Check if indexer is running
        let mut indexer = BackgroundIndexer::new(config.clone()).await?;
        match indexer.get_indexing_status().await {
            Ok(status) => match status {
                crate::indexer::IndexingStatus::Idle => {
                    println!("   💤 Status: Idle");
                }
                crate::indexer::IndexingStatus::ProcessingSite { site_id, site_name } => {
                    println!(
                        "   🔄 Status: Processing site {} (ID: {})",
                        site_name, site_id
                    );
                }
                crate::indexer::IndexingStatus::GeneratingEmbeddings { remaining_chunks } => {
                    println!(
                        "   🧮 Status: Generating embeddings ({} chunks remaining)",
                        remaining_chunks
                    );
                }
                crate::indexer::IndexingStatus::Failed { error } => {
                    println!("   ❌ Status: Failed - {}", error);
                }
            },
            Err(e) => {
                println!("   ⚠️  Status: Unknown - {}", e);
            }
        }

        // Check database consistency
        // Show queue resource usage
        println!();
        println!("🚦 Queue Resource Usage:");
        let queue_usage = indexer.get_queue_resource_usage();
        println!(
            "   📊 Processing Items Tracked: {}",
            queue_usage.processing_items_tracked
        );
        println!(
            "   💾 Estimated Memory Usage: {:.2} MB",
            queue_usage.estimated_memory_usage_mb
        );
        println!("   📦 Active Batch Size: {}", queue_usage.active_batch_size);
        println!("   ⏱️  Timeout: {}s", queue_usage.timeout_seconds);

        println!();
        println!("🔍 Database Consistency:");
        match indexer.validate_consistency().await {
            Ok(report) => {
                if report.is_consistent {
                    println!("   ✅ Databases are consistent");
                    println!("   📊 SQLite chunks: {}", report.sqlite_chunks);
                    println!("   📊 LanceDB embeddings: {}", report.lancedb_embeddings);
                } else {
                    println!("   ⚠️  Consistency issues found:");
                    println!("   📊 SQLite chunks: {}", report.sqlite_chunks);
                    println!("   📊 LanceDB embeddings: {}", report.lancedb_embeddings);
                    if !report.missing_in_lancedb.is_empty() {
                        println!(
                            "   🚫 Missing in LanceDB: {}",
                            report.missing_in_lancedb.len()
                        );
                    }
                    if !report.orphaned_in_lancedb.is_empty() {
                        println!(
                            "   👻 Orphaned in LanceDB: {}",
                            report.orphaned_in_lancedb.len()
                        );
                    }
                }
            }
            Err(e) => {
                println!("   ❌ Failed to check consistency: {}", e);
            }
        }

        // Show site statistics
        println!();
        println!("📚 Site Overview:");
        match SiteQueries::list_all(database.pool()).await {
            Ok(sites) => {
                if sites.is_empty() {
                    println!("   📭 No sites indexed yet");
                } else {
                    let total_sites = sites.len();
                    let completed_sites = sites.iter().filter(|s| s.is_completed()).count();
                    let indexing_sites = sites.iter().filter(|s| s.is_indexing()).count();
                    let failed_sites = sites.iter().filter(|s| s.is_failed()).count();
                    let pending_sites = sites
                        .iter()
                        .filter(|s| s.status == crate::database::sqlite::SiteStatus::Pending)
                        .count();

                    println!("   📊 Total Sites: {}", total_sites);
                    println!("   ✅ Completed: {}", completed_sites);
                    println!("   🔄 Currently Indexing: {}", indexing_sites);
                    println!("   ⏳ Pending: {}", pending_sites);
                    println!("   ❌ Failed: {}", failed_sites);

                    // Show total chunks across all sites
                    let mut total_chunks = 0;
                    for site in &sites {
                        if let Ok(Some(stats)) =
                            SiteQueries::get_statistics(database.pool(), site.id).await
                        {
                            total_chunks += stats.total_chunks;
                        }
                    }
                    println!("   📄 Total Chunks Indexed: {}", total_chunks);
                }
            }
            Err(e) => {
                println!("   ❌ Failed to load site statistics: {}", e);
            }
        }
    }

    println!();
    println!("💡 Next Steps:");
    println!("   • Use 'docs-mcp add <url>' to index a new documentation site");
    println!("   • Use 'docs-mcp list' to see detailed site information");
    println!("   • Use 'docs-mcp serve' to start the MCP server for AI assistants");

    Ok(())
}

/// Start MCP server and background indexer with auto-start/termination logic
#[inline]
pub async fn serve_mcp(port: u16) -> Result<()> {
    // Validate port number
    validation::validate_port(port).context("Invalid port number")?;

    info!(
        "Starting MCP server on port {} with background indexer",
        port
    );

    // Load configuration
    let config = Config::load().context("Failed to load configuration")?;

    // Verify Ollama connectivity before starting
    match crate::embeddings::ollama::OllamaClient::new(&config) {
        Ok(client) => match client.health_check() {
            Ok(()) => {
                info!(
                    "✅ Ollama connected at {}:{} with model {}",
                    config.ollama.host, config.ollama.port, config.ollama.model
                );
            }
            Err(e) => {
                warn!("⚠️  Ollama is reachable but unhealthy: {}", e);
                println!("Warning: Ollama may not be ready. Background indexing may fail.");
            }
        },
        Err(e) => {
            error!("❌ Failed to connect to Ollama: {}", e);
            println!(
                "Error: Cannot connect to Ollama at {}:{}",
                config.ollama.host, config.ollama.port
            );
            println!("Please ensure Ollama is running and accessible.");
            println!("Use 'docs-mcp config' to update connection settings.");
            return Err(e);
        }
    }

    // Initialize background indexer
    let indexer = BackgroundIndexer::new(config.clone())
        .await
        .context("Failed to create background indexer")?;

    // Check if another indexer is already running
    let indexer_handle = if indexer.is_indexer_running().await? {
        println!("⚠️  Background indexer is already running");
        println!("Use 'docs-mcp status' to check the current status");
        println!("Starting MCP server only...");
        None
    } else {
        println!("🚀 Starting background indexer...");

        // Start background indexer in a separate task
        let indexer_handle = {
            let mut indexer_clone = BackgroundIndexer::new(config.clone()).await?;
            tokio::spawn(async move {
                match indexer_clone.start().await {
                    Ok(()) => {
                        info!("Background indexer completed successfully");
                    }
                    Err(e) => {
                        error!("Background indexer failed: {}", e);
                    }
                }
            })
        };

        // Give indexer a moment to start
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Verify indexer started successfully
        if indexer.is_indexer_running().await? {
            println!("✅ Background indexer started successfully");
        } else {
            warn!("⚠️  Background indexer may have failed to start");
        }

        // Store handle for later cleanup
        Some(indexer_handle)
    };

    // Initialize MCP server components
    println!("🌐 Initializing MCP server...");

    let config_dir = crate::config::get_config_dir()?;
    let sqlite_db = std::sync::Arc::new(
        crate::database::sqlite::Database::initialize_from_config_dir(&config_dir)
            .await
            .context("Failed to initialize SQLite database")?,
    );

    let vector_store = std::sync::Arc::new(
        crate::database::lancedb::VectorStore::new(&config)
            .await
            .context("Failed to initialize vector store")?,
    );

    let ollama_client = std::sync::Arc::new(
        crate::embeddings::ollama::OllamaClient::new(&config)
            .context("Failed to create Ollama client")?,
    );

    // Create MCP server
    let server = std::sync::Arc::new(
        crate::mcp::McpServer::new(
            "docs-mcp".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
        )
        .context("Failed to create MCP server")?,
    );

    // Register tools
    let search_handler = crate::mcp::tools::SearchDocsHandler::new(
        std::sync::Arc::clone(&sqlite_db),
        std::sync::Arc::clone(&vector_store),
        std::sync::Arc::clone(&ollama_client),
    );
    let list_handler = crate::mcp::tools::ListSitesHandler::new(std::sync::Arc::clone(&sqlite_db));

    server
        .register_tool(
            crate::mcp::tools::SearchDocsHandler::tool_definition(),
            search_handler,
        )
        .await
        .context("Failed to register search_docs tool")?;

    server
        .register_tool(
            crate::mcp::tools::ListSitesHandler::tool_definition(),
            list_handler,
        )
        .await
        .context("Failed to register list_sites tool")?;

    println!("✅ MCP server initialized with tools: search_docs, list_sites");
    println!("🌐 Starting MCP server on stdio transport...");
    println!("📊 Use 'docs-mcp status' to monitor indexing progress");
    println!("📚 Use 'docs-mcp list' to see indexed sites");
    println!();
    println!("Note: This server uses stdio transport. Connect via MCP client.");
    println!("Press Ctrl+C to stop the server and background indexer");

    // Start MCP server and background indexer concurrently with retry logic
    let mut restart_count = 0;
    const MAX_RESTARTS: u32 = 3;

    loop {
        tokio::select! {
            result = Arc::clone(&server).serve_stdio() => {
                match result {
                    Ok(()) => {
                        info!("MCP server stopped normally");
                        break;
                    }
                    Err(e) => {
                        error!("MCP server error (attempt {}/{}): {}", restart_count + 1, MAX_RESTARTS + 1, e);
                        restart_count += 1;

                        if restart_count > MAX_RESTARTS {
                            error!("Maximum restart attempts reached, shutting down");
                            break;
                        }

                        println!("⚠️  MCP server encountered an error, restarting in 5 seconds...");
                        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                        println!("🔄 Restarting MCP server (attempt {}/{})...", restart_count + 1, MAX_RESTARTS + 1);
                    }
                }
            }
            _ = tokio::signal::ctrl_c() => {
                println!("\n📴 Received interrupt signal, shutting down...");
                break;
            }
        }
    }

    // Cleanup background indexer if needed
    if indexer.is_indexer_running().await? {
        if let Some(handle) = indexer_handle {
            println!("🛑 Stopping background indexer...");
            handle.abort(); // Force stop the background task
            match handle.await {
                Ok(()) => {}
                Err(e) if e.is_cancelled() => {
                    println!("✅ Background indexer stopped");
                }
                Err(e) => {
                    warn!("⚠️  Error stopping background indexer: {}", e);
                }
            }
        }
    }

    println!("✅ Shutdown complete");

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

    #[test]
    fn validate_port_works() {
        // Valid cases
        assert!(validate_port(8080).is_ok());
        assert!(validate_port(3000).is_ok());
        assert!(validate_port(65535).is_ok()); // Max port
        assert!(validate_port(1024).is_ok()); // First non-privileged port

        // Invalid cases
        assert!(validate_port(0).is_err());

        // These should succeed but show warnings
        assert!(validate_port(80).is_ok()); // HTTP
        assert!(validate_port(443).is_ok()); // HTTPS
        assert!(validate_port(22).is_ok()); // SSH
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
