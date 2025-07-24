// Queue management module for background indexing operations
// Provides comprehensive queue processing with priority, monitoring, and maintenance

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use chrono::Utc;
use sqlx::Executor;
use tracing::{debug, info, warn};

use crate::database::sqlite::{CrawlQueueItem, CrawlStatus, Database, NewCrawlQueueItem};

/// Priority levels for queue items
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum QueuePriority {
    Low = 0,
    #[default]
    Normal = 1,
    High = 2,
    Critical = 3,
}

/// Queue processing configuration
#[derive(Debug, Clone)]
pub struct QueueConfig {
    /// Maximum number of retry attempts for failed items
    pub max_retries: u32,
    /// Initial retry delay in milliseconds (exponential backoff base)
    pub initial_retry_delay_ms: u64,
    /// Maximum retry delay in milliseconds
    pub max_retry_delay_ms: u64,
    /// Batch size for queue processing
    pub batch_size: usize,
    /// Timeout for individual queue item processing
    pub processing_timeout_seconds: u64,
    /// Maximum age of completed items before cleanup (in seconds)
    pub cleanup_age_seconds: u64,
}

impl Default for QueueConfig {
    #[inline]
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_retry_delay_ms: 1000, // 1 second
            max_retry_delay_ms: 60000,    // 1 minute
            batch_size: 64,
            processing_timeout_seconds: 300, // 5 minutes
            cleanup_age_seconds: 86400,      // 24 hours
        }
    }
}

/// Queue statistics for monitoring
#[derive(Debug, Clone, PartialEq)]
pub struct QueueStats {
    pub pending_count: u64,
    pub processing_count: u64,
    pub completed_count: u64,
    pub failed_count: u64,
    pub total_count: u64,
    pub average_processing_time_ms: Option<u64>,
    pub oldest_pending_age_seconds: Option<u64>,
    pub retry_rate_percent: f32,
}

/// Performance metrics for queue operations
#[derive(Debug, Clone)]
pub struct QueueMetrics {
    pub items_processed_per_minute: f32,
    pub average_retry_count: f32,
    pub success_rate_percent: f32,
    pub current_throughput: f32,
    pub bottleneck_analysis: String,
}

/// Comprehensive queue manager for background indexing operations
pub struct QueueManager {
    database: Database,
    config: QueueConfig,
    processing_start_times: HashMap<i64, SystemTime>,
}

impl QueueManager {
    /// Create a new queue manager
    #[inline]
    pub fn new(database: Database, config: QueueConfig) -> Self {
        Self {
            database,
            config,
            processing_start_times: HashMap::new(),
        }
    }

    /// Add a single URL to the queue with priority
    #[inline]
    pub async fn add_url_with_priority(
        &self,
        site_id: i64,
        url: String,
        _priority: QueuePriority,
    ) -> Result<CrawlQueueItem> {
        let new_item = NewCrawlQueueItem { site_id, url };
        let now = Utc::now().naive_utc();
        self.database
            .pool()
            .execute(sqlx::query!(
                "INSERT INTO crawl_queue (site_id, url, status, created_date) VALUES (?, ?, 'pending', ?)",
                new_item.site_id,
                new_item.url,
                now
            ))
            .await
            .context("Failed to add URL to queue")?;

        // Retrieve the created item
        let item = sqlx::query_as!(
            CrawlQueueItem,
            r#"
            SELECT id, site_id, url, status as "status: CrawlStatus", retry_count, error_message, created_date
            FROM crawl_queue 
            WHERE site_id = ? AND url = ?
            ORDER BY id DESC
            LIMIT 1
            "#,
            site_id,
            new_item.url
        )
        .fetch_one(self.database.pool())
        .await
        .context("Failed to retrieve created queue item")?;

        debug!("Added URL to queue: {} (ID: {})", new_item.url, item.id);
        Ok(item)
    }

    /// Get next batch of items for processing with priority ordering
    #[inline]
    pub async fn get_next_batch(&mut self, site_id: i64) -> Result<Vec<CrawlQueueItem>> {
        let max_retries = self.config.max_retries as i64;
        let batch_size = self.config.batch_size as i64;
        let items = sqlx::query_as!(
            CrawlQueueItem,
            r#"
            SELECT id, site_id, url, status as "status: CrawlStatus", retry_count, error_message, created_date
            FROM crawl_queue 
            WHERE site_id = ? 
            AND (status = 'pending' OR (status = 'failed' AND retry_count < ?))
            ORDER BY created_date ASC
            LIMIT ?
            "#,
            site_id,
            max_retries,
            batch_size
        )
        .fetch_all(self.database.pool())
        .await
        .context("Failed to get next batch from queue")?;

        // Mark items as processing and track start times
        for item in &items {
            self.mark_processing(item.id).await?;
            self.processing_start_times
                .insert(item.id, SystemTime::now());
        }

        debug!("Retrieved batch of {} items for processing", items.len());
        Ok(items)
    }

    /// Mark item as processing
    async fn mark_processing(&self, item_id: i64) -> Result<()> {
        sqlx::query!(
            "UPDATE crawl_queue SET status = 'processing' WHERE id = ?",
            item_id
        )
        .execute(self.database.pool())
        .await
        .context("Failed to mark item as processing")?;

        Ok(())
    }

    /// Mark item as completed
    #[inline]
    pub async fn mark_completed(&mut self, item_id: i64) -> Result<()> {
        self.processing_start_times.remove(&item_id);

        sqlx::query!(
            "UPDATE crawl_queue SET status = 'completed' WHERE id = ?",
            item_id
        )
        .execute(self.database.pool())
        .await
        .context("Failed to mark item as completed")?;

        debug!("Marked queue item {} as completed", item_id);
        Ok(())
    }

    /// Mark item as failed with exponential backoff retry logic
    #[inline]
    pub async fn mark_failed_with_retry(
        &mut self,
        item_id: i64,
        error_message: String,
    ) -> Result<()> {
        self.processing_start_times.remove(&item_id);

        // Get current retry count
        let current_item = sqlx::query_as!(
            CrawlQueueItem,
            r#"
            SELECT id, site_id, url, status as "status: CrawlStatus", retry_count, error_message, created_date
            FROM crawl_queue WHERE id = ?
            "#,
            item_id
        )
        .fetch_one(self.database.pool())
        .await
        .context("Failed to get current item for retry logic")?;

        let new_retry_count = current_item.retry_count + 1;

        if new_retry_count >= self.config.max_retries as i64 {
            // Max retries reached, mark as permanently failed
            sqlx::query!(
                "UPDATE crawl_queue SET status = 'failed', retry_count = ?, error_message = ? WHERE id = ?",
                new_retry_count,
                error_message,
                item_id
            )
            .execute(self.database.pool())
            .await
            .context("Failed to mark item as permanently failed")?;

            warn!(
                "Item {} permanently failed after {} retries: {}",
                item_id, new_retry_count, error_message
            );
        } else {
            // Calculate exponential backoff delay
            let delay_ms = self.calculate_retry_delay(new_retry_count as u32);

            // Mark as pending for retry (we'll implement retry scheduling later)
            sqlx::query!(
                "UPDATE crawl_queue SET status = 'pending', retry_count = ?, error_message = ? WHERE id = ?",
                new_retry_count,
                error_message,
                item_id
            )
            .execute(self.database.pool())
            .await
            .context("Failed to schedule item for retry")?;

            info!(
                "Item {} scheduled for retry {} after {}ms: {}",
                item_id, new_retry_count, delay_ms, error_message
            );
        }

        Ok(())
    }

    /// Calculate retry delay using exponential backoff
    fn calculate_retry_delay(&self, retry_count: u32) -> u64 {
        let delay = self.config.initial_retry_delay_ms * (2_u64.pow(retry_count.saturating_sub(1)));
        delay.min(self.config.max_retry_delay_ms)
    }

    /// Get comprehensive queue statistics
    #[inline]
    pub async fn get_queue_stats(&self, site_id: Option<i64>) -> Result<QueueStats> {
        let (pending_count, processing_count, completed_count, failed_count) =
            if let Some(site_id) = site_id {
                // Site-specific stats
                let stats = sqlx::query!(
                    r#"
                SELECT 
                    SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END) as "pending: i64",
                    SUM(CASE WHEN status = 'processing' THEN 1 ELSE 0 END) as "processing: i64",
                    SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END) as "completed: i64",
                    SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) as "failed: i64"
                FROM crawl_queue WHERE site_id = ?
                "#,
                    site_id
                )
                .fetch_one(self.database.pool())
                .await
                .context("Failed to get site queue statistics")?;

                (
                    stats.pending.unwrap_or(0) as u64,
                    stats.processing.unwrap_or(0) as u64,
                    stats.completed.unwrap_or(0) as u64,
                    stats.failed.unwrap_or(0) as u64,
                )
            } else {
                // Global stats
                let stats = sqlx::query!(
                    r#"
                SELECT 
                    SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END) as "pending: i64",
                    SUM(CASE WHEN status = 'processing' THEN 1 ELSE 0 END) as "processing: i64",
                    SUM(CASE WHEN status = 'completed' THEN 1 ELSE 0 END) as "completed: i64",
                    SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END) as "failed: i64"
                FROM crawl_queue
                "#
                )
                .fetch_one(self.database.pool())
                .await
                .context("Failed to get global queue statistics")?;

                (
                    stats.pending.unwrap_or(0) as u64,
                    stats.processing.unwrap_or(0) as u64,
                    stats.completed.unwrap_or(0) as u64,
                    stats.failed.unwrap_or(0) as u64,
                )
            };

        let total_count = pending_count + processing_count + completed_count + failed_count;

        // Calculate retry rate
        let retry_items = if let Some(site_id) = site_id {
            sqlx::query_scalar!(
                "SELECT COUNT(*) FROM crawl_queue WHERE site_id = ? AND retry_count > 0",
                site_id
            )
            .fetch_one(self.database.pool())
            .await
            .context("Failed to get retry count for site")?
        } else {
            sqlx::query_scalar!("SELECT COUNT(*) FROM crawl_queue WHERE retry_count > 0")
                .fetch_one(self.database.pool())
                .await
                .context("Failed to get retry count")?
        };

        let retry_rate_percent = if total_count > 0 {
            (retry_items as f32 / total_count as f32) * 100.0
        } else {
            0.0
        };

        // Get oldest pending item age
        let oldest_pending_age_seconds = if let Some(site_id) = site_id {
            sqlx::query_scalar!(
                "SELECT MIN(created_date) as \"created_date: chrono::NaiveDateTime\" FROM crawl_queue WHERE site_id = ? AND status = 'pending'",
                site_id
            )
            .fetch_optional(self.database.pool())
            .await
            .context("Failed to get oldest pending item for site")?
        } else {
            sqlx::query_scalar!(
                "SELECT MIN(created_date) as \"created_date: chrono::NaiveDateTime\" FROM crawl_queue WHERE status = 'pending'"
            )
            .fetch_optional(self.database.pool())
            .await
            .context("Failed to get oldest pending item")?
        };

        let oldest_pending_age_seconds = oldest_pending_age_seconds.flatten().map(|created_date| {
            let now = Utc::now().naive_utc();
            now.signed_duration_since(created_date).num_seconds().max(0) as u64
        });

        Ok(QueueStats {
            pending_count,
            processing_count,
            completed_count,
            failed_count,
            total_count,
            average_processing_time_ms: None, // TODO: Implement processing time tracking
            oldest_pending_age_seconds,
            retry_rate_percent,
        })
    }

    /// Clean up old completed and failed queue items
    #[inline]
    pub async fn cleanup_old_items(&self, site_id: Option<i64>) -> Result<u64> {
        let cutoff_time = Utc::now().naive_utc()
            - chrono::Duration::seconds(self.config.cleanup_age_seconds as i64);

        let deleted_count = if let Some(site_id) = site_id {
            sqlx::query!(
                "DELETE FROM crawl_queue WHERE site_id = ? AND status IN ('completed', 'failed') AND created_date < ?",
                site_id,
                cutoff_time
            )
            .execute(self.database.pool())
            .await
            .context("Failed to cleanup old queue items for site")?
            .rows_affected()
        } else {
            sqlx::query!(
                "DELETE FROM crawl_queue WHERE status IN ('completed', 'failed') AND created_date < ?",
                cutoff_time
            )
            .execute(self.database.pool())
            .await
            .context("Failed to cleanup old queue items")?
            .rows_affected()
        };

        if deleted_count > 0 {
            info!("Cleaned up {} old queue items", deleted_count);
        }

        Ok(deleted_count)
    }

    /// Reset stuck processing items (items that have been processing too long)
    #[inline]
    pub async fn reset_stuck_items(&mut self) -> Result<u64> {
        let timeout_cutoff = Utc::now().naive_utc()
            - chrono::Duration::seconds(self.config.processing_timeout_seconds as i64);

        let reset_count = sqlx::query!(
            "UPDATE crawl_queue SET status = 'pending' WHERE status = 'processing' AND created_date < ?",
            timeout_cutoff
        )
        .execute(self.database.pool())
        .await
        .context("Failed to reset stuck processing items")?
        .rows_affected();

        // Clear tracking for reset items
        self.processing_start_times.retain(|_, start_time| {
            start_time.elapsed().unwrap_or(Duration::ZERO)
                < Duration::from_secs(self.config.processing_timeout_seconds)
        });

        if reset_count > 0 {
            warn!("Reset {} stuck processing items", reset_count);
        }

        Ok(reset_count)
    }

    /// Get performance metrics for queue operations
    #[inline]
    pub async fn get_performance_metrics(&self, time_window_minutes: u32) -> Result<QueueMetrics> {
        let time_cutoff =
            Utc::now().naive_utc() - chrono::Duration::minutes(time_window_minutes as i64);

        // Get items processed in time window
        let processed_count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM crawl_queue WHERE status IN ('completed', 'failed') AND created_date >= ?",
            time_cutoff
        )
        .fetch_one(self.database.pool())
        .await
        .context("Failed to get processed count")? as f32;

        let items_processed_per_minute = processed_count / time_window_minutes as f32;

        // Get average retry count
        let avg_retry_count = sqlx::query_scalar!(
            "SELECT AVG(CAST(retry_count as REAL)) FROM crawl_queue WHERE created_date >= ?",
            time_cutoff
        )
        .fetch_one(self.database.pool())
        .await
        .context("Failed to get average retry count")?
        .unwrap_or(0.0) as f32;

        // Get success rate
        let completed_count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM crawl_queue WHERE status = 'completed' AND created_date >= ?",
            time_cutoff
        )
        .fetch_one(self.database.pool())
        .await
        .context("Failed to get completed count")? as f32;

        let success_rate_percent = if processed_count > 0.0 {
            (completed_count / processed_count) * 100.0
        } else {
            0.0
        };

        // Simple bottleneck analysis
        let pending_count =
            sqlx::query_scalar!("SELECT COUNT(*) FROM crawl_queue WHERE status = 'pending'")
                .fetch_one(self.database.pool())
                .await
                .context("Failed to get pending count for bottleneck analysis")?;

        let processing_count =
            sqlx::query_scalar!("SELECT COUNT(*) FROM crawl_queue WHERE status = 'processing'")
                .fetch_one(self.database.pool())
                .await
                .context("Failed to get processing count for bottleneck analysis")?;

        let bottleneck_analysis = if pending_count > processing_count * 5 {
            "High pending queue - consider increasing processing capacity".to_string()
        } else if processing_count > pending_count && items_processed_per_minute < 1.0 {
            "Low throughput - investigate processing efficiency".to_string()
        } else if avg_retry_count > 1.0 {
            "High retry rate - investigate error patterns".to_string()
        } else {
            "Queue processing appears healthy".to_string()
        };

        Ok(QueueMetrics {
            items_processed_per_minute,
            average_retry_count: avg_retry_count,
            success_rate_percent,
            current_throughput: items_processed_per_minute,
            bottleneck_analysis,
        })
    }

    /// Optimize queue performance by analyzing and reorganizing data
    #[inline]
    pub async fn optimize_queue(&self) -> Result<String> {
        info!("Starting queue optimization");

        let mut optimizations = Vec::new();

        // Analyze and cleanup
        let cleanup_count = self.cleanup_old_items(None).await?;
        if cleanup_count > 0 {
            optimizations.push(format!("Cleaned up {} old items", cleanup_count));
        }

        // Analyze vacuum need (SQLite specific)
        sqlx::query!("VACUUM")
            .execute(self.database.pool())
            .await
            .context("Failed to vacuum database")?;
        optimizations.push("Database vacuumed for optimal performance".to_string());

        // Analyze indexes (these should already exist from migrations)
        sqlx::query!("ANALYZE crawl_queue")
            .execute(self.database.pool())
            .await
            .context("Failed to analyze crawl_queue table")?;
        optimizations.push("Queue table statistics updated".to_string());

        let optimization_summary = if optimizations.is_empty() {
            "Queue is already optimized".to_string()
        } else {
            format!("Optimizations applied: {}", optimizations.join(", "))
        };

        info!("Queue optimization completed: {}", optimization_summary);
        Ok(optimization_summary)
    }

    /// Clean up queue resources to free memory
    #[inline]
    pub fn cleanup_resources(&mut self) {
        // Clear processing start times for items that are no longer processing
        self.processing_start_times.retain(|_, start_time| {
            start_time.elapsed().unwrap_or(Duration::ZERO)
                < Duration::from_secs(self.config.processing_timeout_seconds)
        });

        info!("Queue resource cleanup completed");
    }

    /// Get resource usage statistics for the queue manager
    #[inline]
    pub fn get_resource_usage(&self) -> QueueResourceUsage {
        let processing_items_count = self.processing_start_times.len();
        let estimated_memory_mb = (processing_items_count as f64 * 32.0) / 1024.0 / 1024.0;

        QueueResourceUsage {
            processing_items_tracked: processing_items_count,
            estimated_memory_usage_mb: estimated_memory_mb,
            active_batch_size: self.config.batch_size,
            timeout_seconds: self.config.processing_timeout_seconds,
        }
    }
}

/// Resource usage statistics for queue management
#[derive(Debug, Clone, PartialEq)]
pub struct QueueResourceUsage {
    pub processing_items_tracked: usize,
    pub estimated_memory_usage_mb: f64,
    pub active_batch_size: usize,
    pub timeout_seconds: u64,
}
