#![expect(
    clippy::tests_outside_test_module,
    reason = "integration tests are only compiled in test mode"
)]

// Integration tests for complete background indexing system
// Tests the integration between queue management, background indexer, and CLI commands

use tempfile::TempDir;

use docs_mcp::config::{BrowserConfig, Config, OllamaConfig};
use docs_mcp::database::sqlite::{Database, NewSite, SiteQueries, SiteStatus};
use docs_mcp::indexer::{BackgroundIndexer, IndexingStatus};

/// Create a test configuration and database setup
async fn create_test_setup() -> anyhow::Result<(Config, Database, TempDir)> {
    let temp_dir = TempDir::new()?;
    let config = Config {
        ollama: OllamaConfig {
            protocol: "http".to_string(),
            host: "localhost".to_string(),
            port: 11434,
            model: "nomic-embed-text:latest".to_string(),
            batch_size: 32,
        },
        base_dir: Some(temp_dir.path().to_path_buf()),
        browser: BrowserConfig::default(),
    };

    let database = Database::new(&config.database_path()).await?;
    Ok((config, database, temp_dir))
}

/// Test complete background indexing workflow integration
#[tokio::test]
async fn complete_background_indexing_workflow() {
    let (config, _database, _temp_dir) = create_test_setup().await.expect("can create test setup");

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

    // Test consistency validation
    let consistency_report = indexer
        .validate_consistency()
        .await
        .expect("can validate consistency");
    assert!(
        consistency_report.is_consistent,
        "Should be consistent initially"
    );
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
}

/// Test cleanup procedures for various scenarios
#[tokio::test]
async fn cleanup_procedures() {
    let (config, _database, _temp_dir) = create_test_setup().await.expect("can create test setup");

    let indexer = BackgroundIndexer::new(config.clone())
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
}
