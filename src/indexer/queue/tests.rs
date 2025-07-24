use super::*;
use crate::config::{Config, OllamaConfig};
use crate::database::sqlite::models::NewSite;
use crate::database::sqlite::queries::SiteQueries;
use tempfile::TempDir;

async fn create_test_setup() -> Result<(QueueManager, Database, i64, TempDir)> {
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

    let database = Database::new(&config.database_path()).await?;

    // Create a test site
    let new_site = NewSite {
        name: "Test Site".to_string(),
        base_url: "https://example.com".to_string(),
        version: "1.0".to_string(),
    };

    let site = SiteQueries::create(database.pool(), new_site).await?;

    let queue_config = QueueConfig {
        max_retries: 3,
        initial_retry_delay_ms: 100,
        max_retry_delay_ms: 5000,
        batch_size: 5,
        processing_timeout_seconds: 10,
        cleanup_age_seconds: 3600,
    };

    let queue_manager = QueueManager::new(database.clone(), queue_config);

    Ok((queue_manager, database, site.id, temp_dir))
}

#[tokio::test]
async fn queue_manager_basic_operations() {
    let (queue_manager, _database, site_id, _temp_dir) =
        create_test_setup().await.expect("can create test setup");

    // Test adding URLs to queue
    let item1 = queue_manager
        .add_url_with_priority(
            site_id,
            "https://example.com/page1".to_string(),
            QueuePriority::Normal,
        )
        .await
        .expect("can add URL to queue");

    assert_eq!(item1.site_id, site_id);
    assert_eq!(item1.url, "https://example.com/page1");
    assert_eq!(item1.status, CrawlStatus::Pending);
    assert_eq!(item1.retry_count, 0);

    // Test queue stats
    let stats = queue_manager
        .get_queue_stats(Some(site_id))
        .await
        .expect("can get queue stats");

    assert_eq!(stats.pending_count, 1);
    assert_eq!(stats.processing_count, 0);
    assert_eq!(stats.completed_count, 0);
    assert_eq!(stats.failed_count, 0);
    assert_eq!(stats.total_count, 1);
}

#[tokio::test]
async fn queue_stats_empty_queue() {
    let temp_dir = TempDir::new().expect("can create temp dir");
    let config = Config {
        ollama: OllamaConfig {
            host: "localhost".to_string(),
            port: 11434,
            model: "nomic-embed-text:latest".to_string(),
            batch_size: 32,
        },
        base_dir: Some(temp_dir.path().to_path_buf()),
    };
    let database = Database::new(&config.database_path())
        .await
        .expect("can connect to test database");
    let queue_config = QueueConfig {
        max_retries: 3,
        initial_retry_delay_ms: 100,
        max_retry_delay_ms: 5000,
        batch_size: 5,
        processing_timeout_seconds: 10,
        cleanup_age_seconds: 3600,
    };
    let queue_manager = QueueManager::new(database.clone(), queue_config);

    let stats = queue_manager
        .get_queue_stats(None)
        .await
        .expect("can get queue stats");

    assert_eq!(stats.pending_count, 0);
    assert_eq!(stats.processing_count, 0);
    assert_eq!(stats.completed_count, 0);
    assert_eq!(stats.failed_count, 0);
    assert_eq!(stats.total_count, 0);
}

#[tokio::test]
async fn priority_based_queue_processing() {
    let (mut queue_manager, _database, site_id, _temp_dir) =
        create_test_setup().await.expect("can create test setup");

    // Add URLs with different priorities
    let _ = queue_manager
        .add_url_with_priority(
            site_id,
            "https://example.com/low".to_string(),
            QueuePriority::Low,
        )
        .await
        .expect("can add low priority URL");

    let _ = queue_manager
        .add_url_with_priority(
            site_id,
            "https://example.com/critical".to_string(),
            QueuePriority::Critical,
        )
        .await
        .expect("can add critical priority URL");

    let _ = queue_manager
        .add_url_with_priority(
            site_id,
            "https://example.com/normal".to_string(),
            QueuePriority::Normal,
        )
        .await
        .expect("can add normal priority URL");

    // Get next batch for processing
    let batch = queue_manager
        .get_next_batch(site_id)
        .await
        .expect("can get next batch");

    assert_eq!(batch.len(), 3);

    // All items should now be marked as processing
    let stats = queue_manager
        .get_queue_stats(Some(site_id))
        .await
        .expect("can get queue stats");

    assert_eq!(stats.pending_count, 0);
    assert_eq!(stats.processing_count, 3);
}

#[tokio::test]
async fn retry_logic_with_exponential_backoff() {
    let (mut queue_manager, _database, site_id, _temp_dir) =
        create_test_setup().await.expect("can create test setup");

    // Add a URL and get it for processing
    queue_manager
        .add_url_with_priority(
            site_id,
            "https://example.com/failing".to_string(),
            QueuePriority::Normal,
        )
        .await
        .expect("can add URL to queue");

    let mut batch = queue_manager
        .get_next_batch(site_id)
        .await
        .expect("can get next batch");

    assert_eq!(batch.len(), 1);
    let item_id = batch[0].id;

    // Test retry logic
    for retry_attempt in 1..=3 {
        queue_manager
            .mark_failed_with_retry(item_id, format!("Error attempt {}", retry_attempt))
            .await
            .expect("can mark as failed with retry");

        // Get stats to verify retry behavior
        let stats = queue_manager
            .get_queue_stats(Some(site_id))
            .await
            .expect("can get queue stats");

        if retry_attempt < 3 {
            // Should be pending for retry
            assert_eq!(stats.pending_count, 1);
            assert_eq!(stats.processing_count, 0);
            assert_eq!(stats.failed_count, 0);

            // Get the item again for next retry
            batch = queue_manager
                .get_next_batch(site_id)
                .await
                .expect("can get next batch for retry");
            assert_eq!(batch.len(), 1);
            assert_eq!(batch[0].retry_count, retry_attempt);
        } else {
            // Should be permanently failed after max retries
            assert_eq!(stats.pending_count, 0);
            assert_eq!(stats.processing_count, 0);
            assert_eq!(stats.failed_count, 1);
        }
    }
}

#[tokio::test]
async fn queue_completion_workflow() {
    let (mut queue_manager, _database, site_id, _temp_dir) =
        create_test_setup().await.expect("can create test setup");

    // Add URLs and process them
    for i in 1..=5 {
        queue_manager
            .add_url_with_priority(
                site_id,
                format!("https://example.com/page{}", i),
                QueuePriority::Normal,
            )
            .await
            .expect("can add URL to queue");
    }

    let batch = queue_manager
        .get_next_batch(site_id)
        .await
        .expect("can get next batch");

    assert_eq!(batch.len(), 5);

    // Mark items as completed
    for item in batch {
        queue_manager
            .mark_completed(item.id)
            .await
            .expect("can mark item as completed");
    }

    // Verify all items are completed
    let stats = queue_manager
        .get_queue_stats(Some(site_id))
        .await
        .expect("can get queue stats");

    assert_eq!(stats.pending_count, 0);
    assert_eq!(stats.processing_count, 0);
    assert_eq!(stats.completed_count, 5);
    assert_eq!(stats.failed_count, 0);
    assert_eq!(stats.total_count, 5);
}

#[tokio::test]
async fn stuck_item_recovery() {
    let (mut queue_manager, _database, site_id, _temp_dir) =
        create_test_setup().await.expect("can create test setup");

    // Add URL and start processing
    queue_manager
        .add_url_with_priority(
            site_id,
            "https://example.com/stuck".to_string(),
            QueuePriority::Normal,
        )
        .await
        .expect("can add URL to queue");

    let batch = queue_manager
        .get_next_batch(site_id)
        .await
        .expect("can get next batch");

    assert_eq!(batch.len(), 1);

    // Simulate processing timeout by manually updating the created_date
    // to be older than the timeout
    let old_time = Utc::now().naive_utc() - chrono::Duration::seconds(20); // Older than 10s timeout
    sqlx::query!(
        "UPDATE crawl_queue SET created_date = ? WHERE id = ?",
        old_time,
        batch[0].id
    )
    .execute(queue_manager.database.pool())
    .await
    .expect("can update created_date");

    // Reset stuck items
    let reset_count = queue_manager
        .reset_stuck_items()
        .await
        .expect("can reset stuck items");

    assert_eq!(reset_count, 1);

    // Verify item is back to pending
    let stats = queue_manager
        .get_queue_stats(Some(site_id))
        .await
        .expect("can get queue stats");

    assert_eq!(stats.pending_count, 1);
    assert_eq!(stats.processing_count, 0);
}

#[tokio::test]
async fn queue_cleanup_operations() {
    let (queue_manager, _database, site_id, _temp_dir) =
        create_test_setup().await.expect("can create test setup");

    // Add and complete some items
    for i in 1..=3 {
        let item = queue_manager
            .add_url_with_priority(
                site_id,
                format!("https://example.com/old{}", i),
                QueuePriority::Normal,
            )
            .await
            .expect("can add URL to queue");

        // Manually mark as completed with old timestamp
        let old_time = Utc::now().naive_utc() - chrono::Duration::hours(25); // Older than 24h cleanup threshold
        sqlx::query!(
            "UPDATE crawl_queue SET status = 'completed', created_date = ? WHERE id = ?",
            old_time,
            item.id
        )
        .execute(queue_manager.database.pool())
        .await
        .expect("can update item as old completed");
    }

    // Add recent item
    queue_manager
        .add_url_with_priority(
            site_id,
            "https://example.com/recent".to_string(),
            QueuePriority::Normal,
        )
        .await
        .expect("can add recent URL");

    // Test cleanup
    let cleanup_count = queue_manager
        .cleanup_old_items(Some(site_id))
        .await
        .expect("can cleanup old items");

    assert_eq!(cleanup_count, 3);

    // Verify recent item remains
    let stats = queue_manager
        .get_queue_stats(Some(site_id))
        .await
        .expect("can get queue stats");

    assert_eq!(stats.total_count, 1);
    assert_eq!(stats.pending_count, 1);
}

#[tokio::test]
async fn performance_metrics_calculation() {
    let (queue_manager, _database, site_id, _temp_dir) =
        create_test_setup().await.expect("can create test setup");

    // Add URLs with varying success/failure patterns
    for i in 1..=10 {
        let item = queue_manager
            .add_url_with_priority(
                site_id,
                format!("https://example.com/perf{}", i),
                QueuePriority::Normal,
            )
            .await
            .expect("can add URL to queue");

        // Mark some as completed, some as failed
        if i <= 7 {
            // Mark directly as completed (simulating successful processing)
            sqlx::query!(
                "UPDATE crawl_queue SET status = 'completed' WHERE id = ?",
                item.id
            )
            .execute(queue_manager.database.pool())
            .await
            .expect("can mark as completed");
        } else {
            // Mark as failed with retries
            sqlx::query!(
                "UPDATE crawl_queue SET status = 'failed', retry_count = 2 WHERE id = ?",
                item.id
            )
            .execute(queue_manager.database.pool())
            .await
            .expect("can mark as failed");
        }
    }

    // Get performance metrics
    let metrics = queue_manager
        .get_performance_metrics(60) // 60-minute window
        .await
        .expect("can get performance metrics");

    assert!(metrics.items_processed_per_minute >= 0.0);
    assert!(metrics.success_rate_percent >= 0.0 && metrics.success_rate_percent <= 100.0);
    assert!(metrics.average_retry_count >= 0.0);
    assert!(!metrics.bottleneck_analysis.is_empty());

    // Success rate should be 70% (7 out of 10 successful)
    assert!((metrics.success_rate_percent - 70.0).abs() < 1.0);
}

#[tokio::test]
async fn queue_batch_size_limits() {
    let (mut queue_manager, _database, site_id, _temp_dir) =
        create_test_setup().await.expect("can create test setup");

    // Add more URLs than batch size (batch size is 5 in test config)
    for i in 1..=10 {
        queue_manager
            .add_url_with_priority(
                site_id,
                format!("https://example.com/batch{}", i),
                QueuePriority::Normal,
            )
            .await
            .expect("can add URL to queue");
    }

    // Get batch should respect batch size limit
    let batch = queue_manager
        .get_next_batch(site_id)
        .await
        .expect("can get next batch");

    assert_eq!(batch.len(), 5); // Should be limited by batch_size config

    // Verify remaining items are still pending
    let stats = queue_manager
        .get_queue_stats(Some(site_id))
        .await
        .expect("can get queue stats");

    assert_eq!(stats.pending_count, 5);
    assert_eq!(stats.processing_count, 5);
}

#[tokio::test]
async fn queue_optimization() {
    let (queue_manager, _database, site_id, _temp_dir) =
        create_test_setup().await.expect("can create test setup");

    // Add some test data
    for i in 1..=5 {
        queue_manager
            .add_url_with_priority(
                site_id,
                format!("https://example.com/opt{}", i),
                QueuePriority::Normal,
            )
            .await
            .expect("can add URL to queue");
    }

    // Run optimization
    let optimization_result = queue_manager
        .optimize_queue()
        .await
        .expect("can optimize queue");

    assert!(!optimization_result.is_empty());
    // The result should contain information about what was optimized
    assert!(
        optimization_result.contains("optimization")
            || optimization_result.contains("optimized")
            || optimization_result.contains("Optimizations applied")
            || optimization_result.contains("already optimized")
    );
}

#[tokio::test]
async fn exponential_backoff_timing() {
    let (queue_manager, _, _, _temp_dir) =
        create_test_setup().await.expect("can create test setup");

    // Test exponential backoff calculation (with config initial_retry_delay_ms: 100)
    assert_eq!(queue_manager.calculate_retry_delay(1), 100); // 100ms
    assert_eq!(queue_manager.calculate_retry_delay(2), 200); // 200ms
    assert_eq!(queue_manager.calculate_retry_delay(3), 400); // 400ms
    assert_eq!(queue_manager.calculate_retry_delay(4), 800); // 800ms
    assert_eq!(queue_manager.calculate_retry_delay(5), 1600); // 1600ms
    assert_eq!(queue_manager.calculate_retry_delay(10), 5000); // capped at max (5000ms)
}
