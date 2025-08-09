#[cfg(test)]
mod tests;

use super::models::*;
use anyhow::{Context, Result};
use chrono::Utc;
use sqlx::SqlitePool;

pub struct SiteQueries;

impl SiteQueries {
    pub async fn create(pool: &SqlitePool, new_site: NewSite) -> Result<Site> {
        let now = Utc::now();
        let id = sqlx::query!(
            "INSERT INTO sites (index_url, base_url, name, version, status, created_date) VALUES (?, ?, ?, ?, 'pending', ?)",
            new_site.index_url,
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

    pub async fn get_by_id(pool: &SqlitePool, id: i64) -> Result<Option<Site>> {
        let result = sqlx::query_as!(
            Site,
            r#"
            SELECT id,
                   index_url, 
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

    #[allow(dead_code)]
    pub async fn get_by_name_and_version(
        pool: &SqlitePool,
        name: &str,
        version: &str,
    ) -> Result<Option<Site>> {
        let result = sqlx::query_as!(
            Site,
            r#"
            SELECT id,
                   index_url, 
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

    pub async fn get_by_index_url(pool: &SqlitePool, index_url: &str) -> Result<Option<Site>> {
        let result = sqlx::query_as!(
            Site,
            r#"
            SELECT id,
                   index_url, 
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
            FROM sites WHERE index_url = ?
            "#,
            index_url
        )
        .fetch_optional(pool)
        .await
        .context("Failed to get site by index URL")?;

        Ok(result)
    }

    pub async fn list_all(pool: &SqlitePool) -> Result<Vec<Site>> {
        let sites = sqlx::query_as!(
            Site,
            r#"
            SELECT id,
                   index_url,
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

    #[allow(dead_code)]
    pub async fn list_completed(pool: &SqlitePool) -> Result<Vec<Site>> {
        let sites = sqlx::query_as!(
            Site,
            r#"
            SELECT id,
                   index_url,
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

    pub async fn delete(pool: &SqlitePool, id: i64) -> Result<bool> {
        let result = sqlx::query!("DELETE FROM sites WHERE id = ?", id)
            .execute(pool)
            .await
            .context("Failed to delete site")?;

        Ok(result.rows_affected() > 0)
    }

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

    pub async fn get_sites_by_status(pool: &SqlitePool, status: SiteStatus) -> Result<Vec<Site>> {
        let status_str = match status {
            SiteStatus::Pending => "pending",
            SiteStatus::Indexing => "indexing",
            SiteStatus::Completed => "completed",
            SiteStatus::Failed => "failed",
        };

        let sites = sqlx::query_as!(
            Site,
            r#"
            SELECT id,
                   index_url,
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
            FROM sites WHERE status = ? ORDER BY created_date ASC
            "#,
            status_str
        )
        .fetch_all(pool)
        .await
        .context("Failed to get sites by status")?;

        Ok(sites)
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct QueueStats {
    pub total: i64,
    pub pending: i64,
    pub processing: i64,
    pub completed: i64,
    pub failed: i64,
}

pub struct CrawlQueueQueries;

impl CrawlQueueQueries {
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

    pub async fn get_next_pending(
        pool: &SqlitePool,
        site_id: i64,
        max_retries: u32,
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
            WHERE site_id = ? AND (status = 'pending' OR (status = 'failed' AND retry_count < ?))
            ORDER BY created_date ASC
            LIMIT 1
            "#,
            site_id,
            max_retries
        )
        .fetch_optional(pool)
        .await
        .context("Failed to get next pending crawl item")?;

        Ok(result)
    }

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

    pub async fn create(pool: &SqlitePool, new_item: NewCrawlQueueItem) -> Result<CrawlQueueItem> {
        // This is an alias for add_url for consistency with other create methods
        Self::add_url(pool, new_item).await
    }

    pub async fn update(
        pool: &SqlitePool,
        id: i64,
        update: CrawlQueueUpdate,
    ) -> Result<Option<CrawlQueueItem>> {
        // This is an alias for update_status for consistency with other update methods
        Self::update_status(pool, id, update).await
    }

    pub async fn increment_retry_count(pool: &SqlitePool, id: i64) -> Result<()> {
        sqlx::query!(
            "UPDATE crawl_queue SET retry_count = retry_count + 1 WHERE id = ?",
            id
        )
        .execute(pool)
        .await
        .context("Failed to increment retry count")?;

        Ok(())
    }

    pub async fn get_stats(pool: &SqlitePool, site_id: i64) -> Result<QueueStats> {
        let stats = sqlx::query!(
            r#"
            SELECT 
                COUNT(*) as total,
                COALESCE(SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END), 0) as "pending!",
                COALESCE(SUM(CASE WHEN status = 'processing' THEN 1 ELSE 0 END), 0) as "processing!",
                COALESCE(SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END), 0) as "completed!",
                COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0) as "failed!"
            FROM crawl_queue 
            WHERE site_id = ?
            "#,
            site_id
        )
        .fetch_one(pool)
        .await
        .context("Failed to get crawl queue statistics")?;

        Ok(QueueStats {
            total: stats.total,
            pending: stats.pending,
            processing: stats.processing,
            completed: stats.completed,
            failed: stats.failed,
        })
    }

    pub async fn get_completed_for_site(
        pool: &SqlitePool,
        site_id: i64,
    ) -> Result<Vec<CrawlQueueItem>> {
        let items = sqlx::query_as!(
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
            WHERE site_id = ? AND status = 'completed'
            ORDER BY created_date ASC
            "#,
            site_id
        )
        .fetch_all(pool)
        .await
        .context("Failed to get completed crawl items for site")?;

        Ok(items)
    }
}

pub struct IndexedChunkQueries;

impl IndexedChunkQueries {
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

    #[cfg(test)]
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
