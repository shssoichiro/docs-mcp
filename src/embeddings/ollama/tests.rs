use super::*;
use crate::config::OllamaConfig;

#[test]
fn client_configuration() {
    let config = OllamaConfig {
        protocol: "http".to_string(),
        host: "test-host".to_string(),
        port: 1234,
        model: "test-model".to_string(),
        batch_size: 128,
    };
    let client = OllamaClient::new(config).expect("Failed to create client");

    assert_eq!(client.model, "test-model");
    assert_eq!(client.batch_size, 128);
    assert_eq!(client.base_url.host_str(), Some("test-host"));
    assert_eq!(client.base_url.port(), Some(1234));
    // Note: timeout is now part of the agent configuration
    assert_eq!(client.retry_attempts, DEFAULT_RETRY_ATTEMPTS);
}

#[test]
fn client_builder_methods() {
    let config = OllamaConfig::default();
    let client = OllamaClient::new(config)
        .expect("Failed to create client")
        .with_timeout(Duration::from_secs(60))
        .with_retry_attempts(5);

    // Note: timeout is now part of the agent configuration
    assert_eq!(client.retry_attempts, 5);
}

#[test]
fn embedding_result_structure() {
    let result = EmbeddingResult {
        text: "test text".to_string(),
        embedding: vec![0.1, 0.2, 0.3, 0.4, 0.5],
        token_count: 10,
        chunk_index: Some(0),
        heading_path: Some("Test Section".to_string()),
    };

    assert_eq!(result.text, "test text");
    assert_eq!(result.embedding.len(), 5);
    assert_eq!(result.token_count, 10);
    assert_eq!(result.chunk_index, Some(0));
    assert_eq!(result.heading_path, Some("Test Section".to_string()));
}
