// Embeddings module
// This module will handle Ollama integration and content chunking

pub mod chunking;
pub mod ollama;

pub use chunking::{
    ChunkingConfig, ContentChunk, chunk_content, create_contextual_chunk, estimate_token_count,
};
pub use ollama::{EmbeddingResult, OllamaClient};
