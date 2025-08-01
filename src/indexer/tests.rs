use super::*;
use crate::config::OllamaConfig;
use tempfile::TempDir;

async fn create_test_indexer() -> Result<(Indexer, TempDir)> {
    let temp_dir = TempDir::new()?;
    let config = Config {
        ollama: OllamaConfig {
            protocol: "http".to_string(),
            host: "localhost".to_string(),
            port: 11434,
            model: "nomic-embed-text:latest".to_string(),
            batch_size: 32,
            embedding_dimension: None,
        },
        base_dir: temp_dir.path().to_path_buf(),
    };

    let indexer = Indexer::new(config).await?;
    Ok((indexer, temp_dir))
}

#[tokio::test]
async fn indexer_creation() {
    let result = create_test_indexer().await;
    assert!(result.is_ok(), "Should create indexer successfully");
}
