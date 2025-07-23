use super::*;

#[test]
fn embedding_record_structure() {
    let metadata = ChunkMetadata {
        chunk_id: "test_chunk_123".to_string(),
        site_id: "test_site_456".to_string(),
        page_title: "Test Page".to_string(),
        page_url: "https://example.com/test".to_string(),
        heading_path: Some("Section > Subsection".to_string()),
        content: "This is test content for the chunk".to_string(),
        token_count: 25,
        chunk_index: 0,
        created_at: "2024-01-01T00:00:00Z".to_string(),
    };

    let record = EmbeddingRecord {
        id: "embedding_123".to_string(),
        vector: vec![0.1, 0.2, 0.3],
        metadata,
    };

    assert_eq!(record.id, "embedding_123");
    assert_eq!(record.vector.len(), 3);
    assert_eq!(record.metadata.chunk_id, "test_chunk_123");
    assert_eq!(record.metadata.token_count, 25);
}

#[test]
fn chunk_metadata_serialization() {
    let metadata = ChunkMetadata {
        chunk_id: "test_chunk".to_string(),
        site_id: "test_site".to_string(),
        page_title: "Test".to_string(),
        page_url: "https://example.com".to_string(),
        heading_path: None,
        content: "Test content".to_string(),
        token_count: 10,
        chunk_index: 5,
        created_at: "2024-01-01T00:00:00Z".to_string(),
    };

    // Test that it can be serialized and deserialized
    let json = serde_json::to_string(&metadata).expect("can serialize json");
    let deserialized: ChunkMetadata = serde_json::from_str(&json).expect("can parse json");

    assert_eq!(metadata.chunk_id, deserialized.chunk_id);
    assert_eq!(metadata.heading_path, deserialized.heading_path);
}
