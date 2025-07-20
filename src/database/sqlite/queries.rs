use super::models::*;
use anyhow::{Context, Result};
use chrono::Utc;
use sqlx::SqlitePool;
use tracing::{debug, warn};

pub struct SiteQueries;

impl SiteQueries {
    #[inline]
    pub async fn create(pool: &SqlitePool, new_site: NewSite) -> Result<Site> {
        let now = Utc::now();
        let id = sqlx::query!(
            "INSERT INTO sites (base_url, name, version, status, created_date) VALUES (?, ?, ?, 'pending', ?)",
            new_site.base_url,
            new_site.name,
            new_site.version,
            now
        )
        .execute(pool)
        .await
        .context("Failed to create site")?
        .last_insert_rowid();

        Self::get_by_id(pool, id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve created site"))
    }

    #[inline]
    pub async fn get_by_id(pool: &SqlitePool, id: i64) -> Result<Option<Site>> {
        let result = sqlx::query_as!(
            Site,
            r#"
            SELECT id,
                   base_url,
                   name,
                   version,
                   indexed_date,
                   status as "status: SiteStatus",
                   progress_percent,
                   total_pages,
                   indexed_pages,
                   error_message,
                   created_date,
                   last_heartbeat
            FROM sites WHERE id = ?
            "#,
            id
        )
        .fetch_optional(pool)
        .await
        .context("Failed to get site by id")?;

        Ok(result)
    }

    #[inline]
    pub async fn get_by_name_and_version(
        pool: &SqlitePool,
        name: &str,
        version: &str,
    ) -> Result<Option<Site>> {
        let result = sqlx::query_as!(
            Site,
            r#"
            SELECT id,
                   base_url,
                   name,
                   version,
                   indexed_date,
                   status as "status: SiteStatus",
                   progress_percent,
                   total_pages,
                   indexed_pages,
                   error_message,
                   created_date,
                   last_heartbeat
            FROM sites WHERE name = ? AND version = ?
            "#,
            name,
            version
        )
        .fetch_optional(pool)
        .await
        .context("Failed to get site by name and version")?;

        Ok(result)
    }

    #[inline]
    pub async fn get_by_base_url(pool: &SqlitePool, base_url: &str) -> Result<Option<Site>> {
        let result = sqlx::query_as!(
            Site,
            r#"
            SELECT id,
                   base_url, 
                   name, 
                   version, 
                   indexed_date,
                   status as "status: SiteStatus",
                   progress_percent,
                   total_pages,
                   indexed_pages,
                   error_message,
                   created_date,
                   last_heartbeat
            FROM sites WHERE base_url = ?
            "#,
            base_url
        )
        .fetch_optional(pool)
        .await
        .context("Failed to get site by base URL")?;

        Ok(result)
    }

    #[inline]
    pub async fn list_all(pool: &SqlitePool) -> Result<Vec<Site>> {
        let sites = sqlx::query_as!(
            Site,
            r#"
            SELECT id,
                   base_url, 
                   name, 
                   version, 
                   indexed_date,
                   status as "status: SiteStatus",
                   progress_percent,
                   total_pages,
                   indexed_pages,
                   error_message,
                   created_date,
                   last_heartbeat
            FROM sites ORDER BY created_date DESC
            "#
        )
        .fetch_all(pool)
        .await
        .context("Failed to list all sites")?;

        Ok(sites)
    }

    #[inline]
    pub async fn list_completed(pool: &SqlitePool) -> Result<Vec<Site>> {
        let sites = sqlx::query_as!(
            Site,
            r#"
            SELECT id,
                   base_url, 
                   name, 
                   version, 
                   indexed_date,
                   status as "status: SiteStatus",
                   progress_percent,
                   total_pages,
                   indexed_pages,
                   error_message,
                   created_date,
                   last_heartbeat
            FROM sites WHERE status = 'completed' ORDER BY indexed_date DESC
            "#
        )
        .fetch_all(pool)
        .await
        .context("Failed to list completed sites")?;

        Ok(sites)
    }

    #[inline]
    pub async fn update(pool: &SqlitePool, id: i64, update: SiteUpdate) -> Result<Option<Site>> {
        let mut query_parts = Vec::new();
        let mut query_values = Vec::new();

        if let Some(status) = update.status {
            query_parts.push("status = ?");
            let status_str = match status {
                SiteStatus::Pending => "pending",
                SiteStatus::Indexing => "indexing",
                SiteStatus::Completed => "completed",
                SiteStatus::Failed => "failed",
            };
            query_values.push(status_str.to_string());
        }

        if let Some(progress) = update.progress_percent {
            query_parts.push("progress_percent = ?");
            query_values.push(progress.to_string());
        }

        if let Some(total) = update.total_pages {
            query_parts.push("total_pages = ?");
            query_values.push(total.to_string());
        }

        if let Some(indexed) = update.indexed_pages {
            query_parts.push("indexed_pages = ?");
            query_values.push(indexed.to_string());
        }

        if let Some(error) = update.error_message {
            query_parts.push("error_message = ?");
            query_values.push(error);
        }

        if let Some(heartbeat) = update.last_heartbeat {
            query_parts.push("last_heartbeat = ?");
            query_values.push(heartbeat.to_string());
        }

        if let Some(indexed_date) = update.indexed_date {
            query_parts.push("indexed_date = ?");
            query_values.push(indexed_date.to_string());
        }

        if query_parts.is_empty() {
            return Self::get_by_id(pool, id).await;
        }

        let query_str = format!("UPDATE sites SET {} WHERE id = ?", query_parts.join(", "));

        let mut query = sqlx::query(&query_str);
        for value in query_values {
            query = query.bind(value);
        }
        query = query.bind(id);

        query.execute(pool).await.context("Failed to update site")?;

        Self::get_by_id(pool, id).await
    }

    #[inline]
    pub async fn delete(pool: &SqlitePool, id: i64) -> Result<bool> {
        let result = sqlx::query!("DELETE FROM sites WHERE id = ?", id)
            .execute(pool)
            .await
            .context("Failed to delete site")?;

        Ok(result.rows_affected() > 0)
    }

    #[inline]
    pub async fn get_statistics(pool: &SqlitePool, site_id: i64) -> Result<Option<SiteStatistics>> {
        let Some(site) = Self::get_by_id(pool, site_id).await? else {
            return Ok(None);
        };

        let chunk_count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM indexed_chunks WHERE site_id = ?",
            site_id
        )
        .fetch_one(pool)
        .await
        .context("Failed to get chunk count")?;

        let pending_crawl = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM crawl_queue WHERE site_id = ? AND status = 'pending'",
            site_id
        )
        .fetch_one(pool)
        .await
        .context("Failed to get pending crawl count")?;

        let failed_crawl = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM crawl_queue WHERE site_id = ? AND status = 'failed'",
            site_id
        )
        .fetch_one(pool)
        .await
        .context("Failed to get failed crawl count")?;

        Ok(Some(SiteStatistics {
            site,
            total_chunks: chunk_count,
            pending_crawl_items: pending_crawl,
            failed_crawl_items: failed_crawl,
        }))
    }
}

pub struct CrawlQueueQueries;

impl CrawlQueueQueries {
    #[inline]
    pub async fn add_url(pool: &SqlitePool, new_item: NewCrawlQueueItem) -> Result<CrawlQueueItem> {
        let now = Utc::now();
        let id = sqlx::query!(
            r#"
            INSERT INTO crawl_queue (site_id, url, status, created_date)
            VALUES (?, ?, 'pending', ?)
            ON CONFLICT(site_id, url) DO UPDATE SET status = 'pending'
            "#,
            new_item.site_id,
            new_item.url,
            now
        )
        .execute(pool)
        .await
        .context("Failed to add URL to crawl queue")?
        .last_insert_rowid();

        Self::get_by_id(pool, id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve created crawl queue item"))
    }

    #[inline]
    pub async fn add_urls_batch(
        pool: &SqlitePool,
        site_id: i64,
        urls: Vec<String>,
    ) -> Result<usize> {
        let mut transaction = pool
            .begin()
            .await
            .context("Failed to begin transaction for batch URL insert")?;

        let mut inserted_count = 0;
        let now = Utc::now();

        for url in urls {
            let result = sqlx::query(
                r#"
                INSERT INTO crawl_queue (site_id, url, status, created_date)
                VALUES (?, ?, 'pending', ?)
                ON CONFLICT(site_id, url) DO NOTHING
                "#,
            )
            .bind(site_id)
            .bind(&url)
            .bind(now)
            .execute(&mut *transaction)
            .await;

            match result {
                Ok(query_result) => {
                    if query_result.rows_affected() > 0 {
                        inserted_count += 1;
                    }
                }
                Err(e) => {
                    warn!("Failed to insert URL {}: {}", url, e);
                }
            }
        }

        transaction
            .commit()
            .await
            .context("Failed to commit batch URL insert transaction")?;

        debug!(
            "Inserted {} URLs into crawl queue for site {}",
            inserted_count, site_id
        );
        Ok(inserted_count)
    }

    #[inline]
    pub async fn get_next_pending(
        pool: &SqlitePool,
        site_id: i64,
    ) -> Result<Option<CrawlQueueItem>> {
        let result = sqlx::query_as!(
            CrawlQueueItem,
            r#"
            SELECT id,
                   site_id,
                   url, 
                   status as "status: CrawlStatus",
                   retry_count,
                   error_message, 
                   created_date
            FROM crawl_queue 
            WHERE site_id = ? AND (status = 'pending' OR (status = 'failed' AND retry_count < 3))
            ORDER BY created_date ASC
            LIMIT 1
            "#,
            site_id
        )
        .fetch_optional(pool)
        .await
        .context("Failed to get next pending crawl item")?;

        Ok(result)
    }

    #[inline]
    pub async fn update_status(
        pool: &SqlitePool,
        id: i64,
        update: CrawlQueueUpdate,
    ) -> Result<Option<CrawlQueueItem>> {
        let mut query_parts = Vec::new();
        let mut query_values = Vec::new();

        if let Some(status) = update.status {
            query_parts.push("status = ?");
            let status_str = match status {
                CrawlStatus::Pending => "pending",
                CrawlStatus::Processing => "processing",
                CrawlStatus::Completed => "completed",
                CrawlStatus::Failed => "failed",
            };
            query_values.push(status_str.to_string());
        }

        if let Some(retry_count) = update.retry_count {
            query_parts.push("retry_count = ?");
            query_values.push(retry_count.to_string());
        }

        if let Some(error) = update.error_message {
            query_parts.push("error_message = ?");
            query_values.push(error);
        }

        if query_parts.is_empty() {
            return Self::get_by_id(pool, id).await;
        }

        let query_str = format!(
            "UPDATE crawl_queue SET {} WHERE id = ?",
            query_parts.join(", ")
        );

        let mut query = sqlx::query(&query_str);
        for value in query_values {
            query = query.bind(value);
        }
        query = query.bind(id);

        query
            .execute(pool)
            .await
            .context("Failed to update crawl queue item")?;

        Self::get_by_id(pool, id).await
    }

    #[inline]
    pub async fn get_by_id(pool: &SqlitePool, id: i64) -> Result<Option<CrawlQueueItem>> {
        let result = sqlx::query_as!(
            CrawlQueueItem,
            r#"
            SELECT id,
                   site_id,
                   url, 
                   status as "status: CrawlStatus",
                   retry_count,
                   error_message,
                   created_date
            FROM crawl_queue WHERE id = ?
            "#,
            id
        )
        .fetch_optional(pool)
        .await
        .context("Failed to get crawl queue item by id")?;

        Ok(result)
    }

    #[inline]
    pub async fn delete_completed_for_site(pool: &SqlitePool, site_id: i64) -> Result<usize> {
        let result = sqlx::query!(
            "DELETE FROM crawl_queue WHERE site_id = ? AND status = 'completed'",
            site_id
        )
        .execute(pool)
        .await
        .context("Failed to delete completed crawl queue items")?;

        Ok(result.rows_affected() as usize)
    }
}

pub struct IndexedChunkQueries;

impl IndexedChunkQueries {
    #[inline]
    pub async fn create(pool: &SqlitePool, new_chunk: NewIndexedChunk) -> Result<IndexedChunk> {
        let now = Utc::now();
        let id = sqlx::query!(
            r#"
            INSERT INTO indexed_chunks (site_id, url, page_title, heading_path, chunk_content, chunk_index, vector_id, indexed_date)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
            new_chunk.site_id,
            new_chunk.url,
            new_chunk.page_title,
            new_chunk.heading_path,
            new_chunk.chunk_content,
            new_chunk.chunk_index,
            new_chunk.vector_id,
            now
        )
        .execute(pool)
        .await
        .context("Failed to create indexed chunk")?
        .last_insert_rowid();

        Self::get_by_id(pool, id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Failed to retrieve created chunk"))
    }

    #[inline]
    pub async fn create_batch(
        pool: &SqlitePool,
        chunks: Vec<NewIndexedChunk>,
    ) -> Result<Vec<IndexedChunk>> {
        if chunks.is_empty() {
            return Ok(Vec::new());
        }

        let mut transaction = pool
            .begin()
            .await
            .context("Failed to begin transaction for batch chunk insert")?;

        let mut created_chunks = Vec::new();
        let now = Utc::now().naive_utc();

        for chunk in chunks {
            let id = sqlx::query(
                r#"
                INSERT INTO indexed_chunks (site_id, url, page_title, heading_path, chunk_content, chunk_index, vector_id, indexed_date)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                "#
            )
            .bind(chunk.site_id)
            .bind(&chunk.url)
            .bind(&chunk.page_title)
            .bind(&chunk.heading_path)
            .bind(&chunk.chunk_content)
            .bind(chunk.chunk_index)
            .bind(&chunk.vector_id)
            .bind(now)
            .execute(&mut *transaction)
            .await
            .context("Failed to create indexed chunk in batch")?
            .last_insert_rowid();

            created_chunks.push(IndexedChunk {
                id,
                site_id: chunk.site_id,
                url: chunk.url,
                page_title: chunk.page_title,
                heading_path: chunk.heading_path,
                chunk_content: chunk.chunk_content,
                chunk_index: chunk.chunk_index,
                vector_id: chunk.vector_id,
                indexed_date: now,
            });
        }

        transaction
            .commit()
            .await
            .context("Failed to commit batch chunk insert transaction")?;

        debug!("Created {} indexed chunks", created_chunks.len());
        Ok(created_chunks)
    }

    #[inline]
    pub async fn get_by_vector_id(
        pool: &SqlitePool,
        vector_id: &str,
    ) -> Result<Option<IndexedChunk>> {
        let result = sqlx::query_as!(
            IndexedChunk,
            r#"
            SELECT id,
                   site_id,
                   url, 
                   page_title, 
                   heading_path, 
                   chunk_content, 
                   chunk_index,
                   vector_id, 
                   indexed_date
            FROM indexed_chunks WHERE vector_id = ?
            "#,
            vector_id
        )
        .fetch_optional(pool)
        .await
        .context("Failed to get indexed chunk by vector id")?;

        Ok(result)
    }

    #[inline]
    pub async fn get_by_id(pool: &SqlitePool, id: i64) -> Result<Option<IndexedChunk>> {
        let result = sqlx::query_as!(
            IndexedChunk,
            r#"
            SELECT id,
                   site_id,
                   url, 
                   page_title, 
                   heading_path, 
                   chunk_content, 
                   chunk_index,
                   vector_id, 
                   indexed_date
            FROM indexed_chunks WHERE id = ?
            "#,
            id
        )
        .fetch_optional(pool)
        .await
        .context("Failed to get indexed chunk by id")?;

        Ok(result)
    }

    #[inline]
    pub async fn list_by_site(pool: &SqlitePool, site_id: i64) -> Result<Vec<IndexedChunk>> {
        let chunks = sqlx::query_as!(
            IndexedChunk,
            r#"
            SELECT id,
                   site_id,
                   url, 
                   page_title, 
                   heading_path, 
                   chunk_content, 
                   chunk_index,
                   vector_id, 
                   indexed_date
            FROM indexed_chunks WHERE site_id = ? ORDER BY url, chunk_index
            "#,
            site_id
        )
        .fetch_all(pool)
        .await
        .context("Failed to list indexed chunks by site")?;

        Ok(chunks)
    }

    #[inline]
    pub async fn delete_by_site(pool: &SqlitePool, site_id: i64) -> Result<usize> {
        let result = sqlx::query!("DELETE FROM indexed_chunks WHERE site_id = ?", site_id)
            .execute(pool)
            .await
            .context("Failed to delete indexed chunks by site")?;

        Ok(result.rows_affected() as usize)
    }

    #[inline]
    pub async fn count_by_site(pool: &SqlitePool, site_id: i64) -> Result<i64> {
        let count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM indexed_chunks WHERE site_id = ?",
            site_id
        )
        .fetch_one(pool)
        .await
        .context("Failed to count indexed chunks by site")?;

        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use tempfile::TempDir;

    async fn create_test_pool() -> (TempDir, SqlitePool) {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        let db_path = temp_dir.path().join("test.db");

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                sqlx::sqlite::SqliteConnectOptions::new()
                    .filename(&db_path)
                    .create_if_missing(true)
                    .foreign_keys(true),
            )
            .await
            .expect("Failed to create test pool");

        sqlx::query(include_str!("migrations/001_initial_schema.sql"))
            .execute(&pool)
            .await
            .expect("Failed to run migrations");

        (temp_dir, pool)
    }

    #[tokio::test]
    async fn site_crud_operations() {
        let (_temp_dir, pool) = create_test_pool().await;

        let new_site = NewSite {
            base_url: "https://example.com".to_string(),
            name: "Test Site".to_string(),
            version: "1.0".to_string(),
        };

        let created_site = SiteQueries::create(&pool, new_site)
            .await
            .expect("Failed to create site");

        assert_eq!(created_site.name, "Test Site");
        assert_eq!(created_site.status, SiteStatus::Pending);

        let retrieved_site = SiteQueries::get_by_id(&pool, created_site.id)
            .await
            .expect("Failed to get site")
            .expect("Site should exist");

        assert_eq!(retrieved_site.id, created_site.id);
        assert_eq!(retrieved_site.name, "Test Site");

        let update = SiteUpdate {
            status: Some(SiteStatus::Indexing),
            progress_percent: Some(50),
            total_pages: Some(100),
            indexed_pages: Some(50),
            error_message: None,
            last_heartbeat: Some(Utc::now().naive_utc()),
            indexed_date: None,
        };

        let updated_site = SiteQueries::update(&pool, created_site.id, update)
            .await
            .expect("Failed to update site")
            .expect("Site should exist");

        assert_eq!(updated_site.status, SiteStatus::Indexing);
        assert_eq!(updated_site.progress_percent, 50);

        let deleted = SiteQueries::delete(&pool, created_site.id)
            .await
            .expect("Failed to delete site");

        assert!(deleted);

        let not_found = SiteQueries::get_by_id(&pool, created_site.id)
            .await
            .expect("Query should succeed");

        assert!(not_found.is_none());
    }

    #[tokio::test]
    async fn crawl_queue_operations() {
        let (_temp_dir, pool) = create_test_pool().await;

        let new_site = NewSite {
            base_url: "https://example.com".to_string(),
            name: "Test Site".to_string(),
            version: "1.0".to_string(),
        };

        let site = SiteQueries::create(&pool, new_site)
            .await
            .expect("Failed to create site");

        let new_item = NewCrawlQueueItem {
            site_id: site.id,
            url: "https://example.com/page1".to_string(),
        };

        let created_item = CrawlQueueQueries::add_url(&pool, new_item)
            .await
            .expect("Failed to add URL to queue");

        assert_eq!(created_item.site_id, site.id);
        assert_eq!(created_item.status, CrawlStatus::Pending);

        let next_item = CrawlQueueQueries::get_next_pending(&pool, site.id)
            .await
            .expect("Failed to get next pending")
            .expect("Should have pending item");

        assert_eq!(next_item.id, created_item.id);

        let update = CrawlQueueUpdate {
            status: Some(CrawlStatus::Completed),
            retry_count: None,
            error_message: None,
        };

        let updated_item = CrawlQueueQueries::update_status(&pool, created_item.id, update)
            .await
            .expect("Failed to update status")
            .expect("Item should exist");

        assert_eq!(updated_item.status, CrawlStatus::Completed);
    }

    #[tokio::test]
    async fn indexed_chunk_operations() {
        let (_temp_dir, pool) = create_test_pool().await;

        let new_site = NewSite {
            base_url: "https://example.com".to_string(),
            name: "Test Site".to_string(),
            version: "1.0".to_string(),
        };

        let site = SiteQueries::create(&pool, new_site)
            .await
            .expect("Failed to create site");

        let new_chunk = NewIndexedChunk {
            site_id: site.id,
            url: "https://example.com/page1".to_string(),
            page_title: Some("Test Page".to_string()),
            heading_path: Some("Page Title > Section".to_string()),
            chunk_content: "This is a test chunk of content.".to_string(),
            chunk_index: 0,
            vector_id: "test-vector-id".to_string(),
        };

        let created_chunk = IndexedChunkQueries::create(&pool, new_chunk)
            .await
            .expect("Failed to create chunk");

        assert_eq!(created_chunk.site_id, site.id);
        assert_eq!(created_chunk.vector_id, "test-vector-id");

        let retrieved_chunk = IndexedChunkQueries::get_by_vector_id(&pool, "test-vector-id")
            .await
            .expect("Failed to get chunk")
            .expect("Chunk should exist");

        assert_eq!(retrieved_chunk.id, created_chunk.id);
        assert_eq!(
            retrieved_chunk.chunk_content,
            "This is a test chunk of content."
        );

        let count = IndexedChunkQueries::count_by_site(&pool, site.id)
            .await
            .expect("Failed to count chunks");

        assert_eq!(count, 1);
    }
}
