#[cfg(test)]
mod tests;

use anyhow::Result;
use tracing::debug;

use crate::crawler::extractor::{ContentSection, ExtractedContent};

/// Represents a chunk of content ready for embedding
#[derive(Debug, Clone, PartialEq)]
pub struct ContentChunk {
    /// The content text
    pub content: String,
    /// The heading path for this chunk
    pub heading_path: String,
    /// The index of this chunk within the page
    pub chunk_index: usize,
    /// Estimated token count
    pub token_count: usize,
    /// Whether this chunk contains code blocks
    pub has_code_blocks: bool,
}

/// Configuration for content chunking
#[derive(Debug, Clone)]
pub struct ChunkingConfig {
    /// Target chunk size in tokens
    pub target_chunk_size: usize,
    /// Maximum chunk size in tokens before forced splitting
    pub max_chunk_size: usize,
    /// Minimum chunk size in tokens (smaller chunks will be merged)
    pub min_chunk_size: usize,
    /// Overlap size in tokens between adjacent chunks
    pub overlap_size: usize,
    /// Whether to preserve code blocks as single units
    pub preserve_code_blocks: bool,
    /// Whether to break at sentence boundaries when possible
    pub sentence_boundary_splitting: bool,
}

impl Default for ChunkingConfig {
    #[inline]
    fn default() -> Self {
        Self {
            target_chunk_size: 650,
            max_chunk_size: 1000,
            min_chunk_size: 100,
            overlap_size: 50,
            preserve_code_blocks: true,
            sentence_boundary_splitting: true,
        }
    }
}

/// Chunk extracted content into embedding-ready pieces
#[inline]
pub fn chunk_content(
    content: &ExtractedContent,
    config: &ChunkingConfig,
) -> Result<Vec<ContentChunk>> {
    let mut chunks = Vec::new();
    let mut chunk_index = 0;

    // Process each section individually
    for section in &content.sections {
        let section_chunks = chunk_section(section, config, &mut chunk_index)?;
        chunks.extend(section_chunks);
    }

    // If no sections were processed, chunk the raw text as fallback
    if chunks.is_empty() && !content.raw_text.trim().is_empty() {
        let fallback_section = ContentSection {
            heading_path: content.title.clone(),
            content: content.raw_text.clone(),
            heading_level: None,
            has_code_blocks: false,
        };
        chunks = chunk_section(&fallback_section, config, &mut chunk_index)?;
    }

    // Post-process chunks: merge small chunks and add overlap
    let processed_chunks = post_process_chunks(chunks, config)?;

    debug!(
        "Chunked content '{}' into {} chunks (avg {} tokens)",
        content.title,
        processed_chunks.len(),
        processed_chunks
            .iter()
            .map(|c| c.token_count)
            .sum::<usize>()
            / processed_chunks.len().max(1)
    );

    Ok(processed_chunks)
}

/// Chunk a single content section
fn chunk_section(
    section: &ContentSection,
    config: &ChunkingConfig,
    chunk_index: &mut usize,
) -> Result<Vec<ContentChunk>> {
    let mut chunks = Vec::new();
    let content = &section.content;

    if content.trim().is_empty() {
        return Ok(chunks);
    }

    let token_count = estimate_token_count(content);

    // If content is small enough, return as single chunk
    if token_count <= config.target_chunk_size {
        chunks.push(ContentChunk {
            content: content.clone(),
            heading_path: section.heading_path.clone(),
            chunk_index: *chunk_index,
            token_count,
            has_code_blocks: section.has_code_blocks,
        });
        *chunk_index += 1;
        return Ok(chunks);
    }

    // Split content using semantic chunking strategy
    let splits = if section.has_code_blocks && config.preserve_code_blocks {
        split_with_code_preservation(content, config)?
    } else {
        split_by_semantics(content, config)?
    };

    // Create chunks from splits
    for split in splits {
        if split.trim().is_empty() {
            continue;
        }

        let chunk_token_count = estimate_token_count(&split);
        let has_code_blocks = section.has_code_blocks && contains_code_block(&split);
        chunks.push(ContentChunk {
            content: split,
            heading_path: section.heading_path.clone(),
            chunk_index: *chunk_index,
            token_count: chunk_token_count,
            has_code_blocks,
        });
        *chunk_index += 1;
    }

    Ok(chunks)
}

/// Split content while preserving code blocks
fn split_with_code_preservation(content: &str, config: &ChunkingConfig) -> Result<Vec<String>> {
    let mut splits = Vec::new();
    let mut current_split = String::new();
    let mut in_code_block = false;
    let mut current_token_count = 0;

    for line in content.lines() {
        let line_with_newline = format!("{}\n", line);
        let line_tokens = estimate_token_count(&line_with_newline);

        // Detect code block boundaries
        if line.trim().starts_with("```") {
            in_code_block = !in_code_block;
        }

        // If adding this line would exceed max size and we're not in a code block, split
        if !in_code_block
            && current_token_count + line_tokens > config.max_chunk_size
            && !current_split.trim().is_empty()
        {
            splits.push(current_split.trim().to_string());
            current_split.clear();
            current_token_count = 0;
        }

        current_split.push_str(&line_with_newline);
        current_token_count += line_tokens;
    }

    // Add the final split if it has content
    if !current_split.trim().is_empty() {
        splits.push(current_split.trim().to_string());
    }

    Ok(splits)
}

/// Split content using semantic boundaries
fn split_by_semantics(content: &str, config: &ChunkingConfig) -> Result<Vec<String>> {
    let mut splits = Vec::new();
    let mut current_split = String::new();
    let mut current_token_count = 0;

    // Split by paragraphs first
    let paragraphs = content.split("\n\n").collect::<Vec<_>>();

    for paragraph in paragraphs {
        if paragraph.trim().is_empty() {
            continue;
        }

        let paragraph_tokens = estimate_token_count(paragraph);

        // If this paragraph alone exceeds max size, split it further
        if paragraph_tokens > config.max_chunk_size {
            // Split by sentences if enabled
            if config.sentence_boundary_splitting {
                let sentence_splits = split_by_sentences(paragraph, config)?;
                for sentence_split in sentence_splits {
                    if current_token_count + estimate_token_count(&sentence_split)
                        > config.target_chunk_size
                        && !current_split.trim().is_empty()
                    {
                        splits.push(current_split.trim().to_string());
                        current_split.clear();
                        current_token_count = 0;
                    }
                    current_split.push_str(&sentence_split);
                    current_split.push_str("\n\n");
                    current_token_count += estimate_token_count(&sentence_split);
                }
            } else {
                // Fallback to word-based splitting
                let word_splits = split_by_words(paragraph, config)?;
                for word_split in word_splits {
                    if current_token_count + estimate_token_count(&word_split)
                        > config.target_chunk_size
                        && !current_split.trim().is_empty()
                    {
                        splits.push(current_split.trim().to_string());
                        current_split.clear();
                        current_token_count = 0;
                    }
                    current_split.push_str(&word_split);
                    current_split.push_str("\n\n");
                    current_token_count += estimate_token_count(&word_split);
                }
            }
        } else {
            // Check if adding this paragraph would exceed target size
            if current_token_count + paragraph_tokens > config.target_chunk_size
                && !current_split.trim().is_empty()
            {
                splits.push(current_split.trim().to_string());
                current_split.clear();
                current_token_count = 0;
            }

            current_split.push_str(paragraph);
            current_split.push_str("\n\n");
            current_token_count += paragraph_tokens;
        }
    }

    // Add the final split if it has content
    if !current_split.trim().is_empty() {
        splits.push(current_split.trim().to_string());
    }

    Ok(splits)
}

/// Split text by sentences
fn split_by_sentences(text: &str, config: &ChunkingConfig) -> Result<Vec<String>> {
    let mut splits = Vec::new();
    let mut current_split = String::new();
    let mut current_token_count = 0;

    // Simple sentence boundary detection
    let sentences = text
        .split(['.', '!', '?'])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();

    for (i, sentence) in sentences.iter().enumerate() {
        let sentence_with_punct = if i < sentences.len() - 1 {
            format!("{}. ", sentence)
        } else {
            (*sentence).to_string()
        };

        let sentence_tokens = estimate_token_count(&sentence_with_punct);

        if current_token_count + sentence_tokens > config.target_chunk_size
            && !current_split.trim().is_empty()
        {
            splits.push(current_split.trim().to_string());
            current_split.clear();
            current_token_count = 0;
        }

        current_split.push_str(&sentence_with_punct);
        current_token_count += sentence_tokens;
    }

    if !current_split.trim().is_empty() {
        splits.push(current_split.trim().to_string());
    }

    Ok(splits)
}

/// Split text by words as a last resort
fn split_by_words(text: &str, config: &ChunkingConfig) -> Result<Vec<String>> {
    let mut splits = Vec::new();
    let mut current_split = String::new();
    let mut current_token_count = 0;

    let words = text.split_whitespace().collect::<Vec<_>>();

    for word in words {
        let word_with_space = format!("{} ", word);
        let word_tokens = estimate_token_count(&word_with_space);

        if current_token_count + word_tokens > config.target_chunk_size
            && !current_split.trim().is_empty()
        {
            splits.push(current_split.trim().to_string());
            current_split.clear();
            current_token_count = 0;
        }

        current_split.push_str(&word_with_space);
        current_token_count += word_tokens;
    }

    if !current_split.trim().is_empty() {
        splits.push(current_split.trim().to_string());
    }

    Ok(splits)
}

/// Post-process chunks to merge small ones and add overlap
fn post_process_chunks(
    chunks: Vec<ContentChunk>,
    config: &ChunkingConfig,
) -> Result<Vec<ContentChunk>> {
    if chunks.is_empty() {
        return Ok(chunks);
    }

    let mut processed = Vec::new();
    let mut pending_merge: Option<ContentChunk> = None;

    for chunk in chunks {
        // If we have a pending merge and this chunk is small, try to merge
        if let Some(mut pending) = pending_merge.take() {
            if chunk.token_count < config.min_chunk_size
                && pending.token_count + chunk.token_count <= config.max_chunk_size
                && pending.heading_path == chunk.heading_path
            {
                // Merge chunks
                pending.content.push_str("\n\n");
                pending.content.push_str(&chunk.content);
                pending.token_count += chunk.token_count;
                pending.has_code_blocks = pending.has_code_blocks || chunk.has_code_blocks;
                pending_merge = Some(pending);
                continue;
            } else {
                // Can't merge, add pending to processed
                processed.push(pending);
            }
        }

        // If current chunk is too small, mark for potential merging
        if chunk.token_count < config.min_chunk_size {
            pending_merge = Some(chunk);
        } else {
            processed.push(chunk);
        }
    }

    // Add any remaining pending merge
    if let Some(pending) = pending_merge {
        processed.push(pending);
    }

    // Add overlap between adjacent chunks if configured
    if config.overlap_size > 0 {
        processed = add_overlap(processed, config)?;
    }

    // Re-index chunks
    for (i, chunk) in processed.iter_mut().enumerate() {
        chunk.chunk_index = i;
    }

    Ok(processed)
}

/// Add overlap between adjacent chunks
fn add_overlap(
    mut chunks: Vec<ContentChunk>,
    config: &ChunkingConfig,
) -> Result<Vec<ContentChunk>> {
    let mut i = 1;
    while i < chunks.len() {
        let (left, right) = chunks.split_at_mut(i);
        let prev_chunk = &left[i - 1];
        let curr_chunk = &mut right[0];

        // Only add overlap if chunks are from the same section
        if prev_chunk.heading_path == curr_chunk.heading_path {
            let overlap_text = extract_overlap_text(&prev_chunk.content, config.overlap_size);
            if !overlap_text.is_empty() {
                curr_chunk.content = format!("{}\n\n{}", overlap_text, curr_chunk.content);
                curr_chunk.token_count += estimate_token_count(&overlap_text);
            }
        }
        i += 1;
    }

    Ok(chunks)
}

/// Extract overlap text from the end of a chunk
fn extract_overlap_text(content: &str, overlap_tokens: usize) -> String {
    let words: Vec<&str> = content.split_whitespace().collect();
    let word_count = (overlap_tokens as f64 * 0.75) as usize; // Rough word-to-token ratio

    if words.len() <= word_count {
        return String::new();
    }

    words[words.len() - word_count.min(words.len())..].join(" ")
}

/// Estimate token count using a simple heuristic
/// This is a rough approximation - actual tokenization would be more accurate
#[inline]
pub fn estimate_token_count(text: &str) -> usize {
    // Rough heuristic: 1 token ≈ 0.75 words for English text
    // Add extra tokens for punctuation and special characters
    let word_count = text.split_whitespace().count();
    let punct_count = text.chars().filter(|c| c.is_ascii_punctuation()).count();

    (punct_count as f64).mul_add(0.1, word_count as f64 / 0.75) as usize
}

/// Check if text contains code blocks
fn contains_code_block(text: &str) -> bool {
    text.contains("```") || text.lines().any(|line| line.starts_with("    "))
}

/// Create a chunk with proper context for a page
#[inline]
pub fn create_contextual_chunk(
    content: &str,
    page_title: &str,
    heading_path: &str,
    chunk_index: usize,
) -> ContentChunk {
    let _token_count = estimate_token_count(content);
    let has_code_blocks = contains_code_block(content);

    // Create contextual content with page title and heading path
    let contextual_content = if heading_path != page_title {
        format!(
            "Page: {}\nSection: {}\n\n{}",
            page_title, heading_path, content
        )
    } else {
        format!("Page: {}\n\n{}", page_title, content)
    };

    let token_count = estimate_token_count(&contextual_content);
    ContentChunk {
        content: contextual_content,
        heading_path: heading_path.to_string(),
        chunk_index,
        token_count,
        has_code_blocks,
    }
}
