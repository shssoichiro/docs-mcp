use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Pool, Sqlite};
use std::path::Path;
use tracing::{debug, info};

pub mod models;
pub mod queries;

#[cfg(test)]
mod tests;

pub use models::*;
pub use queries::*;

pub type DbPool = Pool<Sqlite>;

#[derive(Debug, Clone)]
pub struct Database {
    pool: DbPool,
}

impl Database {
    #[inline]
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

    #[inline]
    pub fn pool(&self) -> &DbPool {
        &self.pool
    }

    #[inline]
    pub async fn run_migrations(&self) -> Result<()> {
        info!("Running database migrations");

        sqlx::migrate!("src/database/sqlite/migrations")
            .run(&self.pool)
            .await
            .context("Failed to run schema migration")?;

        debug!("Database migrations completed successfully");
        Ok(())
    }

    #[inline]
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

    #[inline]
    pub async fn begin_transaction(&self) -> Result<sqlx::Transaction<'_, Sqlite>> {
        self.pool
            .begin()
            .await
            .context("Failed to begin database transaction")
    }

    // Site operations
    #[inline]
    pub async fn get_sites_by_status(&self, status: SiteStatus) -> Result<Vec<Site>> {
        SiteQueries::get_sites_by_status(&self.pool, status).await
    }

    #[inline]
    pub async fn get_sites_needing_indexing(&self) -> Result<Vec<Site>> {
        SiteQueries::get_sites_needing_indexing(&self.pool).await
    }

    #[inline]
    pub async fn update_site(&self, id: i64, update: &SiteUpdate) -> Result<Option<Site>> {
        SiteQueries::update(&self.pool, id, update.clone()).await
    }

    #[inline]
    pub async fn list_sites(&self) -> Result<Vec<Site>> {
        SiteQueries::list_all(&self.pool).await
    }

    #[inline]
    pub async fn get_site_by_name(&self, name: &str) -> Result<Option<Site>> {
        // For simplicity, we'll get the first site with matching name
        // In a real scenario, we might need version handling too
        let sites = SiteQueries::list_all(&self.pool).await?;
        Ok(sites.into_iter().find(|site| site.name == name))
    }

    #[inline]
    pub async fn get_site_by_id(&self, id: i64) -> Result<Option<Site>> {
        SiteQueries::get_by_id(&self.pool, id).await
    }

    // Crawl queue operations
    #[inline]
    pub async fn get_pending_crawl_items_for_site(
        &self,
        site_id: i64,
    ) -> Result<Vec<CrawlQueueItem>> {
        CrawlQueueQueries::get_pending_for_site(&self.pool, site_id).await
    }

    #[inline]
    pub async fn get_completed_crawl_items_for_site(
        &self,
        site_id: i64,
    ) -> Result<Vec<CrawlQueueItem>> {
        CrawlQueueQueries::get_completed_for_site(&self.pool, site_id).await
    }

    // Indexed chunk operations
    #[inline]
    pub async fn get_chunks_for_site(&self, site_id: i64) -> Result<Vec<IndexedChunk>> {
        IndexedChunkQueries::list_by_site(&self.pool, site_id).await
    }

    #[inline]
    pub async fn insert_indexed_chunk(&self, chunk: &NewIndexedChunk) -> Result<IndexedChunk> {
        IndexedChunkQueries::create(&self.pool, chunk.clone()).await
    }

    #[inline]
    pub async fn get_chunk_by_vector_id(&self, vector_id: &str) -> Result<Option<IndexedChunk>> {
        IndexedChunkQueries::get_by_vector_id(&self.pool, vector_id).await
    }

    /// Optimize database performance by running VACUUM and ANALYZE
    #[inline]
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
