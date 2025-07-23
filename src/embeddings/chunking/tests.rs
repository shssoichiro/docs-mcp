use super::estimate_token_count as estimate_token_count_impl;
use super::split_with_code_preservation as split_with_code_preservation_impl;
use super::*;
use crate::crawler::extractor::ContentSection;

fn create_test_content() -> ExtractedContent {
    ExtractedContent {
            title: "Test Documentation".to_string(),
            sections: vec![
                ContentSection {
                    heading_path: "Introduction".to_string(),
                    content: "This is the introduction section with some basic information about the topic.".to_string(),
                    heading_level: Some(1),
                    has_code_blocks: false,
                },
                ContentSection {
                    heading_path: "Introduction > Getting Started".to_string(),
                    content: "Here's how to get started:\n\n```bash\nnpm install example\n```\n\nThen run the application.".to_string(),
                    heading_level: Some(2),
                    has_code_blocks: true,
                },
                ContentSection {
                    heading_path: "Advanced Usage".to_string(),
                    content: "Advanced usage involves understanding complex concepts. ".repeat(100),
                    heading_level: Some(1),
                    has_code_blocks: false,
                },
            ],
            raw_text: "Full text content...".to_string(),
        }
}

#[test]
fn estimate_token_count() {
    assert_eq!(estimate_token_count_impl("hello world"), 2);
    assert_eq!(estimate_token_count_impl("This is a test."), 5);
    assert_eq!(estimate_token_count_impl(""), 0);
}

#[test]
fn chunk_small_content() {
    let content = create_test_content();
    let config = ChunkingConfig::default();

    let chunks = chunk_content(&content, &config).expect("chunk_content should succeed");

    assert!(!chunks.is_empty());

    // Small sections should remain as single chunks

    assert_eq!(
        chunks
            .iter()
            .filter(|c| c.heading_path == "Introduction")
            .count(),
        1
    );
}

#[test]
fn chunk_large_content() {
    let content = create_test_content();
    let config = ChunkingConfig {
        target_chunk_size: 50,
        max_chunk_size: 100,
        ..ChunkingConfig::default()
    };

    let chunks = chunk_content(&content, &config).expect("chunk_content should succeed");

    // Large section should be split into multiple chunks

    assert!(
        chunks
            .iter()
            .filter(|c| c.heading_path == "Advanced Usage")
            .count()
            > 1
    );
}

#[test]
fn preserve_code_blocks() {
    let content = create_test_content();
    let config = ChunkingConfig::default();

    let chunks = chunk_content(&content, &config).expect("chunk_content should succeed");

    // Find chunk with code blocks
    let code_chunk = chunks
        .iter()
        .find(|c| c.has_code_blocks)
        .expect("Should find chunk with code blocks");

    assert!(code_chunk.content.contains("```"));
    assert!(code_chunk.has_code_blocks);
}

#[test]
fn heading_path_preservation() {
    let content = create_test_content();
    let config = ChunkingConfig::default();

    let chunks = chunk_content(&content, &config).expect("chunk_content should succeed");

    // All chunks should have meaningful heading paths
    for chunk in &chunks {
        assert!(!chunk.heading_path.is_empty());
        assert_ne!(chunk.heading_path, "Content");
    }
}

#[test]
fn contextual_chunk() {
    let content = "This is some content";
    let page_title = "Documentation";
    let heading_path = "Section > Subsection";

    let chunk = create_contextual_chunk(content, page_title, heading_path, 0);

    assert!(chunk.content.contains(page_title));
    assert!(chunk.content.contains("Section > Subsection"));
    assert!(chunk.content.contains(content));
    assert_eq!(chunk.chunk_index, 0);
}

#[test]
fn split_with_code_preservation() {
    let content =
        "Some text\n\n```rust\nfn main() {\n    println!(\"Hello\");\n}\n```\n\nMore text";
    let config = ChunkingConfig::default();

    let splits = split_with_code_preservation_impl(content, &config)
        .expect("split_with_code_preservation should succeed");

    // Should preserve code block as a unit
    let code_split = splits
        .iter()
        .find(|s| s.contains("```"))
        .expect("code block should exist");
    assert!(code_split.contains("fn main()"));
    assert!(code_split.contains("println!"));
}

#[test]
fn empty_content() {
    let content = ExtractedContent {
        title: "Empty".to_string(),
        sections: vec![],
        raw_text: String::new(),
    };
    let config = ChunkingConfig::default();

    let chunks = chunk_content(&content, &config).expect("chunk_content should succeed");
    assert!(chunks.is_empty());
}
