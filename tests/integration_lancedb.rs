#![expect(
    clippy::tests_outside_test_module,
    reason = "integration tests are only compiled in test mode"
)]

/// Integration tests for LanceDB vector store with realistic data
use docs_mcp::{
    config::OllamaConfig,
    database::lancedb::{ChunkMetadata, EmbeddingRecord, VectorStore},
    embeddings::chunking::ChunkingConfig,
};
use tempfile::TempDir;
use uuid::Uuid;

use docs_mcp::config::Config;

fn create_test_config() -> (Config, TempDir) {
    let temp_dir = TempDir::new().expect("should create temp dir");
    let config = Config {
        base_dir: temp_dir.path().to_path_buf(),
        ollama: OllamaConfig::default(),
        chunking: ChunkingConfig::default(),
    };
    (config, temp_dir)
}

fn create_realistic_embedding_record(
    id: &str,
    site_id: &str,
    page_title: &str,
    content: &str,
    vector_variation: f32,
) -> EmbeddingRecord {
    // Create a realistic 768-dimensional vector (nomic-embed-text dimension)
    let base_vector: Vec<f32> = (0..768)
        .map(|i| {
            let base = (i as f32).mul_add(0.01, vector_variation).sin() * 0.1;
            (content.len() as f32).mul_add(0.001, base)
        })
        .collect();

    EmbeddingRecord {
        id: id.to_string(),
        vector: base_vector,
        metadata: ChunkMetadata {
            chunk_id: format!("chunk_{}", Uuid::new_v4()),
            site_id: site_id.to_string(),
            page_title: page_title.to_string(),
            page_url: format!(
                "https://docs.example.com/{}",
                page_title.to_lowercase().replace(' ', "-")
            ),
            heading_path: if content.contains("Installation") {
                Some("Getting Started > Installation".to_string())
            } else if content.contains("API") {
                Some("Reference > API".to_string())
            } else if content.contains("Tutorial") {
                Some("Guides > Tutorial".to_string())
            } else {
                None
            },
            content: content.to_string(),
            token_count: content.split_whitespace().count() as u32,
            chunk_index: id.parse::<u32>().unwrap_or(0),
            created_at: chrono::Utc::now().to_rfc3339(),
        },
    }
}

fn create_documentation_dataset() -> Vec<EmbeddingRecord> {
    vec![
        create_realistic_embedding_record(
            "1",
            "rust_docs",
            "Getting Started",
            "Rust is a systems programming language that runs blazingly fast, prevents segfaults, and guarantees thread safety. Installation is simple with rustup, the Rust toolchain installer.",
            0.1,
        ),
        create_realistic_embedding_record(
            "2",
            "rust_docs",
            "Ownership and Borrowing",
            "Ownership is Rust's most unique feature and has deep implications for the rest of the language. It enables Rust to make memory safety guarantees without needing a garbage collector.",
            0.2,
        ),
        create_realistic_embedding_record(
            "3",
            "python_docs",
            "Python Tutorial",
            "Python is an easy to learn, powerful programming language. It has efficient high-level data structures and a simple but effective approach to object-oriented programming.",
            0.3,
        ),
        create_realistic_embedding_record(
            "4",
            "python_docs",
            "Data Structures",
            "This chapter describes some things you've learned about already in more detail, and adds some new things as well. Tutorial on lists, dictionaries, and sets in Python.",
            0.4,
        ),
        create_realistic_embedding_record(
            "5",
            "rust_docs",
            "Error Handling",
            "Rust groups errors into two major categories: recoverable and unrecoverable errors. For recoverable errors we have Result<T, E> and for unrecoverable errors we have the panic! macro.",
            0.15,
        ),
        create_realistic_embedding_record(
            "6",
            "javascript_docs",
            "JavaScript Basics",
            "JavaScript is a programming language that adds interactivity to your website. This happens in games, in the behavior of responses when buttons are pressed.",
            0.5,
        ),
        create_realistic_embedding_record(
            "7",
            "rust_docs",
            "Cargo Package Manager",
            "Cargo is Rust's build system and package manager. Most Rustaceans use this tool to manage their Rust projects because Cargo handles a lot of tasks for you.",
            0.12,
        ),
        create_realistic_embedding_record(
            "8",
            "python_docs",
            "API Reference",
            "The Python standard library is very extensive, offering a wide range of facilities. The library contains built-in modules written in C that provide access to system functionality.",
            0.35,
        ),
    ]
}

#[tokio::test]
async fn realistic_documentation_storage_and_search() {
    let (config, _temp_dir) = create_test_config();
    let mut store = VectorStore::new(&config)
        .await
        .expect("should create vector store");

    // Store realistic documentation data
    let dataset = create_documentation_dataset();
    let result = store.store_embeddings_batch(dataset.clone()).await;
    assert!(
        result.is_ok(),
        "Failed to store documentation dataset: {:?}",
        result.err()
    );

    // Verify all records were stored
    let count = store
        .count_embeddings()
        .await
        .expect("count embeddings should succeed");
    assert_eq!(count, dataset.len() as u64);

    // Test semantic search for Rust-related content
    let rust_query_vector = &dataset[0].vector; // Use Rust getting started vector as query
    let rust_results = store
        .search_similar(rust_query_vector, 5, Some("rust_docs"))
        .await
        .expect("search should succeed");

    assert!(!rust_results.is_empty(), "Should find Rust documentation");
    assert!(
        rust_results.len() <= 4,
        "Should find at most 4 Rust docs (excluding the exact match if it exists)"
    );

    // Verify all results are from rust_docs site
    for result in &rust_results {
        assert_eq!(result.chunk_metadata.site_id, "rust_docs");
        assert!(
            result.similarity_score > 0.0,
            "Similarity score should be positive"
        );
    }

    // Test cross-site search without filter
    let general_results = store
        .search_similar(rust_query_vector, 8, None)
        .await
        .expect("search should succeed");
    assert!(general_results.len() <= 8, "Should respect limit");

    // Should find results from multiple sites
    let unique_sites: std::collections::HashSet<_> = general_results
        .iter()
        .map(|r| &r.chunk_metadata.site_id)
        .collect();
    assert!(
        unique_sites.len() > 1,
        "Should find results from multiple sites"
    );
}

#[tokio::test]
async fn search_relevance_ranking() {
    let (config, _temp_dir) = create_test_config();
    let mut store = VectorStore::new(&config)
        .await
        .expect("should create vector store");

    let dataset = create_documentation_dataset();
    store
        .store_embeddings_batch(dataset.clone())
        .await
        .expect("should store embeddings successfully");

    // Use a Rust ownership query vector
    let ownership_query = &dataset[1].vector; // Ownership and Borrowing content
    let results = store
        .search_similar(ownership_query, 5, None)
        .await
        .expect("search should succeed");

    assert!(!results.is_empty(), "Should find relevant results");

    // Results should be ordered by similarity (highest first)
    for i in 1..results.len() {
        assert!(
            results[i - 1].similarity_score >= results[i].similarity_score,
            "Results should be ordered by similarity score (descending)"
        );
    }

    // The most similar result should be Rust-related content
    let top_result = &results[0];
    assert!(
        top_result.chunk_metadata.site_id == "rust_docs"
            || top_result
                .chunk_metadata
                .content
                .to_lowercase()
                .contains("rust")
            || top_result
                .chunk_metadata
                .content
                .to_lowercase()
                .contains("ownership"),
        "Top result should be related to the query"
    );
}

#[tokio::test]
async fn large_batch_processing() {
    let (config, _temp_dir) = create_test_config();
    let mut store = VectorStore::new(&config)
        .await
        .expect("should create vector store");

    // Create a large batch of embeddings
    let mut large_dataset = Vec::new();
    for i in 0..100 {
        large_dataset.push(create_realistic_embedding_record(
                &i.to_string(),
                &format!("site_{}", i % 5), // 5 different sites
                &format!("Page {}", i),
                &format!("This is content for page {} with unique information about topic {}. It contains detailed explanations and examples.", i, i % 10),
                i as f32 * 0.01,
            ));
    }

    // Store the large batch
    let start_time = std::time::Instant::now();
    let result = store.store_embeddings_batch(large_dataset.clone()).await;
    let storage_duration = start_time.elapsed();

    assert!(
        result.is_ok(),
        "Failed to store large batch: {:?}",
        result.err()
    );
    assert!(
        storage_duration.as_secs() < 30,
        "Storage should complete within 30 seconds"
    );

    // Verify count
    let count = store
        .count_embeddings()
        .await
        .expect("count embeddings should succeed");
    assert_eq!(count, large_dataset.len() as u64);

    // Test search performance
    let search_start = std::time::Instant::now();
    let query_vector = &large_dataset[0].vector;
    let results = store
        .search_similar(query_vector, 20, None)
        .await
        .expect("search should succeed");
    let search_duration = search_start.elapsed();

    assert!(!results.is_empty(), "Should find results in large dataset");
    assert!(
        search_duration.as_secs() < 10,
        "Search should complete within 10 seconds"
    );
    assert!(results.len() <= 20, "Should respect search limit");
}

#[tokio::test]
async fn metadata_preservation_and_filtering() {
    let (config, _temp_dir) = create_test_config();
    let mut store = VectorStore::new(&config)
        .await
        .expect("should create vector store");

    let dataset = create_documentation_dataset();
    store
        .store_embeddings_batch(dataset.clone())
        .await
        .expect("should store embeddings successfully");

    // Search and verify metadata is preserved
    let query_vector = &dataset[0].vector;
    let results = store
        .search_similar(query_vector, 5, None)
        .await
        .expect("search should succeed");

    for result in &results {
        let metadata = &result.chunk_metadata;

        // Verify all metadata fields are populated
        assert!(
            !metadata.chunk_id.is_empty(),
            "Chunk ID should not be empty"
        );
        assert!(!metadata.site_id.is_empty(), "Site ID should not be empty");
        assert!(
            !metadata.page_title.is_empty(),
            "Page title should not be empty"
        );
        assert!(
            !metadata.page_url.is_empty(),
            "Page URL should not be empty"
        );
        assert!(!metadata.content.is_empty(), "Content should not be empty");
        assert!(metadata.token_count > 0, "Token count should be positive");
        assert!(
            !metadata.created_at.is_empty(),
            "Created at should not be empty"
        );

        // Verify URL format
        assert!(
            metadata.page_url.starts_with("https://"),
            "URL should use HTTPS: {}",
            metadata.page_url
        );

        // Verify token count reasonably matches content
        let word_count = metadata.content.split_whitespace().count() as u32;
        assert!(
            metadata.token_count >= word_count * 70 / 100, // Allow some variance for tokenization
            "Token count {} should be reasonably close to word count {}",
            metadata.token_count,
            word_count
        );
    }
}

#[tokio::test]
async fn site_deletion_integrity() {
    let (config, _temp_dir) = create_test_config();
    let mut store = VectorStore::new(&config)
        .await
        .expect("should create vector store");

    let dataset = create_documentation_dataset();
    store
        .store_embeddings_batch(dataset.clone())
        .await
        .expect("should store embeddings");

    // Delete rust_docs site
    let result = store.delete_site_embeddings("rust_docs").await;
    assert!(
        result.is_ok(),
        "Failed to delete rust_docs embeddings: {:?}",
        result.err()
    );

    // Verify remaining embeddings
    let query_vector = &dataset[0].vector;
    let remaining_results = store
        .search_similar(query_vector, 20, None)
        .await
        .expect("search should succeed");

    // Should have no rust_docs results
    for result in &remaining_results {
        assert_ne!(
            result.chunk_metadata.site_id, "rust_docs",
            "Should not find any rust_docs after deletion"
        );
    }

    // Should still have python_docs and javascript_docs
    let remaining_sites: std::collections::HashSet<_> = remaining_results
        .iter()
        .map(|r| r.chunk_metadata.site_id.as_str())
        .collect();
    assert!(
        remaining_sites.contains("python_docs"),
        "Should still have python_docs"
    );
    assert!(
        remaining_sites.contains("javascript_docs"),
        "Should still have javascript_docs"
    );
}

#[tokio::test]
async fn vector_index_creation_and_performance() {
    let (config, _temp_dir) = create_test_config();
    let mut store = VectorStore::new(&config)
        .await
        .expect("should create vector store");

    // Store enough data for vector index creation (LanceDB requires 256+ rows for PQ training)
    let mut dataset = Vec::new();
    for i in 0..300 {
        dataset.push(create_realistic_embedding_record(
            &i.to_string(),
            "performance_test",
            &format!("Performance Test Page {}", i),
            &format!(
                "Performance test content for page {} with various keywords and information.",
                i
            ),
            i as f32 * 0.02,
        ));
    }

    store
        .store_embeddings_batch(dataset.clone())
        .await
        .expect("should store embeddings");

    // Measure search time before index
    let query_vector = &dataset[0].vector;
    let start = std::time::Instant::now();
    let _results_before = store
        .search_similar(query_vector, 10, None)
        .await
        .expect("search should succeed");
    let time_before_index = start.elapsed();

    // Create vector index
    let index_result = store.create_vector_index().await;
    assert!(
        index_result.is_ok(),
        "Failed to create vector index: {:?}",
        index_result.err()
    );

    // Measure search time after index
    let start = std::time::Instant::now();
    let results_after = store
        .search_similar(query_vector, 10, None)
        .await
        .expect("search should succeed");
    let time_after_index = start.elapsed();

    // Verify search still works correctly
    assert!(
        !results_after.is_empty(),
        "Search should still work after indexing"
    );
    assert!(
        results_after.len() <= 10,
        "Should respect limit after indexing"
    );

    // Log timing information (index may or may not improve performance with small datasets)
    eprintln!("Search time before index: {:?}", time_before_index);
    eprintln!("Search time after index: {:?}", time_after_index);
}

#[tokio::test]
async fn database_optimization() {
    let (config, _temp_dir) = create_test_config();
    let mut store = VectorStore::new(&config)
        .await
        .expect("should create vector store");

    // Store some data
    let dataset = create_documentation_dataset();
    store
        .store_embeddings_batch(dataset.clone())
        .await
        .expect("should store embeddings");

    // Delete some data to create fragmentation
    let _result = store
        .delete_site_embeddings("javascript_docs")
        .await
        .expect("delete embeddings should succeed");

    // Optimize the database
    let optimize_result = store.optimize().await;
    assert!(
        optimize_result.is_ok(),
        "Failed to optimize database: {:?}",
        optimize_result.err()
    );

    // Verify search still works after optimization
    let query_vector = &dataset[0].vector;
    let results = store
        .search_similar(query_vector, 5, None)
        .await
        .expect("search should succeed");
    assert!(!results.is_empty(), "Search should work after optimization");

    // Verify no javascript_docs remain
    for result in &results {
        assert_ne!(result.chunk_metadata.site_id, "javascript_docs");
    }
}

#[tokio::test]
async fn database_corruption_recovery() {
    let (config, _temp_dir) = create_test_config();
    let mut store = VectorStore::new(&config)
        .await
        .expect("should create vector store");

    // Store some initial data
    let dataset = create_documentation_dataset();
    store
        .store_embeddings_batch(dataset.clone())
        .await
        .expect("should store embeddings");

    // Verify initial state
    let count = store
        .count_embeddings()
        .await
        .expect("count embeddings should succeed");
    assert_eq!(count, dataset.len() as u64);

    // Test that we can recreate the store (simulating recovery)
    drop(store);
    let recovered_store = VectorStore::new(&config)
        .await
        .expect("should create vector store");

    // Verify data is still accessible after recovery
    let recovered_count = recovered_store
        .count_embeddings()
        .await
        .expect("count embeddings should succeed");
    assert_eq!(recovered_count, dataset.len() as u64);

    // Verify search functionality after recovery
    let query_vector = &dataset[0].vector;
    let results = recovered_store
        .search_similar(query_vector, 5, None)
        .await
        .expect("search should succeed");
    assert!(!results.is_empty(), "Search should work after recovery");
}

#[tokio::test]
async fn concurrent_access_simulation() {
    let (config, _temp_dir) = create_test_config();
    let mut store = VectorStore::new(&config)
        .await
        .expect("should create vector store");

    // Store initial data
    let dataset = create_documentation_dataset();
    store
        .store_embeddings_batch(dataset.clone())
        .await
        .expect("should store embeddings");

    // Simulate concurrent operations by performing multiple operations in sequence
    // (Note: actual concurrency would require Arc<Mutex<VectorStore>> or similar)

    let query_vector = &dataset[0].vector;

    // Multiple searches
    for i in 0..5 {
        let results = store
            .search_similar(query_vector, 3, None)
            .await
            .expect("search should succeed");
        assert!(!results.is_empty(), "Search {} should return results", i);
    }

    // Add more data while searching
    let additional_record = create_realistic_embedding_record(
        "concurrent_test",
        "concurrent_site",
        "Concurrent Test Page",
        "This is content added during concurrent access testing",
        0.99,
    );

    store
        .store_embedding(additional_record)
        .await
        .expect("should store embedding successfully");

    // Verify the new data is searchable
    let final_results = store
        .search_similar(query_vector, 10, None)
        .await
        .expect("search should succeed");
    let has_concurrent_data = final_results
        .iter()
        .any(|r| r.chunk_metadata.site_id == "concurrent_site");
    assert!(
        has_concurrent_data,
        "concurrent site should be in search results"
    );

    // Note: Due to vector similarity, the concurrent data might not always be in top results
    // but the count should increase
    let final_count = store
        .count_embeddings()
        .await
        .expect("count embeddings should succeed");
    assert_eq!(final_count, dataset.len() as u64 + 1);
}
