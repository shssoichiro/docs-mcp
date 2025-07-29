#![expect(
    clippy::tests_outside_test_module,
    reason = "integration tests are only compiled in test mode"
)]

// Integration tests that require a local Ollama instance
// Run with: cargo test --test integration_ollama

use docs_mcp::config::{BrowserConfig, Config, OllamaConfig};
use docs_mcp::embeddings::chunking::{ContentChunk, estimate_token_count};
use docs_mcp::embeddings::ollama::OllamaClient;
use std::env;
use std::time::Duration;
use tracing::{debug, info};

const TEST_MODEL: &str = "nomic-embed-text:latest";
const DEFAULT_OLLAMA_HOST: &str = "localhost";
const DEFAULT_OLLAMA_PORT: u16 = 11434;

fn create_integration_test_client() -> OllamaClient {
    let host = env::var("OLLAMA_HOST").unwrap_or_else(|_| DEFAULT_OLLAMA_HOST.to_string());
    let port = env::var("OLLAMA_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_OLLAMA_PORT);
    let model = env::var("OLLAMA_MODEL").unwrap_or_else(|_| TEST_MODEL.to_string());

    let config = Config {
        ollama: OllamaConfig {
            host,
            port,
            model,
            batch_size: 5, // Smaller batch size for testing
        },
        base_dir: None,
        browser: BrowserConfig::default(),
    };

    OllamaClient::new(&config)
        .expect("Failed to create Ollama client")
        .with_timeout(Duration::from_secs(60)) // Longer timeout for embedding generation
        .with_retry_attempts(3)
}

fn init_test_tracing() {
    tracing_subscriber::fmt()
        .with_env_filter("debug")
        .with_test_writer()
        .try_init()
        .ok(); // Ignore error if already initialized
}

#[test]
fn real_ollama_health_check() {
    init_test_tracing();

    let client = create_integration_test_client();

    info!("Testing health check against real Ollama instance");
    let result = client.health_check();

    assert!(
        result.is_ok(),
        "Health check should succeed with local Ollama: {:?}",
        result
    );

    info!("Health check passed successfully");
}

#[test]

fn real_ollama_list_models() {
    init_test_tracing();

    let client = create_integration_test_client();

    info!("Testing model listing against real Ollama instance");
    let result = client.list_models();

    assert!(result.is_ok(), "Model listing should succeed: {:?}", result);

    let models = result.expect("models exist");
    assert!(
        !models.is_empty(),
        "Should have at least one model available"
    );

    info!("Found {} models", models.len());
    for model in &models {
        debug!("Available model: {} (size: {:?})", model.name, model.size);
    }

    // Check if our test model is available
    let has_test_model = models.iter().any(|m| m.name == TEST_MODEL);
    if !has_test_model {
        println!(
            "Warning: Test model '{}' not found. Available models: {:?}",
            TEST_MODEL,
            models.iter().map(|m| &m.name).collect::<Vec<_>>()
        );
    }
}

#[test]

fn real_ollama_single_embedding() {
    init_test_tracing();

    let client = create_integration_test_client();

    let test_text = "This is a test document about machine learning and artificial intelligence.";

    info!("Generating embedding for single text");
    let result = client.generate_embedding(test_text);

    assert!(
        result.is_ok(),
        "Single embedding generation should succeed: {:?}",
        result
    );

    let embedding_result = result.expect("embedding result succeeded");
    assert_eq!(embedding_result.text, test_text);
    assert!(
        !embedding_result.embedding.is_empty(),
        "Embedding should not be empty"
    );
    assert!(
        embedding_result.token_count > 0,
        "Token count should be positive"
    );

    info!(
        "Generated embedding with {} dimensions and {} tokens",
        embedding_result.embedding.len(),
        embedding_result.token_count
    );

    // Verify embedding dimensions are consistent with nomic-embed-text
    // (typically 768 dimensions for this model)
    assert!(
        embedding_result.embedding.len() >= 100,
        "Embedding should have reasonable number of dimensions"
    );
}

#[test]

fn real_ollama_batch_embeddings() {
    init_test_tracing();

    let client = create_integration_test_client();

    let test_texts = vec![
        "Document about artificial intelligence and machine learning.".to_string(),
        "Guide to web development with JavaScript and TypeScript.".to_string(),
        "Tutorial on database design and SQL optimization.".to_string(),
        "Introduction to cloud computing and microservices architecture.".to_string(),
    ];

    info!(
        "Generating embeddings for batch of {} texts",
        test_texts.len()
    );
    let result = client.generate_embeddings_batch(&test_texts);

    assert!(
        result.is_ok(),
        "Batch embedding generation should succeed: {:?}",
        result
    );

    let embedding_results = result.expect("embedding result succeeded");
    assert_eq!(
        embedding_results.len(),
        test_texts.len(),
        "Should have one embedding per input"
    );

    for (i, embedding_result) in embedding_results.iter().enumerate() {
        assert_eq!(embedding_result.text, test_texts[i]);
        assert!(
            !embedding_result.embedding.is_empty(),
            "Embedding {} should not be empty",
            i
        );
        assert!(
            embedding_result.token_count > 0,
            "Token count {} should be positive",
            i
        );

        debug!(
            "Embedding {}: {} dimensions, {} tokens",
            i,
            embedding_result.embedding.len(),
            embedding_result.token_count
        );
    }

    // Verify all embeddings have the same dimensionality
    let first_dim = embedding_results[0].embedding.len();
    for (i, result) in embedding_results.iter().enumerate() {
        assert_eq!(
            result.embedding.len(),
            first_dim,
            "Embedding {} should have consistent dimensions",
            i
        );
    }

    info!(
        "Successfully generated {} embeddings with {} dimensions each",
        embedding_results.len(),
        first_dim
    );
}

#[test]

fn real_ollama_chunk_embeddings() {
    init_test_tracing();

    let client = create_integration_test_client();

    let test_chunks = vec![
        ContentChunk {
            content: "Page: API Documentation\nSection: Authentication\n\nThis section covers API authentication using OAuth 2.0 and JWT tokens.".to_string(),
            heading_path: "Authentication".to_string(),
            chunk_index: 0,
            token_count: estimate_token_count("This section covers API authentication using OAuth 2.0 and JWT tokens."),
            has_code_blocks: false,
        },
        ContentChunk {
            content: "Page: API Documentation\nSection: Rate Limiting\n\n```python\nimport requests\nresponse = requests.get('https://api.example.com/data')\nprint(response.status_code)\n```".to_string(),
            heading_path: "Rate Limiting".to_string(),
            chunk_index: 1,
            token_count: estimate_token_count("import requests\nresponse = requests.get('https://api.example.com/data')\nprint(response.status_code)"),
            has_code_blocks: true,
        },
        ContentChunk {
            content: "Page: API Documentation\nSection: Error Handling\n\nAPI errors are returned with appropriate HTTP status codes and JSON error messages.".to_string(),
            heading_path: "Error Handling".to_string(),
            chunk_index: 2,
            token_count: estimate_token_count("API errors are returned with appropriate HTTP status codes and JSON error messages."),
            has_code_blocks: false,
        },
    ];

    info!(
        "Generating embeddings for {} content chunks",
        test_chunks.len()
    );
    let result = client.generate_chunk_embeddings(&test_chunks);

    assert!(
        result.is_ok(),
        "Chunk embedding generation should succeed: {:?}",
        result
    );

    let embedding_results = result.expect("embedding result succeeded");
    assert_eq!(
        embedding_results.len(),
        test_chunks.len(),
        "Should have one embedding per chunk"
    );

    for (i, embedding_result) in embedding_results.iter().enumerate() {
        let original_chunk = &test_chunks[i];

        assert_eq!(embedding_result.text, original_chunk.content);
        assert_eq!(
            embedding_result.chunk_index,
            Some(original_chunk.chunk_index)
        );
        assert_eq!(
            embedding_result.heading_path,
            Some(original_chunk.heading_path.clone())
        );
        assert_eq!(embedding_result.token_count, original_chunk.token_count);

        assert!(
            !embedding_result.embedding.is_empty(),
            "Embedding {} should not be empty",
            i
        );

        debug!(
            "Chunk {}: '{}' -> {} dimensions",
            i,
            original_chunk.heading_path,
            embedding_result.embedding.len()
        );
    }

    info!(
        "Successfully generated embeddings for all {} content chunks",
        test_chunks.len()
    );
}

#[test]

fn real_ollama_large_batch() {
    init_test_tracing();

    let client = create_integration_test_client();

    // Create a larger batch to test batch processing limits
    let test_texts: Vec<String> = (0..15)
        .map(|i| {
            format!(
                "This is test document number {}. It contains information about various topics including technology, science, and education.",
                i + 1
            )
        })
        .collect();

    info!(
        "Generating embeddings for large batch of {} texts",
        test_texts.len()
    );
    let result = client.generate_embeddings_batch(&test_texts);

    assert!(
        result.is_ok(),
        "Large batch embedding generation should succeed: {:?}",
        result
    );

    let embedding_results = result.expect("embedding result succeeded");
    assert_eq!(
        embedding_results.len(),
        test_texts.len(),
        "Should have one embedding per input"
    );

    // Verify embeddings are unique (similar texts should have different embeddings due to numbering)
    for i in 0..embedding_results.len() {
        for j in (i + 1)..embedding_results.len() {
            let embedding_1 = &embedding_results[i].embedding;
            let embedding_2 = &embedding_results[j].embedding;

            // Calculate cosine similarity
            let dot_product: f32 = embedding_1
                .iter()
                .zip(embedding_2.iter())
                .map(|(a, b)| a * b)
                .sum();
            let norm_1: f32 = embedding_1.iter().map(|x| x * x).sum::<f32>().sqrt();
            let norm_2: f32 = embedding_2.iter().map(|x| x * x).sum::<f32>().sqrt();
            let similarity = dot_product / (norm_1 * norm_2);

            // Embeddings should be similar but not identical
            assert!(
                similarity < 0.99,
                "Embeddings {} and {} are too similar (similarity: {})",
                i,
                j,
                similarity
            );
            assert!(
                similarity > 0.7,
                "Embeddings {} and {} are too different (similarity: {})",
                i,
                j,
                similarity
            );
        }
    }

    info!("Large batch processing completed successfully");
}

#[test]

fn real_ollama_empty_input() {
    init_test_tracing();

    let client = create_integration_test_client();

    // Test empty batch
    let result = client.generate_embeddings_batch(&[]);
    assert!(result.is_ok(), "Empty batch should be handled gracefully");
    assert!(
        result.expect("embedding result succeeded").is_empty(),
        "Empty batch should return empty results"
    );

    // Test empty chunks
    let result = client.generate_chunk_embeddings(&[]);
    assert!(result.is_ok(), "Empty chunks should be handled gracefully");
    assert!(
        result.expect("embedding result succeeded").is_empty(),
        "Empty chunks should return empty results"
    );

    info!("Empty input handling works correctly");
}

#[test]

fn real_ollama_error_recovery() {
    init_test_tracing();

    // Create client with invalid model to test error handling
    let config = Config {
        ollama: OllamaConfig {
            host: "localhost".to_string(),
            port: 11434,
            model: "non-existent-model-12345".to_string(),
            batch_size: 5,
        },
        base_dir: None,
        browser: BrowserConfig::default(),
    };

    let client = OllamaClient::new(&config)
        .expect("Failed to create client")
        .with_timeout(Duration::from_secs(10))
        .with_retry_attempts(1); // Don't retry too much for this test

    info!("Testing error recovery with invalid model");

    // Health check should fail due to invalid model
    let result = client.health_check();
    assert!(
        result.is_err(),
        "Health check should fail with invalid model"
    );

    // Embedding generation should also fail
    let result = client.generate_embedding("test text");
    assert!(
        result.is_err(),
        "Embedding generation should fail with invalid model"
    );

    info!("Error recovery testing completed");
}
