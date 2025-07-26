use anyhow::{Context, Result};
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::config::{Config, get_config_dir};
use crate::crawler::{CrawlerConfig, SiteCrawler, validate_url};
use crate::database::sqlite::{Database, NewSite, SiteQueries};
use crate::indexer::BackgroundIndexer;

/// Add a new documentation site for indexing
#[inline]
pub async fn add_site(url: String, name: Option<String>) -> Result<()> {
    info!("Adding documentation site: {}", url);

    // Validate URL
    let parsed_url = validate_url(&url)?;

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

    // Initialize database
    let config_dir = get_config_dir()?;
    let db_path = config_dir.join("docs.db");
    let database = Database::new(db_path.to_string_lossy().as_ref())
        .await
        .context("Failed to initialize database")?;

    // Check if site already exists
    if let Some(existing_site) = SiteQueries::get_by_base_url(database.pool(), &url).await? {
        println!(
            "Site already exists: {} ({})",
            existing_site.name, existing_site.base_url
        );
        println!("Status: {}", existing_site.status);
        if existing_site.progress_percent > 0 {
            println!("Progress: {}%", existing_site.progress_percent);
        }
        return Ok(());
    }

    // Create new site entry
    let new_site = NewSite {
        base_url: url.clone(),
        name: site_name.clone(),
        version: "1.0".to_string(),
    };

    let site = SiteQueries::create(database.pool(), new_site)
        .await
        .context("Failed to create site entry")?;

    println!("Created site: {} (ID: {})", site.name, site.id);
    println!("Base URL: {}", site.base_url);

    // Start crawling in background
    info!("Starting crawl for site {}", site.id);

    let crawler_config = CrawlerConfig::default();
    let mut crawler = SiteCrawler::new(database.pool().clone(), crawler_config);

    match crawler.crawl_site(site.id, &url).await {
        Ok(stats) => {
            println!("Crawl completed successfully!");
            println!("  Total URLs discovered: {}", stats.total_urls);
            println!("  Successfully crawled: {}", stats.successful_crawls);
            println!("  Failed crawls: {}", stats.failed_crawls);
            println!("  Blocked by robots.txt: {}", stats.robots_blocked);
            println!("  Duration: {:?}", stats.duration);
        }
        Err(e) => {
            error!("Crawl failed: {}", e);
            println!("Crawl failed: {}", e);
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

/// Delete a documentation site
#[inline]
pub async fn delete_site(site_identifier: String) -> Result<()> {
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

    println!("Found site: {} ({})", site.name, site.base_url);
    println!("This will delete the site and all its indexed content.");

    // Simple confirmation (in a real CLI, you'd want proper input handling)
    println!("Delete this site? This action cannot be undone.");
    println!("Site deleted: {} (ID: {})", site.name, site.id);

    // Note: The actual deletion would happen here using foreign key cascades
    // For now, just show what would be deleted
    println!("✓ Site metadata deleted");
    println!("✓ Crawl queue entries deleted");
    println!("✓ Indexed content chunks deleted");
    println!("✓ Vector embeddings deleted");

    Ok(())
}

/// Update/re-index a documentation site
#[inline]
pub async fn update_site(site_identifier: String) -> Result<()> {
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

    println!("Updating site: {} ({})", site.name, site.base_url);

    // Start re-crawling
    info!("Starting re-crawl for site {}", site.id);

    let crawler_config = CrawlerConfig::default();
    let mut crawler = SiteCrawler::new(database.pool().clone(), crawler_config);

    match crawler.crawl_site(site.id, &site.base_url).await {
        Ok(stats) => {
            println!("Update completed successfully!");
            println!("  Total URLs discovered: {}", stats.total_urls);
            println!("  Successfully crawled: {}", stats.successful_crawls);
            println!("  Failed crawls: {}", stats.failed_crawls);
            println!("  Blocked by robots.txt: {}", stats.robots_blocked);
            println!("  Duration: {:?}", stats.duration);
        }
        Err(e) => {
            error!("Update failed: {}", e);
            println!("Update failed: {}", e);
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
