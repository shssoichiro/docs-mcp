#![expect(
    clippy::tests_outside_test_module,
    reason = "integration tests are only compiled in test mode"
)]

// Integration tests for complete background indexing system
// Tests the integration between queue management, background indexer, and CLI commands

use tempfile::TempDir;

use docs_mcp::config::{Config, OllamaConfig};
use docs_mcp::database::sqlite::{Database, NewSite, SiteQueries, SiteStatus};
use docs_mcp::indexer::{BackgroundIndexer, IndexingStatus, QueueConfig, QueueManager};

/// Create a test configuration and database setup
async fn create_test_setup() -> anyhow::Result<(Config, Database, TempDir)> {
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
    Ok((config, database, temp_dir))
}

/// Test complete background indexing workflow integration
#[tokio::test]
async fn complete_background_indexing_workflow() {
    let (config, database, _temp_dir) = create_test_setup().await.expect("can create test setup");

    // Create a test site
    let new_site = NewSite {
        name: "Test Integration Site".to_string(),
        base_url: "https://example.com".to_string(),
        version: "1.0".to_string(),
    };
    let site = SiteQueries::create(database.pool(), new_site)
        .await
        .expect("can create test site");

    // Initialize background indexer
    let mut indexer = BackgroundIndexer::new(config.clone())
        .await
        .expect("can create background indexer");

    // Test initial status
    let status = indexer
        .get_indexing_status()
        .await
        .expect("can get initial status");
    assert_eq!(status, IndexingStatus::Idle);

    // Test that indexer is not running initially
    assert!(
        !indexer
            .is_indexer_running()
            .await
            .expect("can check if indexer is running")
    );

    // Test performance metrics on empty system
    let metrics = indexer
        .get_performance_metrics()
        .await
        .expect("can get performance metrics");
    assert_eq!(metrics.total_sites_processed, 1); // One site exists but not processed
    assert_eq!(metrics.total_pages_processed, 0);
    assert_eq!(metrics.total_chunks_created, 0);

    // Test queue manager integration
    let queue_config = QueueConfig::default();
    let queue_manager = QueueManager::new(database.clone(), queue_config);

    // Add some URLs to the queue
    for i in 1..=5 {
        queue_manager
            .add_url_with_priority(
                site.id,
                format!("https://example.com/page{}", i),
                docs_mcp::indexer::QueuePriority::Normal,
            )
            .await
            .expect("can add URL to queue");
    }

    // Test queue statistics
    let queue_stats = queue_manager
        .get_queue_stats(Some(site.id))
        .await
        .expect("can get queue stats");
    assert_eq!(queue_stats.total_count, 5);
    assert_eq!(queue_stats.pending_count, 5);

    // Test consistency validation
    let consistency_report = indexer
        .validate_consistency()
        .await
        .expect("can validate consistency");
    assert!(
        consistency_report.is_consistent,
        "Should be consistent initially"
    );

    // Test performance optimization
    let optimization_result = indexer
        .optimize_performance()
        .await
        .expect("can optimize performance");
    assert!(!optimization_result.is_empty());

    println!("✅ Complete background indexing workflow test passed");
}

/// Test auto-start and termination logic
#[tokio::test]
async fn auto_start_termination_logic() {
    let (config, database, _temp_dir) = create_test_setup().await.expect("can create test setup");

    // Test multiple indexer instances (should prevent concurrent access)
    let indexer1 = BackgroundIndexer::new(config.clone())
        .await
        .expect("can create first indexer");

    let mut indexer2 = BackgroundIndexer::new(config.clone())
        .await
        .expect("can create second indexer");

    // First indexer should be able to start
    assert!(
        !indexer1
            .is_indexer_running()
            .await
            .expect("can check first indexer status")
    );

    // Simulate first indexer creating lock file and heartbeat
    indexer1
        .create_lock_file()
        .await
        .expect("can create lock file");
    database
        .update_indexer_heartbeat()
        .await
        .expect("can update heartbeat");

    // Second indexer should detect first is running
    assert!(
        indexer2
            .is_indexer_running()
            .await
            .expect("can check second indexer status")
    );

    // Test that second indexer fails to start
    let start_result = indexer2.start().await;
    assert!(start_result.is_err(), "Second indexer should fail to start");

    // Clean up first indexer
    indexer1
        .cleanup_lock_file()
        .await
        .expect("can cleanup lock file");

    println!("✅ Auto-start and termination logic test passed");
}

/// Test progress tracking and status reporting
#[tokio::test]
async fn progress_tracking_and_status_reporting() {
    let (config, database, _temp_dir) = create_test_setup().await.expect("can create test setup");

    // Create multiple test sites with different statuses
    let sites_data = vec![
        ("Pending Site", SiteStatus::Pending),
        ("Failed Site", SiteStatus::Failed),
        ("Completed Site", SiteStatus::Completed),
    ];

    for (name, status) in sites_data {
        let new_site = NewSite {
            name: name.to_string(),
            base_url: format!("https://{}.com", name.replace(' ', "")),
            version: "1.0".to_string(),
        };
        let site = SiteQueries::create(database.pool(), new_site)
            .await
            .expect("can create test site");

        // Update site status
        let update = docs_mcp::database::sqlite::SiteUpdate {
            status: Some(status),
            ..Default::default()
        };
        database
            .update_site(site.id, &update)
            .await
            .expect("can update site status");
    }

    // Test progress tracking
    let indexer = BackgroundIndexer::new(config.clone())
        .await
        .expect("can create indexer");

    let status = indexer
        .get_indexing_status()
        .await
        .expect("can get indexing status");
    assert_eq!(status, IndexingStatus::Idle);

    // Test performance metrics with multiple sites
    let metrics = indexer
        .get_performance_metrics()
        .await
        .expect("can get performance metrics");
    assert_eq!(metrics.total_sites_processed, 3);

    // Test queue manager with multiple sites
    let queue_config = QueueConfig::default();
    let queue_manager = QueueManager::new(database.clone(), queue_config);

    // Test global queue statistics
    let global_stats = queue_manager
        .get_queue_stats(None)
        .await
        .expect("can get global queue stats");
    assert_eq!(global_stats.total_count, 0); // No queue items added yet

    // Test performance metrics collection
    let performance_metrics = queue_manager
        .get_performance_metrics(60)
        .await
        .expect("can get queue performance metrics");
    assert_eq!(performance_metrics.items_processed_per_minute, 0.0);

    println!("✅ Progress tracking and status reporting test passed");
}

/// Test health monitoring and error detection
#[tokio::test]
async fn health_monitoring_and_error_detection() {
    let (config, database, _temp_dir) = create_test_setup().await.expect("can create test setup");

    let mut indexer = BackgroundIndexer::new(config.clone())
        .await
        .expect("can create indexer");

    // Test stale lock file detection
    let stale_timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("can get current time")
        .as_secs()
        - 1200; // 20 minutes ago

    tokio::fs::write(&indexer.lock_file_path, stale_timestamp.to_string())
        .await
        .expect("can write stale lock file");

    // Should detect stale lock file and clean it up
    let is_running = indexer
        .is_indexer_running()
        .await
        .expect("can check if indexer is running");
    assert!(!is_running, "Should detect stale lock file as not running");
    assert!(
        !indexer.lock_file_path.exists(),
        "Stale lock file should be removed"
    );

    // Test stale heartbeat detection
    indexer
        .create_lock_file()
        .await
        .expect("can create lock file");

    // Set stale heartbeat (older than 60 seconds)
    let stale_heartbeat = chrono::Utc::now().naive_utc() - chrono::Duration::seconds(120);
    database
        .set_indexer_heartbeat(stale_heartbeat)
        .await
        .expect("can set stale heartbeat");

    // Should detect stale heartbeat and clean up
    let is_running = indexer
        .is_indexer_running()
        .await
        .expect("can check if indexer is running");
    assert!(!is_running, "Should detect stale heartbeat as not running");

    // Test consistency validation error detection
    let consistency_report = indexer
        .validate_consistency()
        .await
        .expect("can validate consistency");
    assert!(consistency_report.is_consistent, "Should be consistent");

    println!("✅ Health monitoring and error detection test passed");
}

/// Test resource management under various load conditions
#[tokio::test]
async fn resource_management_under_load() {
    let (config, database, _temp_dir) = create_test_setup().await.expect("can create test setup");

    // Create test site
    let new_site = NewSite {
        name: "Load Test Site".to_string(),
        base_url: "https://loadtest.com".to_string(),
        version: "1.0".to_string(),
    };
    let site = SiteQueries::create(database.pool(), new_site)
        .await
        .expect("can create test site");

    // Test queue manager under load
    let queue_config = QueueConfig {
        batch_size: 10,
        ..Default::default()
    };
    let mut queue_manager = QueueManager::new(database.clone(), queue_config);

    // Add many URLs to simulate load
    for i in 1..=100 {
        queue_manager
            .add_url_with_priority(
                site.id,
                format!("https://loadtest.com/page{}", i),
                docs_mcp::indexer::QueuePriority::Normal,
            )
            .await
            .expect("can add URL under load");
    }

    // Test batch processing under load
    let batch = queue_manager
        .get_next_batch(site.id)
        .await
        .expect("can get batch under load");
    assert_eq!(batch.len(), 10, "Should respect batch size limit");

    // Test queue statistics under load
    let stats = queue_manager
        .get_queue_stats(Some(site.id))
        .await
        .expect("can get queue stats under load");
    assert_eq!(stats.total_count, 100);
    assert_eq!(stats.pending_count, 90); // 10 are processing
    assert_eq!(stats.processing_count, 10);

    // Test cleanup operations under load
    let cleanup_count = queue_manager
        .cleanup_old_items(Some(site.id))
        .await
        .expect("can cleanup under load");
    assert_eq!(cleanup_count, 0, "No old items to clean yet");

    // Test performance metrics under load
    let performance_metrics = queue_manager
        .get_performance_metrics(60)
        .await
        .expect("can get performance metrics under load");
    assert!(performance_metrics.items_processed_per_minute >= 0.0);

    // Test indexer performance metrics under load
    let indexer = BackgroundIndexer::new(config.clone())
        .await
        .expect("can create indexer");

    let indexer_metrics = indexer
        .get_performance_metrics()
        .await
        .expect("can get indexer performance metrics");
    assert_eq!(indexer_metrics.total_sites_processed, 1);
}

/// Test cleanup procedures for various scenarios
#[tokio::test]
async fn cleanup_procedures() {
    let (config, database, _temp_dir) = create_test_setup().await.expect("can create test setup");

    // Create test site
    let new_site = NewSite {
        name: "Cleanup Test Site".to_string(),
        base_url: "https://cleanup.com".to_string(),
        version: "1.0".to_string(),
    };
    let site = SiteQueries::create(database.pool(), new_site)
        .await
        .expect("can create test site");

    let mut indexer = BackgroundIndexer::new(config.clone())
        .await
        .expect("can create indexer");

    // Test lock file cleanup
    indexer
        .create_lock_file()
        .await
        .expect("can create lock file");
    assert!(indexer.lock_file_path.exists());

    indexer
        .cleanup_lock_file()
        .await
        .expect("can cleanup lock file");
    assert!(!indexer.lock_file_path.exists());

    // Test queue cleanup
    let queue_config = QueueConfig {
        cleanup_age_seconds: 1, // Very short cleanup age for testing
        ..Default::default()
    };
    let queue_manager = QueueManager::new(database.clone(), queue_config.clone());

    // Add and complete some items
    for i in 1..=3 {
        let item = queue_manager
            .add_url_with_priority(
                site.id,
                format!("https://cleanup.com/page{}", i),
                docs_mcp::indexer::QueuePriority::Normal,
            )
            .await
            .expect("can add URL");

        // Manually mark as completed with old timestamp
        let old_time = chrono::Utc::now().naive_utc() - chrono::Duration::hours(25);
        sqlx::query!(
            "UPDATE crawl_queue SET status = 'completed', created_date = ? WHERE id = ?",
            old_time,
            item.id
        )
        .execute(database.pool())
        .await
        .expect("can update item as old completed");
    }

    // Test cleanup
    let cleanup_count = queue_manager
        .cleanup_old_items(Some(site.id))
        .await
        .expect("can cleanup old items");
    assert_eq!(cleanup_count, 3);

    // Test stuck item recovery
    let mut queue_manager_mut = QueueManager::new(database.clone(), queue_config);
    let reset_count = queue_manager_mut
        .reset_stuck_items()
        .await
        .expect("can reset stuck items");
    assert_eq!(reset_count, 0, "No stuck items to reset");

    // Test optimization cleanup
    let optimization_result = indexer
        .optimize_performance()
        .await
        .expect("can optimize performance");
    assert!(!optimization_result.is_empty());

    println!("✅ Cleanup procedures test passed");
}

/// Test that queue manager resource methods are properly integrated
#[tokio::test]
async fn queue_manager_integration() {
    let (config, _database, _temp_dir) = create_test_setup().await.expect("can create test setup");

    let mut indexer = BackgroundIndexer::new(config.clone())
        .await
        .expect("can create indexer");

    // Test queue resource usage stats
    let queue_usage = indexer.get_queue_resource_usage();
    assert_eq!(
        queue_usage.processing_items_tracked, 0,
        "Should start with no processing items"
    );
    assert!(
        queue_usage.estimated_memory_usage_mb >= 0.0,
        "Memory usage should be non-negative"
    );
    assert_eq!(
        queue_usage.active_batch_size, 64,
        "Should use default batch size"
    );
    assert_eq!(
        queue_usage.timeout_seconds, 300,
        "Should use default timeout"
    );

    // Test that optimize_performance includes queue operations
    let optimization_result = indexer
        .optimize_performance()
        .await
        .expect("can optimize performance");

    // Should include queue-related optimizations
    assert!(
        optimization_result.contains("Queue"),
        "Should mention queue optimizations"
    );
    assert!(
        optimization_result.contains("cleaned up"),
        "Should mention cleanup operations"
    );

    println!("✅ Queue manager integration test passed");
}
