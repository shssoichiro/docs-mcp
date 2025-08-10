// Indexer module
// This module handles background process coordination and queue management

pub mod consistency;

#[cfg(test)]
mod tests;

use std::fs::{self, File};

use anyhow::{Context, Result};
use chrono::Utc;
use indicatif::{ProgressBar, ProgressStyle};
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::config::Config;
use crate::crawler::extractor::ExtractedContent;
use crate::database::lancedb::vector_store::VectorStore;
use crate::database::lancedb::{ChunkMetadata, EmbeddingRecord};
use crate::database::sqlite::Database;
use crate::database::sqlite::models::{
    CrawlQueueItem, NewIndexedChunk, Site, SiteStatus, SiteUpdate,
};
use crate::embeddings::chunking::{ChunkingConfig, ContentChunk, chunk_content};
use crate::embeddings::ollama::OllamaClient;
use crate::indexer::consistency::{ConsistencyReport, ConsistencyValidator};

/// Indexer that processes crawled content into searchable embeddings
pub struct Indexer {
    database: Database,
    vector_store: VectorStore,
    ollama_client: OllamaClient,
    chunking_config: ChunkingConfig,
    app_config: Config,
    batch_size: usize,
    verbose: bool,
}

impl Indexer {
    /// Create a new indexer
    #[inline]
    pub async fn new(config: Config, verbose: bool) -> Result<Self> {
        let database = Database::new(config.database_path()?)
            .await
            .context("Failed to initialize SQLite database")?;

        let vector_store = VectorStore::new(&config)
            .await
            .context("Failed to initialize LanceDB vector store")?;

        let ollama_client = OllamaClient::new(config.ollama.clone())
            .context("Failed to initialize Ollama client")?;

        Ok(Self {
            database,
            vector_store,
            ollama_client,
            chunking_config: config.chunking,
            app_config: config,
            batch_size: 64,
            verbose,
        })
    }

    /// Process embeddings for a site where crawling is complete
    #[inline]
    pub async fn process_site_embeddings(&mut self, site: &Site) -> Result<()> {
        // TODO: Implement per-site multiple instance checking

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

        eprintln!("Processing {} pages for embeddings", items_to_process.len());

        let mut total_chunks_created = 0;
        let mut pages_processed = 0;

        let bar = if console::user_attended_stderr() {
            ProgressBar::new_spinner().with_style(
                ProgressStyle::with_template(
                    "{spinner} [{pos}/{len}] Creating embeddings for {msg}",
                )
                .expect("style template is valid"),
            )
        } else {
            ProgressBar::hidden()
        };
        bar.set_position(0);
        bar.set_length(items_to_process.len() as u64);

        for crawl_item in items_to_process {
            bar.set_message(crawl_item.url.clone());
            match self.process_single_page(&crawl_item, site.id, &bar).await {
                Ok(chunks_created) => {
                    total_chunks_created += chunks_created;
                    pages_processed += 1;
                    bar.set_position(pages_processed);

                    // Update site progress
                    let progress_update = SiteUpdate {
                        indexed_pages: Some(site.indexed_pages + pages_processed as i64),
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
        bar.finish_and_clear();

        eprintln!(
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
        bar: &ProgressBar,
    ) -> Result<usize> {
        debug!("Processing page for embeddings: {}", crawl_item.url);

        // Get the extracted content for this URL
        let extracted_content = self.get_extracted_content_for_page(crawl_item.id)?;

        // Chunk the content
        if self.verbose {
            bar.set_message(format!("{} (Chunking content)", crawl_item.url));
        }
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
            if self.verbose {
                bar.set_message(format!(
                    "{} (Generating embeddings {}-{} of {})",
                    crawl_item.url,
                    total_chunks_processed + 1,
                    total_chunks_processed + batch.len(),
                    chunks.len()
                ));
            }
            let embedding_results = self
                .ollama_client
                .generate_chunk_embeddings(&batch)
                .context("Failed to generate embeddings")?;

            // Store embeddings and create indexed chunk records
            for (i, (chunk, embedding_result)) in batch
                .into_iter()
                .zip(embedding_results.into_iter())
                .enumerate()
            {
                if self.verbose {
                    bar.set_message(format!(
                        "{} (Saving embeddings for chunk {} of {})",
                        crawl_item.url,
                        total_chunks_processed + i + 1,
                        chunks.len()
                    ));
                }

                let vector_id = Uuid::new_v4().to_string();

                // Create embedding record for LanceDB
                let embedding_record = EmbeddingRecord {
                    id: vector_id.clone(),
                    vector: embedding_result.embedding,
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
                    heading_path: Some(chunk.heading_path),
                    chunk_content: chunk.content,
                    chunk_index: chunk.chunk_index as i64,
                    vector_id,
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

        if self.verbose {
            bar.set_message(format!("{} (Finalizing)", crawl_item.url));
        }

        self.remove_cached_page(crawl_item.id)?;

        Ok(total_chunks_processed)
    }

    /// Complete indexing for a site
    async fn complete_site_indexing(&self, site: &Site) -> Result<()> {
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

    /// Get extracted content for a page from the cached file
    fn get_extracted_content_for_page(&self, page_id: i64) -> Result<ExtractedContent> {
        let cached_file_path = self
            .app_config
            .cache_dir_path()?
            .join("pages")
            .join(format!("{page_id}.json"));
        let extracted_content: ExtractedContent = serde_json::from_reader(
            File::open(&cached_file_path).context("Failed to open cached page file")?,
        )
        .context("Failed to read cached page file")?;

        debug!(
            "Loaded extracted content for page {}: {} sections, {} chars",
            page_id,
            extracted_content.sections.len(),
            extracted_content.raw_text.len()
        );

        Ok(extracted_content)
    }

    /// Cleanup extracted content for a page after we are finished
    fn remove_cached_page(&self, page_id: i64) -> Result<()> {
        let cached_file_path = self
            .app_config
            .cache_dir_path()?
            .join("pages")
            .join(format!("{page_id}.json"));
        let _ = fs::remove_file(cached_file_path);

        Ok(())
    }

    /// Validate consistency between SQLite and LanceDB
    #[inline]
    pub async fn validate_consistency(&mut self) -> Result<ConsistencyReport> {
        info!("Running database consistency validation");

        let validator = ConsistencyValidator::new(&self.database, &mut self.vector_store);

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

        let validator = ConsistencyValidator::new(&self.database, &mut self.vector_store);

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
}
