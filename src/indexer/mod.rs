// Indexer module
// This module handles background process coordination and queue management

pub mod consistency;
pub mod queue;

#[cfg(test)]
mod tests;

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use chrono::Utc;
use tokio::fs;
use tokio::select;
use tokio::signal;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::DocsError;
use crate::config::Config;
use crate::crawler::extractor::ExtractedContent;
use crate::database::lancedb::{ChunkMetadata, EmbeddingRecord, VectorStore};
use crate::database::sqlite::Database;
use crate::database::sqlite::models::{
    CrawlQueueItem, NewIndexedChunk, Site, SiteStatus, SiteUpdate,
};
use crate::embeddings::chunking::{ChunkingConfig, ContentChunk, chunk_content};
use crate::embeddings::ollama::OllamaClient;

pub use consistency::{ConsistencyReport, ConsistencyValidator, SiteConsistencyIssue};
pub use queue::{
    QueueConfig, QueueManager, QueueMetrics, QueuePriority, QueueResourceUsage, QueueStats,
};

/// Background indexer that processes crawled content into searchable embeddings
pub struct BackgroundIndexer {
    config: Config,
    database: Database,
    vector_store: VectorStore,
    ollama_client: OllamaClient,
    chunking_config: ChunkingConfig,
    queue_manager: QueueManager,
    pub lock_file_path: PathBuf,
    heartbeat_interval: Duration,
    batch_size: usize,
}

/// Statistics about indexing progress
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexingStats {
    pub sites_processed: usize,
    pub pages_processed: usize,
    pub chunks_created: usize,
    pub embeddings_generated: usize,
    pub errors_encountered: usize,
}

/// Status of the indexing process
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexingStatus {
    Idle,
    ProcessingSite { site_id: i64, site_name: String },
    GeneratingEmbeddings { remaining_chunks: usize },
    Failed { error: String },
}

/// Performance metrics for the indexing system
#[derive(Debug, Clone, PartialEq)]
pub struct IndexingPerformanceMetrics {
    pub total_sites_processed: usize,
    pub total_pages_processed: usize,
    pub total_chunks_created: usize,
    pub average_processing_time_per_site: std::time::Duration,
    pub pages_per_minute: f64,
    pub chunks_per_minute: f64,
    pub database_size_mb: f64,
}

impl Default for IndexingPerformanceMetrics {
    #[inline]
    fn default() -> Self {
        Self {
            total_sites_processed: 0,
            total_pages_processed: 0,
            total_chunks_created: 0,
            average_processing_time_per_site: std::time::Duration::ZERO,
            pages_per_minute: 0.0,
            chunks_per_minute: 0.0,
            database_size_mb: 0.0,
        }
    }
}

impl BackgroundIndexer {
    /// Create a new background indexer
    #[inline]
    pub async fn new(config: Config) -> Result<Self> {
        let database = Database::new(&config.database_path())
            .await
            .context("Failed to initialize SQLite database")?;

        let vector_store = VectorStore::new(&config)
            .await
            .context("Failed to initialize LanceDB vector store")?;

        let ollama_client =
            OllamaClient::new(&config).context("Failed to initialize Ollama client")?;

        let queue_manager = QueueManager::new(database.clone(), QueueConfig::default());
        let lock_file_path = config.config_dir_path().join(".indexer.lock");

        Ok(Self {
            config,
            database,
            vector_store,
            ollama_client,
            chunking_config: ChunkingConfig::default(),
            queue_manager,
            lock_file_path,
            heartbeat_interval: Duration::from_secs(30),
            batch_size: 64,
        })
    }

    /// Start the background indexing process with signal handling
    #[inline]
    pub async fn start(&mut self) -> Result<()> {
        // Check if another indexer is already running
        if self.is_indexer_running().await? {
            return Err(DocsError::Database(
                "Another indexer process is already running".to_string(),
            )
            .into());
        }

        // Create lock file
        self.create_lock_file().await?;

        info!("Starting background indexer process");

        // Start heartbeat task
        let heartbeat_handle = self.start_heartbeat_task();

        // Main indexing loop with signal handling
        let result = self.run_indexing_loop_with_signals().await;

        // Stop heartbeat and cleanup
        heartbeat_handle.abort();
        self.cleanup_lock_file().await?;

        result
    }

    /// Check if an indexer is currently running with enhanced stale detection
    #[inline]
    pub async fn is_indexer_running(&self) -> Result<bool> {
        // First check if lock file exists
        if !self.lock_file_path.exists() {
            debug!("No lock file found, indexer not running");
            return Ok(false);
        }

        // Read lock file timestamp for additional validation
        let lock_file_valid = match fs::read_to_string(&self.lock_file_path).await {
            Ok(content) => {
                content.trim().parse::<u64>().map_or_else(
                    |_| {
                        warn!("Lock file contains invalid timestamp, considering invalid");
                        false
                    },
                    |timestamp| {
                        let lock_time = SystemTime::UNIX_EPOCH + Duration::from_secs(timestamp);
                        let now = SystemTime::now();

                        now.duration_since(lock_time).map_or_else(
                            |_| {
                                warn!("Lock file timestamp is in the future, considering invalid");
                                false
                            },
                            |elapsed| {
                                // Lock file should be recent (within 10 minutes)
                                let stale = elapsed > Duration::from_secs(600);
                                if stale {
                                    warn!(
                                        "Lock file is stale ({}s old), considering process dead",
                                        elapsed.as_secs()
                                    );
                                }
                                !stale
                            },
                        )
                    },
                )
            }
            Err(e) => {
                error!("Failed to read lock file: {}", e);
                false
            }
        };

        if !lock_file_valid {
            info!("Removing stale lock file");
            let _ = fs::remove_file(&self.lock_file_path).await;
            return Ok(false);
        }

        // Check if the process is still alive by examining heartbeat
        match self.database.get_indexer_heartbeat().await {
            Ok(heartbeat) => {
                let now = Utc::now().naive_utc();
                let elapsed = now
                    .signed_duration_since(heartbeat)
                    .num_seconds()
                    .unsigned_abs();

                // Consider stale if no heartbeat for 2 minutes (60 seconds as mentioned in requirements)
                let is_alive = elapsed < 60;

                if !is_alive {
                    warn!(
                        "Process heartbeat is stale ({}s since last update), cleaning up",
                        elapsed
                    );
                    // Clean up stale lock file
                    let _ = fs::remove_file(&self.lock_file_path).await;
                    // Reset heartbeat in database
                    let _ = self.database.clear_indexer_heartbeat().await;
                }

                Ok(is_alive)
            }
            Err(e) => {
                error!("Failed to check heartbeat: {}", e);
                // On database error, assume process might be running to avoid conflicts
                Ok(true)
            }
        }
    }

    /// Get current indexing status
    #[inline]
    pub async fn get_indexing_status(&self) -> Result<IndexingStatus> {
        // Check if indexer is running
        if !self.is_indexer_running().await? {
            return Ok(IndexingStatus::Idle);
        }

        // Get sites that are currently being indexed
        let indexing_sites = self
            .database
            .get_sites_by_status(SiteStatus::Indexing)
            .await?;

        if let Some(site) = indexing_sites.first() {
            // Check if we're in the crawling phase or embedding phase
            let pending_crawl_items = self
                .database
                .get_pending_crawl_items_for_site(site.id)
                .await?;

            if !pending_crawl_items.is_empty() {
                return Ok(IndexingStatus::ProcessingSite {
                    site_id: site.id,
                    site_name: site.name.clone(),
                });
            }

            // Check for chunks waiting for embedding generation
            let completed_crawl_items = self
                .database
                .get_completed_crawl_items_for_site(site.id)
                .await?;

            let indexed_chunks = self.database.get_chunks_for_site(site.id).await?;

            let remaining_chunks = completed_crawl_items
                .len()
                .saturating_sub(indexed_chunks.len());

            if remaining_chunks > 0 {
                return Ok(IndexingStatus::GeneratingEmbeddings { remaining_chunks });
            }
        }

        Ok(IndexingStatus::Idle)
    }

    /// Main indexing loop with signal handling for graceful shutdown
    async fn run_indexing_loop_with_signals(&mut self) -> Result<()> {
        let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate())
            .context("Failed to register SIGTERM handler")?;
        let mut sigint = signal::unix::signal(signal::unix::SignalKind::interrupt())
            .context("Failed to register SIGINT handler")?;

        loop {
            select! {
                // Handle SIGTERM (graceful shutdown request)
                _ = sigterm.recv() => {
                    info!("Received SIGTERM, initiating graceful shutdown");
                    return Ok(());
                }
                // Handle SIGINT (Ctrl+C)
                _ = sigint.recv() => {
                    info!("Received SIGINT, initiating graceful shutdown");
                    return Ok(());
                }
                // Normal indexing operations
                result = self.process_next_site() => {
                    match result {
                        Ok(true) => {
                            // Successfully processed a site or made progress
                            sleep(Duration::from_millis(100)).await;
                        }
                        Ok(false) => {
                            // No work to do, exit gracefully
                            info!("No more work to process, shutting down indexer");
                            return Ok(());
                        }
                        Err(e) => {
                            error!("Error in indexing loop: {}", e);
                            sleep(Duration::from_secs(10)).await;
                        }
                    }
                }
            }
        }
    }

    /// Process the next site that needs indexing
    async fn process_next_site(&mut self) -> Result<bool> {
        // Get sites that need indexing (either pending or with completed crawl items)
        let sites = self.database.get_sites_needing_indexing().await?;

        for site in sites {
            match site.status {
                SiteStatus::Pending => {
                    // Site has been added but crawling hasn't started - skip for now
                    // Crawling should be handled by the crawler component
                }
                SiteStatus::Indexing => {
                    // Check if crawling is complete for this site
                    let pending_crawl_items = self
                        .database
                        .get_pending_crawl_items_for_site(site.id)
                        .await?;

                    if !pending_crawl_items.is_empty() {
                        // Crawling is still in progress - skip for now
                        continue;
                    }

                    // Crawling is complete, process embeddings
                    return self.process_site_embeddings(&site).await.map(|_| true);
                }
                SiteStatus::Completed | SiteStatus::Failed => {
                    // Nothing to do for completed or failed sites
                }
            }
        }

        Ok(false)
    }

    /// Process embeddings for a site where crawling is complete
    async fn process_site_embeddings(&mut self, site: &Site) -> Result<()> {
        info!("Processing embeddings for site: {}", site.name);

        // Get all completed crawl items that don't have indexed chunks yet
        let crawl_items = self
            .database
            .get_completed_crawl_items_for_site(site.id)
            .await?;

        let existing_chunks = self.database.get_chunks_for_site(site.id).await?;
        let existing_urls: std::collections::HashSet<String> =
            existing_chunks.into_iter().map(|c| c.url).collect();

        let items_to_process: Vec<CrawlQueueItem> = crawl_items
            .into_iter()
            .filter(|item| !existing_urls.contains(&item.url))
            .collect();

        if items_to_process.is_empty() {
            // All items have been processed, mark site as completed
            self.complete_site_indexing(site).await?;
            return Ok(());
        }

        info!("Processing {} pages for embeddings", items_to_process.len());

        let mut total_chunks_created = 0;
        let mut pages_processed = 0;

        for crawl_item in items_to_process {
            match self.process_single_page(&crawl_item, site.id).await {
                Ok(chunks_created) => {
                    total_chunks_created += chunks_created;
                    pages_processed += 1;

                    // Update site progress
                    let progress_update = SiteUpdate {
                        indexed_pages: Some(site.indexed_pages + pages_processed),
                        ..Default::default()
                    };
                    self.database.update_site(site.id, &progress_update).await?;
                }
                Err(e) => {
                    error!("Failed to process page {}: {}", crawl_item.url, e);
                    // Continue processing other pages
                }
            }
        }

        // Check if all pages are now processed
        let remaining_items = self
            .database
            .get_completed_crawl_items_for_site(site.id)
            .await?;

        let remaining_chunks = self.database.get_chunks_for_site(site.id).await?;

        if remaining_items.len() <= remaining_chunks.len() {
            self.complete_site_indexing(site).await?;
        }

        info!(
            "Processed {} pages, created {} chunks for site: {}",
            pages_processed, total_chunks_created, site.name
        );

        Ok(())
    }

    /// Process a single page for embedding generation
    async fn process_single_page(
        &mut self,
        crawl_item: &CrawlQueueItem,
        site_id: i64,
    ) -> Result<usize> {
        debug!("Processing page for embeddings: {}", crawl_item.url);

        // Get the extracted content for this URL
        // Note: In a real implementation, this would need to be stored
        // during crawling or re-extracted here
        let extracted_content = self.get_extracted_content_for_url(&crawl_item.url).await?;

        // Chunk the content
        let chunks = chunk_content(&extracted_content, &self.chunking_config)
            .context("Failed to chunk content")?;

        if chunks.is_empty() {
            debug!("No chunks generated for URL: {}", crawl_item.url);
            return Ok(0);
        }

        // Generate embeddings in batches
        let chunk_batches: Vec<Vec<ContentChunk>> = chunks
            .chunks(self.batch_size)
            .map(|batch| batch.to_vec())
            .collect();

        let mut total_chunks_processed = 0;

        for batch in chunk_batches {
            let batch_size = batch.len();

            // Generate embeddings for this batch
            let embedding_results = self
                .ollama_client
                .generate_chunk_embeddings(&batch)
                .context("Failed to generate embeddings")?;

            // Store embeddings and create indexed chunk records
            for (chunk, embedding_result) in batch.iter().zip(embedding_results.iter()) {
                let vector_id = Uuid::new_v4().to_string();

                // Create embedding record for LanceDB
                let embedding_record = EmbeddingRecord {
                    id: vector_id.clone(),
                    vector: embedding_result.embedding.clone(),
                    metadata: ChunkMetadata {
                        chunk_id: vector_id.clone(),
                        site_id: site_id.to_string(),
                        page_title: extracted_content.title.clone(),
                        page_url: crawl_item.url.clone(),
                        heading_path: Some(chunk.heading_path.clone()),
                        content: chunk.content.clone(),
                        token_count: chunk.token_count as u32,
                        chunk_index: chunk.chunk_index as u32,
                        created_at: Utc::now().to_rfc3339(),
                    },
                };

                // Store in LanceDB
                self.vector_store
                    .store_embeddings_batch(vec![embedding_record])
                    .await
                    .context("Failed to store embedding in LanceDB")?;

                // Create indexed chunk record for SQLite
                let indexed_chunk = NewIndexedChunk {
                    site_id,
                    url: crawl_item.url.clone(),
                    page_title: Some(extracted_content.title.clone()),
                    heading_path: Some(chunk.heading_path.clone()),
                    chunk_content: chunk.content.clone(),
                    chunk_index: chunk.chunk_index as i64,
                    vector_id: vector_id.clone(),
                };

                self.database
                    .insert_indexed_chunk(&indexed_chunk)
                    .await
                    .context("Failed to store indexed chunk in SQLite")?;
            }

            total_chunks_processed += batch_size;
            debug!(
                "Processed batch of {} chunks for URL: {}",
                batch_size, crawl_item.url
            );
        }

        Ok(total_chunks_processed)
    }

    /// Complete indexing for a site
    async fn complete_site_indexing(&mut self, site: &Site) -> Result<()> {
        info!("Completing indexing for site: {}", site.name);

        let update = SiteUpdate {
            status: Some(SiteStatus::Completed),
            indexed_date: Some(Utc::now().naive_utc()),
            progress_percent: Some(100),
            ..Default::default()
        };

        self.database
            .update_site(site.id, &update)
            .await
            .context("Failed to update site status to completed")?;

        // Optimize vector store for better search performance
        if let Err(e) = self.vector_store.optimize().await {
            warn!("Failed to optimize vector database: {}", e);
        }

        info!("Successfully completed indexing for site: {}", site.name);
        Ok(())
    }

    /// Get extracted content for a URL by re-extracting it
    async fn get_extracted_content_for_url(&self, url: &str) -> Result<ExtractedContent> {
        debug!("Re-extracting content for URL: {}", url);

        // Create HTTP client for content extraction
        let mut http_client =
            crate::crawler::HttpClient::new(crate::crawler::CrawlerConfig::default());

        // Fetch the page content
        let html_content = http_client
            .get(url)
            .await
            .context("Failed to fetch page content for extraction")?;

        // Extract content using the same extractor as the crawler
        let extraction_config = crate::crawler::extractor::ExtractionConfig::default();
        let extracted_content =
            crate::crawler::extractor::extract_content(&html_content, &extraction_config)
                .context("Failed to extract content from HTML")?;

        debug!(
            "Successfully extracted content from {}: {} sections, {} chars",
            url,
            extracted_content.sections.len(),
            extracted_content.raw_text.len()
        );

        Ok(extracted_content)
    }

    /// Validate consistency between SQLite and LanceDB
    #[inline]
    pub async fn validate_consistency(&mut self) -> Result<ConsistencyReport> {
        info!("Running database consistency validation");

        let mut validator = ConsistencyValidator::new(&self.database, &mut self.vector_store);

        validator.validate_consistency().await
    }

    /// Clean up database inconsistencies
    #[inline]
    pub async fn cleanup_inconsistencies(&mut self, report: &ConsistencyReport) -> Result<()> {
        if report.is_consistent {
            info!("Database is consistent, no cleanup needed");
            return Ok(());
        }

        info!("Cleaning up database inconsistencies");

        let mut validator = ConsistencyValidator::new(&self.database, &mut self.vector_store);

        // Clean up orphaned embeddings
        if !report.orphaned_in_lancedb.is_empty() {
            let cleaned = validator
                .cleanup_orphaned_embeddings(&report.orphaned_in_lancedb)
                .await?;
            info!("Cleaned up {} orphaned embeddings", cleaned);
        }

        // Regenerate missing embeddings
        if !report.missing_in_lancedb.is_empty() {
            let regenerated = validator
                .regenerate_missing_embeddings(&report.missing_in_lancedb)
                .await?;
            info!("Regenerated {} missing embeddings", regenerated);
        }

        info!("Database consistency cleanup completed");
        Ok(())
    }

    /// Start heartbeat task to indicate the indexer is alive
    fn start_heartbeat_task(&self) -> tokio::task::JoinHandle<()> {
        let database = self.database.clone();
        let interval = self.heartbeat_interval;

        tokio::spawn(async move {
            #[expect(
                clippy::infinite_loop,
                reason = "intended to run until handle is aborted"
            )]
            loop {
                if let Err(e) = database.update_indexer_heartbeat().await {
                    error!("Failed to update indexer heartbeat: {}", e);
                }
                sleep(interval).await;
            }
        })
    }

    /// Create lock file to prevent multiple indexers
    #[doc(hidden)]
    #[allow(
        clippy::missing_inline_in_public_items,
        reason = "only pub for testing purposes"
    )]
    pub async fn create_lock_file(&self) -> Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("current time is later than start of epoch")
            .as_secs();

        fs::write(&self.lock_file_path, timestamp.to_string())
            .await
            .context("Failed to create indexer lock file")?;

        Ok(())
    }

    /// Remove lock file on shutdown
    #[doc(hidden)]
    #[allow(
        clippy::missing_inline_in_public_items,
        reason = "only pub for testing purposes"
    )]
    pub async fn cleanup_lock_file(&self) -> Result<()> {
        if self.lock_file_path.exists() {
            fs::remove_file(&self.lock_file_path)
                .await
                .context("Failed to remove indexer lock file")?;
        }
        Ok(())
    }

    /// Get performance metrics for the indexing system
    #[inline]
    pub async fn get_performance_metrics(&self) -> Result<IndexingPerformanceMetrics> {
        use crate::database::sqlite::queries::SiteQueries;

        let sites = SiteQueries::list_all(self.database.pool()).await?;
        let mut metrics = IndexingPerformanceMetrics::default();

        // Calculate processing rates and statistics
        let mut total_pages_processed = 0;
        let mut total_chunks_created = 0;
        let mut total_processing_time = std::time::Duration::ZERO;

        for site in &sites {
            total_pages_processed += site.indexed_pages;

            if let Ok(Some(stats)) =
                SiteQueries::get_statistics(self.database.pool(), site.id).await
            {
                total_chunks_created += stats.total_chunks;
            }

            // Calculate processing time if site is completed
            if let (Some(created), Some(indexed)) = (Some(site.created_date), site.indexed_date) {
                let processing_duration = indexed.signed_duration_since(created);
                if processing_duration.num_seconds() > 0 {
                    total_processing_time +=
                        std::time::Duration::from_secs(processing_duration.num_seconds() as u64);
                }
            }
        }

        metrics.total_sites_processed = sites.len();
        metrics.total_pages_processed = total_pages_processed as usize;
        metrics.total_chunks_created = total_chunks_created as usize;
        metrics.average_processing_time_per_site = if sites.is_empty() {
            std::time::Duration::ZERO
        } else {
            total_processing_time / sites.len() as u32
        };

        // Calculate throughput metrics
        if total_processing_time.as_secs() > 0 {
            metrics.pages_per_minute =
                (total_pages_processed as f64 * 60.0) / total_processing_time.as_secs() as f64;
            metrics.chunks_per_minute =
                (total_chunks_created as f64 * 60.0) / total_processing_time.as_secs() as f64;
        }

        // Resource usage estimates
        metrics.database_size_mb = self.get_database_size().await?;

        Ok(metrics)
    }

    /// Optimize indexer performance by cleaning up and reorganizing data
    #[inline]
    pub async fn optimize_performance(&mut self) -> Result<String> {
        info!("Starting indexer performance optimization");
        let mut optimizations = Vec::new();

        // Database optimization
        if let Err(e) = self.database.optimize().await {
            warn!("Database optimization failed: {}", e);
        } else {
            optimizations.push("Database optimized".to_string());
        }

        // Vector store optimization
        if let Err(e) = self.vector_store.optimize().await {
            warn!("Vector store optimization failed: {}", e);
        } else {
            optimizations.push("Vector store optimized".to_string());
        }

        // Queue resource cleanup and optimization
        self.queue_manager.cleanup_resources();
        optimizations.push("Queue resources cleaned up".to_string());

        // Clean up old queue items
        let cleaned_items = self.queue_manager.cleanup_old_items(None).await?;
        if cleaned_items > 0 {
            optimizations.push(format!("Cleaned up {} old queue items", cleaned_items));
        }

        // Reset stuck queue items
        let reset_items = self.queue_manager.reset_stuck_items().await?;
        if reset_items > 0 {
            optimizations.push(format!("Reset {} stuck queue items", reset_items));
        }

        // Optimize queue performance
        let queue_optimization = self.queue_manager.optimize_queue().await?;
        optimizations.push(format!("Queue optimization: {}", queue_optimization));

        // Get queue resource usage for logging
        let queue_usage = self.queue_manager.get_resource_usage();
        info!(
            "Queue resource usage after cleanup: {} processing items, {:.2} MB memory",
            queue_usage.processing_items_tracked, queue_usage.estimated_memory_usage_mb
        );

        // Consistency validation and cleanup
        let consistency_report = self.validate_consistency().await?;
        if !consistency_report.is_consistent {
            self.cleanup_inconsistencies(&consistency_report).await?;
            optimizations.push("Database consistency issues resolved".to_string());
        }

        let summary = if optimizations.is_empty() {
            "System is already optimized".to_string()
        } else {
            format!("Optimizations applied: {}", optimizations.join(", "))
        };

        info!("Performance optimization completed: {}", summary);
        Ok(summary)
    }

    /// Get total database size in MB
    async fn get_database_size(&self) -> Result<f64> {
        let db_path = self.config.database_path();
        let metadata = tokio::fs::metadata(&db_path)
            .await
            .context("Failed to get database file metadata")?;

        let size_mb = metadata.len() as f64 / 1024.0 / 1024.0;
        Ok(size_mb)
    }

    /// Get queue resource usage statistics
    #[inline]
    pub fn get_queue_resource_usage(&self) -> QueueResourceUsage {
        self.queue_manager.get_resource_usage()
    }
}

impl Drop for BackgroundIndexer {
    #[inline]
    fn drop(&mut self) {
        // Best effort cleanup on drop
        let lock_file_path = self.lock_file_path.clone();
        tokio::spawn(async move {
            let _ = fs::remove_file(lock_file_path).await;
        });
    }
}
