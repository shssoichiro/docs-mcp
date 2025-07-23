use super::*;
use crate::config::OllamaConfig;
use std::env;
use tempfile::TempDir;

async fn create_test_indexer() -> Result<(BackgroundIndexer, TempDir)> {
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

    let indexer = BackgroundIndexer::new(config).await?;
    Ok((indexer, temp_dir))
}

#[tokio::test]
async fn indexer_creation() {
    if env::var("SKIP_OLLAMA_TESTS").is_ok() {
        return;
    }

    let result = create_test_indexer().await;
    assert!(result.is_ok(), "Should create indexer successfully");
}

#[tokio::test]
async fn lock_file_operations() {
    if env::var("SKIP_OLLAMA_TESTS").is_ok() {
        return;
    }

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
    if env::var("SKIP_OLLAMA_TESTS").is_ok() {
        return;
    }

    let (indexer, _temp_dir) = create_test_indexer().await.expect("can create indexer");

    // Should start as idle
    let status = indexer
        .get_indexing_status()
        .await
        .expect("can get indexer status");
    assert_eq!(status, IndexingStatus::Idle);
}
