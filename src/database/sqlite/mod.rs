use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};
use std::path::Path;
use tracing::{debug, info};

use crate::database::sqlite::models::{
    CrawlQueueItem, IndexedChunk, NewIndexedChunk, Site, SiteStatus, SiteUpdate,
};
use crate::database::sqlite::queries::{CrawlQueueQueries, IndexedChunkQueries, SiteQueries};

#[cfg(test)]
mod tests;

pub mod models;
pub mod queries;

pub type DbPool = Pool<Sqlite>;

#[derive(Debug, Clone)]
pub struct Database {
    pool: DbPool,
}

impl Database {
    pub async fn new<P: AsRef<Path>>(database_url: P) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(database_url)
            .create_if_missing(true)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(10)
            .connect_with(options)
            .await
            .context("Failed to create database connection pool")?;

        let database = Self { pool };
        database.run_migrations().await?;

        Ok(database)
    }

    pub fn pool(&self) -> &DbPool {
        &self.pool
    }

    pub async fn run_migrations(&self) -> Result<()> {
        info!("Running database migrations");

        sqlx::migrate!("src/database/sqlite/migrations")
            .run(&self.pool)
            .await
            .context("Failed to run schema migration")?;

        debug!("Database migrations completed successfully");
        Ok(())
    }

    pub async fn initialize_from_config_dir(config_dir: &Path) -> Result<Self> {
        let db_path = config_dir.join("metadata.db");
        let db_url = db_path.to_string_lossy();

        std::fs::create_dir_all(config_dir).with_context(|| {
            format!(
                "Failed to create config directory: {}",
                config_dir.display()
            )
        })?;

        Self::new(db_url.as_ref()).await
    }

    // Site operations
    pub async fn get_sites_by_status(&self, status: SiteStatus) -> Result<Vec<Site>> {
        SiteQueries::get_sites_by_status(&self.pool, status).await
    }

    pub async fn update_site(&self, id: i64, update: &SiteUpdate) -> Result<Option<Site>> {
        SiteQueries::update(&self.pool, id, update.clone()).await
    }

    pub async fn list_sites(&self) -> Result<Vec<Site>> {
        SiteQueries::list_all(&self.pool).await
    }

    pub async fn get_site_by_id(&self, id: i64) -> Result<Option<Site>> {
        SiteQueries::get_by_id(&self.pool, id).await
    }

    // Crawl queue operations
    pub async fn get_completed_crawl_items_for_site(
        &self,
        site_id: i64,
    ) -> Result<Vec<CrawlQueueItem>> {
        CrawlQueueQueries::get_completed_for_site(&self.pool, site_id).await
    }

    // Indexed chunk operations
    pub async fn get_chunks_for_site(&self, site_id: i64) -> Result<Vec<IndexedChunk>> {
        IndexedChunkQueries::list_by_site(&self.pool, site_id).await
    }

    pub async fn insert_indexed_chunk(&self, chunk: &NewIndexedChunk) -> Result<IndexedChunk> {
        IndexedChunkQueries::create(&self.pool, chunk.clone()).await
    }

    pub async fn get_chunk_by_vector_id(&self, vector_id: &str) -> Result<Option<IndexedChunk>> {
        IndexedChunkQueries::get_by_vector_id(&self.pool, vector_id).await
    }

    /// Optimize database performance by running VACUUM and ANALYZE
    pub async fn optimize(&self) -> Result<()> {
        info!("Optimizing database performance");

        // Run VACUUM to reclaim space and defragment
        sqlx::query("VACUUM")
            .execute(&self.pool)
            .await
            .context("Failed to vacuum database")?;

        // Run ANALYZE to update table statistics for better query planning
        sqlx::query("ANALYZE")
            .execute(&self.pool)
            .await
            .context("Failed to analyze database")?;

        debug!("Database optimization completed");
        Ok(())
    }
}
