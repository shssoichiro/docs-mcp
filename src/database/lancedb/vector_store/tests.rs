use crate::{config::settings::OllamaConfig, embeddings::chunking::ChunkingConfig};

use super::*;
use tempfile::TempDir;

fn create_test_config() -> (Config, TempDir) {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let config = Config {
        base_dir: temp_dir.path().to_path_buf(),
        ollama: OllamaConfig {
            embedding_dimension: 5,
            ..OllamaConfig::default()
        },
        chunking: ChunkingConfig::default(),
    };
    (config, temp_dir)
}

fn create_test_embedding_record(id: &str, site_id: &str) -> EmbeddingRecord {
    // Create a consistent test vector with the same dimensions for all tests
    let mut test_vector = vec![0.1, 0.2, 0.3, 0.4, 0.5];
    // Add some variation based on the id to make vectors slightly different
    let id_num: f32 = id.parse().unwrap_or(1.0);
    for (i, val) in test_vector.iter_mut().enumerate() {
        *val += id_num.mul_add(0.01, i as f32 * 0.001);
    }

    EmbeddingRecord {
        id: id.to_string(),
        vector: test_vector, // 5-dimensional vector for testing
        metadata: ChunkMetadata {
            chunk_id: format!("chunk_{}", id),
            site_id: site_id.to_string(),
            page_title: "Test Page".to_string(),
            page_url: "https://example.com/test".to_string(),
            heading_path: Some("Section > Subsection".to_string()),
            content: format!("This is test content for chunk {}", id),
            token_count: 25,
            chunk_index: 0,
            created_at: "2024-01-01T00:00:00Z".to_string(),
        },
    }
}

#[tokio::test]
async fn vector_store_initialization() {
    let (config, _temp_dir) = create_test_config();

    let result = VectorStore::new(&config).await;
    assert!(
        result.is_ok(),
        "Failed to initialize VectorStore: {:?}",
        result.err()
    );

    let store = result.expect("should get result successfully");
    assert_eq!(store.table_name, "embeddings");
}

#[tokio::test]
async fn store_single_embedding() {
    let (config, _temp_dir) = create_test_config();
    let mut store = VectorStore::new(&config)
        .await
        .expect("should create vector store");

    let record = create_test_embedding_record("test_1", "site_1");
    let result = store.store_embeddings_batch(vec![record]).await;

    assert!(
        result.is_ok(),
        "Failed to store embedding: {:?}",
        result.err()
    );

    // Verify the embedding was stored
    let count = store
        .count_embeddings()
        .await
        .expect("should count embeddings successfully");
    assert_eq!(count, 1);
}

#[tokio::test]
async fn store_batch_embeddings() {
    let (config, _temp_dir) = create_test_config();
    let mut store = VectorStore::new(&config)
        .await
        .expect("should create vector store");

    let records = vec![
        create_test_embedding_record("test_1", "site_1"),
        create_test_embedding_record("test_2", "site_1"),
        create_test_embedding_record("test_3", "site_2"),
    ];

    let result = store.store_embeddings_batch(records).await;
    assert!(
        result.is_ok(),
        "Failed to store embeddings batch: {:?}",
        result.err()
    );

    // Verify all embeddings were stored
    let count = store
        .count_embeddings()
        .await
        .expect("should count embeddings successfully");
    assert_eq!(count, 3);
}

#[tokio::test]
async fn search_similar_embeddings() {
    let (config, _temp_dir) = create_test_config();
    let mut store = VectorStore::new(&config)
        .await
        .expect("should create vector store");

    // Store some test embeddings
    let records = vec![
        create_test_embedding_record("test_1", "site_1"),
        create_test_embedding_record("test_2", "site_1"),
        create_test_embedding_record("test_3", "site_2"),
    ];

    store
        .store_embeddings_batch(records)
        .await
        .expect("should store embeddings successfully");

    // Search for similar embeddings
    let query_vector = vec![0.1, 0.2, 0.3, 0.4, 0.5];
    let results = store
        .search_similar(&query_vector, 10, None)
        .await
        .expect("search should succeed");

    assert!(!results.is_empty(), "Should find similar embeddings");
    assert!(results.len() <= 3, "Should not return more than stored");

    // Verify result structure
    for result in &results {
        assert!(!result.chunk_metadata.chunk_id.is_empty());
        assert!(!result.chunk_metadata.content.is_empty());
        assert!(result.similarity_score >= 0.0 && result.similarity_score <= 1.0);
    }
}

#[tokio::test]
async fn search_with_site_filter() {
    let (config, _temp_dir) = create_test_config();
    let mut store = VectorStore::new(&config)
        .await
        .expect("should create vector store");

    // Store embeddings for different sites
    let records = vec![
        create_test_embedding_record("test_1", "site_1"),
        create_test_embedding_record("test_2", "site_1"),
        create_test_embedding_record("test_3", "site_2"),
    ];

    store
        .store_embeddings_batch(records)
        .await
        .expect("should store embeddings successfully");

    // Search with site filter
    let query_vector = vec![0.1, 0.2, 0.3, 0.4, 0.5];
    let results = store
        .search_similar(&query_vector, 10, Some("site_1"))
        .await
        .expect("search should succeed");

    assert!(!results.is_empty(), "Should find embeddings for site_1");

    // Verify all results are from site_1
    for result in &results {
        assert_eq!(result.chunk_metadata.site_id, "site_1");
    }
}

#[tokio::test]
async fn delete_site_embeddings() {
    let (config, _temp_dir) = create_test_config();
    let mut store = VectorStore::new(&config)
        .await
        .expect("should create vector store");

    // Store embeddings for different sites
    let records = vec![
        create_test_embedding_record("test_1", "site_1"),
        create_test_embedding_record("test_2", "site_1"),
        create_test_embedding_record("test_3", "site_2"),
    ];

    store
        .store_embeddings_batch(records)
        .await
        .expect("should store embeddings successfully");

    // Verify initial count
    let initial_count = store
        .count_embeddings()
        .await
        .expect("should count embeddings successfully");
    assert_eq!(initial_count, 3);

    // Delete embeddings for site_1
    let result = store.delete_site_embeddings("site_1").await;
    assert!(
        result.is_ok(),
        "Failed to delete site embeddings: {:?}",
        result.err()
    );

    // Verify only site_2 embeddings remain
    let query_vector = vec![0.1, 0.2, 0.3, 0.4, 0.5];
    let remaining_results = store
        .search_similar(&query_vector, 10, None)
        .await
        .expect("search should succeed");

    for result in &remaining_results {
        assert_eq!(result.chunk_metadata.site_id, "site_2");
    }
}

#[tokio::test]
async fn empty_batch_handling() {
    let (config, _temp_dir) = create_test_config();
    let mut store = VectorStore::new(&config)
        .await
        .expect("should create vector store");

    let result = store.store_embeddings_batch(vec![]).await;
    assert!(result.is_ok(), "Should handle empty batch gracefully");

    let count = store
        .count_embeddings()
        .await
        .expect("should count embeddings successfully");
    assert_eq!(count, 0);
}

#[tokio::test]
async fn optimize_database() {
    let (config, _temp_dir) = create_test_config();
    let mut store = VectorStore::new(&config)
        .await
        .expect("should create vector store");

    // Store some data first
    let record = create_test_embedding_record("test_1", "site_1");
    store
        .store_embeddings_batch(vec![record])
        .await
        .expect("should store embedding successfully");

    // Test optimization
    let result = store.optimize().await;
    assert!(
        result.is_ok(),
        "Failed to optimize database: {:?}",
        result.err()
    );
}
