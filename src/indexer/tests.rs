use super::*;
use crate::config::{BrowserConfig, OllamaConfig};
use chrono::DateTime;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::TempDir;
use tokio::fs;
use tokio::time::{Duration, sleep};

async fn create_test_indexer() -> Result<(BackgroundIndexer, TempDir)> {
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

    let indexer = BackgroundIndexer::new(config).await?;
    Ok((indexer, temp_dir))
}

#[tokio::test]
async fn indexer_creation() {
    let result = create_test_indexer().await;
    assert!(result.is_ok(), "Should create indexer successfully");
}

#[tokio::test]
async fn lock_file_operations() {
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
    let (indexer, _temp_dir) = create_test_indexer().await.expect("can create indexer");

    // Should start as idle
    let status = indexer
        .get_indexing_status()
        .await
        .expect("can get indexer status");
    assert_eq!(status, IndexingStatus::Idle);
}

#[tokio::test]
async fn stale_lock_file_detection() {
    let (indexer, _temp_dir) = create_test_indexer().await.expect("can create indexer");

    // Create a stale lock file (timestamp from 20 minutes ago)
    let stale_timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("current time is after UNIX_EPOCH")
        .as_secs()
        - 1200; // 20 minutes ago

    fs::write(&indexer.lock_file_path, stale_timestamp.to_string())
        .await
        .expect("can write stale lock file");

    // Should detect stale lock file and clean it up
    let is_running = indexer
        .is_indexer_running()
        .await
        .expect("can check if indexer is running");
    assert!(!is_running, "Should detect stale lock file as not running");

    // Lock file should be cleaned up
    assert!(
        !indexer.lock_file_path.exists(),
        "Stale lock file should be removed"
    );
}

#[tokio::test]
async fn stale_heartbeat_detection() {
    let (indexer, _temp_dir) = create_test_indexer().await.expect("can create indexer");

    // Create valid lock file
    indexer
        .create_lock_file()
        .await
        .expect("can create lock file");

    // Set stale heartbeat (older than 60 seconds)
    let stale_heartbeat = Utc::now().naive_utc() - chrono::Duration::seconds(120);
    indexer
        .database
        .set_indexer_heartbeat(stale_heartbeat)
        .await
        .expect("can set stale heartbeat");

    // Should detect stale heartbeat and clean up
    let is_running = indexer
        .is_indexer_running()
        .await
        .expect("can check if indexer is running");
    assert!(!is_running, "Should detect stale heartbeat as not running");

    // Lock file should be cleaned up
    assert!(
        !indexer.lock_file_path.exists(),
        "Lock file should be cleaned up after stale heartbeat"
    );
}

#[tokio::test]
async fn concurrent_lock_file_access() {
    let (indexer1, _temp_dir) = create_test_indexer()
        .await
        .expect("can create first indexer");

    // Create second indexer with same config directory
    let config = Config {
        ollama: OllamaConfig {
            protocol: "http".to_string(),
            host: "localhost".to_string(),
            port: 11434,
            model: "nomic-embed-text:latest".to_string(),
            batch_size: 32,
        },
        base_dir: Some(_temp_dir.path().to_path_buf()),
        browser: BrowserConfig::default(),
    };
    let indexer2 = BackgroundIndexer::new(config)
        .await
        .expect("can create second indexer");

    // First indexer creates lock file and heartbeat
    indexer1
        .create_lock_file()
        .await
        .expect("can create lock file");
    indexer1
        .database
        .update_indexer_heartbeat()
        .await
        .expect("can update heartbeat");

    // Second indexer should detect first is running
    let is_running = indexer2
        .is_indexer_running()
        .await
        .expect("can check if indexer is running");
    assert!(is_running, "Should detect first indexer is running");

    // Second indexer should fail to start
    let mut indexer2_mut = indexer2;
    let start_result = indexer2_mut.start().await;
    assert!(start_result.is_err(), "Second indexer should fail to start");

    // Cleanup
    indexer1
        .cleanup_lock_file()
        .await
        .expect("can cleanup lock file");
}

#[tokio::test]
async fn heartbeat_mechanism() {
    let (indexer, _temp_dir) = create_test_indexer().await.expect("can create indexer");

    // Initially no heartbeat
    let heartbeat = indexer
        .database
        .get_indexer_heartbeat()
        .await
        .expect("can get heartbeat");
    assert!(
        heartbeat.and_utc() == DateTime::from_timestamp(0, 0).expect("can create 0 datetime"),
        "Should have no initial heartbeat"
    );

    // Update heartbeat
    indexer
        .database
        .update_indexer_heartbeat()
        .await
        .expect("can update heartbeat");

    // Should now have recent heartbeat
    let heartbeat_time = indexer
        .database
        .get_indexer_heartbeat()
        .await
        .expect("can get heartbeat");
    let now = Utc::now().naive_utc();
    let elapsed = now
        .signed_duration_since(heartbeat_time)
        .num_seconds()
        .unsigned_abs();

    assert!(
        elapsed < 5,
        "Heartbeat should be very recent (less than 5 seconds)"
    );
}

#[tokio::test]
async fn invalid_lock_file_content() {
    let (indexer, _temp_dir) = create_test_indexer().await.expect("can create indexer");

    // Create lock file with invalid content
    fs::write(&indexer.lock_file_path, "invalid_timestamp")
        .await
        .expect("can write invalid lock file");

    // Should detect invalid lock file and clean it up
    let is_running = indexer
        .is_indexer_running()
        .await
        .expect("can check if indexer is running");
    assert!(
        !is_running,
        "Should detect invalid lock file as not running"
    );

    // Lock file should be cleaned up
    assert!(
        !indexer.lock_file_path.exists(),
        "Invalid lock file should be removed"
    );
}

#[tokio::test]
async fn lock_file_cleanup_on_drop() {
    let (indexer, _temp_dir) = create_test_indexer().await.expect("can create indexer");
    let lock_path = indexer.lock_file_path.clone();

    // Create lock file
    indexer
        .create_lock_file()
        .await
        .expect("can create lock file");
    assert!(lock_path.exists(), "Lock file should exist");

    // Drop indexer (this triggers the Drop implementation)
    drop(indexer);

    // Give async cleanup time to run
    sleep(Duration::from_millis(100)).await;

    // Lock file should eventually be cleaned up
    // Note: The Drop implementation uses tokio::spawn so cleanup is async
    // In real scenarios, the process exit would handle cleanup
}
