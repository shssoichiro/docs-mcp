use anyhow::{Result, anyhow};
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::debug;

/// Represents a content section with its heading hierarchy
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ContentSection {
    /// The heading path (e.g., "Getting Started > Installation > Prerequisites")
    pub heading_path: String,
    /// The text content of this section
    pub content: String,
    /// The heading level (1-6 for h1-h6)
    pub heading_level: Option<u8>,
    /// Whether this section contains code blocks
    pub has_code_blocks: bool,
}

/// Represents extracted page content with metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtractedContent {
    /// The page title
    pub title: String,
    /// List of content sections in document order
    pub sections: Vec<ContentSection>,
    /// The raw text content for fallback
    pub raw_text: String,
}

/// Configuration for content extraction
#[derive(Debug, Clone)]
pub struct ExtractionConfig {
    /// Whether to preserve code blocks during extraction
    pub preserve_code_blocks: bool,
    /// Whether to include navigation elements
    pub include_navigation: bool,
    /// Whether to include footer content
    pub include_footer: bool,
    /// Maximum heading level to consider (1-6)
    pub max_heading_level: u8,
}

impl Default for ExtractionConfig {
    #[inline]
    fn default() -> Self {
        Self {
            preserve_code_blocks: true,
            include_navigation: false,
            include_footer: false,
            max_heading_level: 6,
        }
    }
}

/// Extract structured content from HTML document
#[inline]
pub fn extract_content(html: &str, config: &ExtractionConfig) -> Result<ExtractedContent> {
    let document = Html::parse_document(html);

    // Extract page title
    let title = extract_title(&document)?;

    // Extract main content sections
    let sections = extract_sections(&document, config)?;

    // Extract raw text as fallback
    let raw_text = extract_raw_text(&document, config)?;

    debug!(
        "Extracted content: title='{}', {} sections, {} chars raw text",
        title,
        sections.len(),
        raw_text.len()
    );

    Ok(ExtractedContent {
        title,
        sections,
        raw_text,
    })
}

/// Extract the page title from HTML document
fn extract_title(document: &Html) -> Result<String> {
    // Try multiple selectors for title extraction
    let title_selectors = [
        "title",
        "h1",
        "[data-title]",
        ".page-title",
        ".title",
        "#title",
    ];

    for selector_str in &title_selectors {
        if let Ok(selector) = Selector::parse(selector_str) {
            if let Some(element) = document.select(&selector).next() {
                let title = extract_clean_text(element);
                if !title.is_empty() {
                    debug!(
                        "Extracted title using selector '{}': '{}'",
                        selector_str, title
                    );
                    return Ok(title);
                }
            }
        }
    }

    // Fallback to a generic title
    Ok("Untitled".to_string())
}

/// Extract content sections organized by heading hierarchy
fn extract_sections(document: &Html, config: &ExtractionConfig) -> Result<Vec<ContentSection>> {
    let mut sections = Vec::new();
    let mut heading_stack: Vec<(u8, String)> = Vec::new();

    // Find the main content area
    let content_root = find_main_content(document);

    // Process all elements in document order
    for element in content_root.descendants() {
        if let Some(element_ref) = ElementRef::wrap(element) {
            let tag_name = element_ref.value().name();

            // Handle headings
            if tag_name.starts_with('h') && tag_name.len() == 2 {
                if let Some(level_char) = tag_name.chars().nth(1) {
                    if let Some(level) = level_char.to_digit(10) {
                        let level = level as u8;
                        if level <= config.max_heading_level {
                            let heading_text = extract_clean_text(element_ref);
                            if !heading_text.is_empty() {
                                update_heading_stack(&mut heading_stack, level, heading_text);
                            }
                        }
                    }
                }
            }

            // Handle content elements
            if is_content_element(tag_name) {
                let content = extract_element_content(element_ref, config)?;
                if !content.trim().is_empty() {
                    let heading_path = build_heading_path(&heading_stack);
                    let has_code_blocks = contains_code_blocks(element_ref);

                    sections.push(ContentSection {
                        heading_path,
                        content,
                        heading_level: heading_stack.last().map(|(level, _)| *level),
                        has_code_blocks,
                    });
                }
            }
        }
    }

    // If no sections found, create a single section with all content
    if sections.is_empty() {
        let content = extract_raw_text(document, config)?;
        if !content.trim().is_empty() {
            sections.push(ContentSection {
                heading_path: "Main Content".to_string(),
                content,
                heading_level: None,
                has_code_blocks: false,
            });
        }
    }

    Ok(sections)
}

/// Find the main content area of the document
fn find_main_content(document: &Html) -> ElementRef<'_> {
    // Try common main content selectors
    let main_selectors = [
        "main",
        "[role=\"main\"]",
        ".content",
        ".main-content",
        "#content",
        "#main",
        ".documentation",
        ".docs",
        "article",
        ".article-content",
    ];

    for selector_str in &main_selectors {
        if let Ok(selector) = Selector::parse(selector_str) {
            if let Some(element) = document.select(&selector).next() {
                debug!("Found main content using selector: {}", selector_str);
                return element;
            }
        }
    }

    // Fallback to document root
    document.root_element()
}

/// Update the heading stack with a new heading
fn update_heading_stack(stack: &mut Vec<(u8, String)>, level: u8, text: String) {
    // Remove headings at the same or deeper level
    stack.retain(|(l, _)| *l < level);

    // Add the new heading
    stack.push((level, text));
}

/// Build a heading path from the heading stack
fn build_heading_path(stack: &[(u8, String)]) -> String {
    if stack.is_empty() {
        "Content".to_string()
    } else {
        stack
            .iter()
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>()
            .join(" > ")
    }
}

/// Check if an element contains code blocks
fn contains_code_blocks(element: ElementRef) -> bool {
    let code_selector = Selector::parse("pre, code, .highlight, .code-block, .language-")
        .expect("Valid CSS selector");

    element.select(&code_selector).next().is_some()
}

/// Check if a tag represents a content element
fn is_content_element(tag_name: &str) -> bool {
    matches!(
        tag_name,
        "p" | "div" | "section" | "article" | "blockquote" | "li" | "dd" | "dt" | "pre"
    )
}

/// Extract content from a specific element
fn extract_element_content(element: ElementRef, config: &ExtractionConfig) -> Result<String> {
    let tag_name = element.value().name();

    // Handle code blocks specially if preservation is enabled
    if config.preserve_code_blocks && (tag_name == "pre" || tag_name == "code") {
        return Ok(format!(
            "```\n{}\n```",
            element.text().collect::<String>().trim()
        ));
    }

    // Extract text content with some structure preservation
    let mut content = String::new();
    extract_text_recursive(element, &mut content, config);

    Ok(clean_text(&content))
}

/// Recursively extract text content from an element
fn extract_text_recursive(element: ElementRef, content: &mut String, config: &ExtractionConfig) {
    for child in element.children() {
        if let Some(child_element) = ElementRef::wrap(child) {
            let tag_name = child_element.value().name();

            match tag_name {
                // Skip certain elements
                "script" | "style" | "noscript" => {}
                "nav" if !config.include_navigation => {}
                "footer" if !config.include_footer => {}

                // Handle code blocks
                "pre" | "code" if config.preserve_code_blocks => {
                    content.push_str("```\n");
                    content.push_str(child_element.text().collect::<String>().trim());
                    content.push_str("\n```\n");
                }

                // Handle lists
                "li" => {
                    content.push_str("• ");
                    extract_text_recursive(child_element, content, config);
                    content.push('\n');
                }

                // Handle line breaks
                "br" => content.push('\n'),

                // Handle paragraphs and block elements
                "p" | "div" | "section" | "article" | "blockquote" => {
                    extract_text_recursive(child_element, content, config);
                    content.push_str("\n\n");
                }

                // Handle other elements recursively
                _ => extract_text_recursive(child_element, content, config),
            }
        } else if let Some(text_node) = child.value().as_text() {
            content.push_str(text_node);
        }
    }
}

/// Extract raw text content from the entire document
fn extract_raw_text(document: &Html, config: &ExtractionConfig) -> Result<String> {
    let main_content = find_main_content(document);
    let mut content = String::new();
    extract_text_recursive(main_content, &mut content, config);
    Ok(clean_text(&content))
}

/// Extract text content from an element while excluding certain unwanted elements
fn extract_clean_text(element: ElementRef) -> String {
    let mut text_parts = Vec::new();
    extract_text_excluding_elements(
        element,
        &mut text_parts,
        &["button", "script", "style", "noscript"],
    );
    clean_text(&text_parts.join(" "))
}

/// Recursively extract text content while excluding specific element types
fn extract_text_excluding_elements(
    element: ElementRef,
    text_parts: &mut Vec<String>,
    excluded_tags: &[&str],
) {
    for child in element.children() {
        if let Some(child_element) = ElementRef::wrap(child) {
            let tag_name = child_element.value().name();

            // Skip excluded elements entirely
            if excluded_tags.contains(&tag_name) {
                continue;
            }

            // Recursively process child elements
            extract_text_excluding_elements(child_element, text_parts, excluded_tags);
        } else if let Some(text_node) = child.value().as_text() {
            let text = text_node.trim();
            if !text.is_empty() {
                text_parts.push(text.to_string());
            }
        }
    }
}

/// Clean and normalize text content
fn clean_text(text: &str) -> String {
    text
        // Normalize whitespace
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        // Remove excessive newlines
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
        // Normalize Unicode
        .chars()
        .collect::<String>()
        .trim()
        .to_string()
}

/// Extract metadata from HTML document
#[inline]
pub fn extract_metadata(html: &str) -> Result<HashMap<String, String>> {
    let document = Html::parse_document(html);
    let mut metadata = HashMap::new();

    // Extract meta tags
    let meta_selector =
        Selector::parse("meta").map_err(|e| anyhow!("Failed to create meta selector: {:?}", e))?;

    for element in document.select(&meta_selector) {
        if let (Some(name), Some(content)) = (
            element
                .value()
                .attr("name")
                .or_else(|| element.value().attr("property")),
            element.value().attr("content"),
        ) {
            metadata.insert(name.to_string(), content.to_string());
        }
    }

    // Extract title
    if let Ok(title) = extract_title(&document) {
        metadata.insert("title".to_string(), title);
    }

    Ok(metadata)
}

#[cfg(test)]
mod tests;
