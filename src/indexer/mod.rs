// Indexer module
// This module handles background process coordination and queue management

pub mod consistency;

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use chrono::Utc;
use tokio::fs;
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

/// Background indexer that processes crawled content into searchable embeddings
pub struct BackgroundIndexer {
    #[expect(dead_code)]
    config: Config,
    database: Database,
    vector_store: VectorStore,
    ollama_client: OllamaClient,
    chunking_config: ChunkingConfig,
    lock_file_path: PathBuf,
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

        let lock_file_path = config.config_dir_path().join(".indexer.lock");

        Ok(Self {
            config,
            database,
            vector_store,
            ollama_client,
            chunking_config: ChunkingConfig::default(),
            lock_file_path,
            heartbeat_interval: Duration::from_secs(30),
            batch_size: 64,
        })
    }

    /// Start the background indexing process
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

        // Main indexing loop
        let result = self.run_indexing_loop().await;

        // Stop heartbeat and cleanup
        heartbeat_handle.abort();
        let _ = self.cleanup_lock_file().await;

        result
    }

    /// Check if an indexer is currently running
    #[inline]
    pub async fn is_indexer_running(&self) -> Result<bool> {
        if !self.lock_file_path.exists() {
            return Ok(false);
        }

        // Check if the process is still alive by examining heartbeat
        match self.database.get_indexer_heartbeat().await {
            Ok(Some(heartbeat)) => {
                let now = Utc::now().naive_utc();
                let elapsed = now
                    .signed_duration_since(heartbeat)
                    .num_seconds()
                    .unsigned_abs();

                // Consider stale if no heartbeat for 2 minutes
                Ok(elapsed < 120)
            }
            Ok(None) => Ok(false),
            Err(_) => Ok(false),
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

    /// Main indexing loop
    async fn run_indexing_loop(&mut self) -> Result<()> {
        loop {
            match self.process_next_site().await {
                Ok(true) => {
                    // Successfully processed a site or made progress
                    sleep(Duration::from_millis(100)).await;
                }
                Ok(false) => {
                    // No work to do, exit
                    return Ok(());
                }
                Err(e) => {
                    error!("Error in indexing loop: {}", e);
                    sleep(Duration::from_secs(10)).await;
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
                    let _ = self.database.update_site(site.id, &progress_update).await;
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
    async fn create_lock_file(&self) -> Result<()> {
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
    async fn cleanup_lock_file(&self) -> Result<()> {
        if self.lock_file_path.exists() {
            fs::remove_file(&self.lock_file_path)
                .await
                .context("Failed to remove indexer lock file")?;
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OllamaConfig;
    use std::env;
    use tempfile::TempDir;

    async fn create_test_indexer() -> Result<(BackgroundIndexer, TempDir)> {
        let temp_dir = TempDir::new()?;
        let config = Config {
            ollama: OllamaConfig {
                host: "localhost".to_string(),
                port: 11434,
                model: "nomic-embed-text:latest".to_string(),
                batch_size: 32,
            },
            base_dir: Some(temp_dir.path().to_path_buf()),
        };

        let indexer = BackgroundIndexer::new(config).await?;
        Ok((indexer, temp_dir))
    }

    #[tokio::test]
    async fn indexer_creation() {
        if env::var("SKIP_OLLAMA_TESTS").is_ok() {
            return;
        }

        let result = create_test_indexer().await;
        assert!(result.is_ok(), "Should create indexer successfully");
    }

    #[tokio::test]
    async fn lock_file_operations() {
        if env::var("SKIP_OLLAMA_TESTS").is_ok() {
            return;
        }

        let (indexer, _temp_dir) = create_test_indexer().await.expect("can create indexer");

        // Initially no lock file should exist
        assert!(
            !indexer
                .is_indexer_running()
                .await
                .expect("can get indexer status")
        );

        // Create lock file
        indexer
            .create_lock_file()
            .await
            .expect("can create lock file");
        assert!(indexer.lock_file_path.exists());

        // Cleanup lock file
        indexer
            .cleanup_lock_file()
            .await
            .expect("can cleanup lock file");
        assert!(!indexer.lock_file_path.exists());
    }

    #[tokio::test]
    async fn indexing_status() {
        if env::var("SKIP_OLLAMA_TESTS").is_ok() {
            return;
        }

        let (indexer, _temp_dir) = create_test_indexer().await.expect("can create indexer");

        // Should start as idle
        let status = indexer
            .get_indexing_status()
            .await
            .expect("can get indexer status");
        assert_eq!(status, IndexingStatus::Idle);
    }
}
