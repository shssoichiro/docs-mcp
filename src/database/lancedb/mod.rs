// LanceDB vector database module
// Handles vector storage and similarity search for embeddings

#[cfg(test)]
mod tests;

pub mod vector_store;

use serde::{Deserialize, Serialize};

/// Embedding record stored in LanceDB
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingRecord {
    /// Unique identifier for this embedding
    pub id: String,
    /// The vector embedding (768 dimensions for nomic-embed-text)
    pub vector: Vec<f32>,
    /// Metadata about the chunk this embedding represents
    pub metadata: ChunkMetadata,
}

/// Metadata for a chunk stored alongside its embedding
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    /// ID of the chunk in SQLite database
    pub chunk_id: String,
    /// ID of the site this chunk belongs to
    pub site_id: String,
    /// Title of the page/document
    pub page_title: String,
    /// URL of the source page
    pub page_url: String,
    /// Heading path (e.g., "Getting Started > Installation > Requirements")
    pub heading_path: Option<String>,
    /// The actual text content of the chunk
    pub content: String,
    /// Token count of the chunk
    pub token_count: u32,
    /// Index of this chunk within the page (for ordering)
    pub chunk_index: u32,
    /// Timestamp when this embedding was created
    pub created_at: String,
}
