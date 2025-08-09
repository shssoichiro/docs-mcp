#[cfg(test)]
mod tests;

use crate::turndown::{
    CodeBlockStyle, Filter, HeadingStyle, Rule, TurndownOptions, TurndownService,
};
use anyhow::Result;
use pulldown_cmark::HeadingLevel;
use pulldown_cmark::{Event, Parser, Tag, TagEnd};
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::{borrow::Cow, rc::Rc};
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

/// Extract structured content from HTML document
pub fn extract_content(html: &str) -> Result<ExtractedContent> {
    let opts = TurndownOptions {
        heading_style: HeadingStyle::Atx,
        code_block_style: CodeBlockStyle::Fenced,
        bullet_list_marker: "-",
        ..TurndownOptions::default()
    };
    let mut turndown = TurndownService::new(Some(opts));
    turndown.add_rule(
        "remove_scripts",
        Rule::new(
            Filter::TagNames(vec![
                "script",
                "iframe",
                "style",
                "nav",
                "navbar",
                "header",
                "footer",
                "aside",
                "button",
                "rustdoc-search",
                "rostdoc-toolbar",
            ]),
            Rc::new(|_, _, _| Cow::Borrowed("")),
        ),
    );
    turndown.add_rule(
        "clean_code_blocks",
        Rule::new(
            Filter::TagName("pre"),
            Rc::new(|content, _, _| Cow::Owned(format!("\n```\n{}\n```\n", content))),
        ),
    );

    let document = Html::parse_document(html);
    let clean_document = clean_content(document);
    let markdown = turndown.turndown(&clean_document.html())?;

    // Extract page title from first heading
    let title = extract_title_from_markdown(&markdown);

    // Extract main content sections
    let sections = extract_sections(&markdown)?;

    debug!(
        "Extracted content: title='{}', {} sections, {} chars raw text",
        title,
        sections.len(),
        markdown.len()
    );

    Ok(ExtractedContent {
        title,
        sections,
        raw_text: markdown,
    })
}

/// Extract content sections organized by heading hierarchy from markdown
fn extract_sections(markdown: &str) -> Result<Vec<ContentSection>> {
    let mut sections = Vec::new();
    let mut heading_stack: Vec<(u8, String)> = Vec::new();

    let parser = Parser::new(markdown);
    let mut current_content = String::new();
    let mut current_heading_text = String::new();
    let mut in_heading = false;
    let mut in_code_block = false;
    let mut has_code_blocks = false;

    for event in parser {
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { .. } => {
                    // Save any pending content before processing new heading
                    if !current_content.trim().is_empty() {
                        let heading_path = build_heading_path(&heading_stack);
                        sections.push(ContentSection {
                            heading_path,
                            content: current_content.trim().to_string(),
                            heading_level: heading_stack.last().map(|(level, _)| *level),
                            has_code_blocks,
                        });
                        current_content.clear();
                        has_code_blocks = false;
                    }

                    in_heading = true;
                    current_heading_text.clear();
                }
                Tag::CodeBlock(_) => {
                    in_code_block = true;
                    has_code_blocks = true;
                    current_content.push_str("```\n");
                }
                Tag::Paragraph => {
                    // Add some spacing for paragraph separation
                    if !current_content.is_empty() && !current_content.ends_with("\n\n") {
                        current_content.push('\n');
                    }
                }
                Tag::List(_) => {
                    if !current_content.is_empty() && !current_content.ends_with("\n") {
                        current_content.push('\n');
                    }
                }
                Tag::Item => {
                    current_content.push_str("• ");
                }
                _ => {}
            },
            Event::End(tag_end) => match tag_end {
                TagEnd::Heading(level) => {
                    if in_heading {
                        let heading_level = heading_level_to_u8(level);
                        if !current_heading_text.trim().is_empty() {
                            update_heading_stack(
                                &mut heading_stack,
                                heading_level,
                                current_heading_text.trim().to_string(),
                            );
                        }
                        in_heading = false;
                    }
                }
                TagEnd::CodeBlock => {
                    if in_code_block {
                        current_content.push_str("```\n");
                        in_code_block = false;
                    }
                }
                TagEnd::Paragraph => {
                    current_content.push('\n');
                }
                TagEnd::Item => {
                    current_content.push('\n');
                }
                _ => {}
            },
            Event::Text(text) => {
                if in_heading {
                    current_heading_text.push_str(&text);
                } else {
                    current_content.push_str(&text);
                }
            }
            Event::Code(code) => {
                if in_heading {
                    current_heading_text.push_str(&code);
                } else {
                    current_content.push('`');
                    current_content.push_str(&code);
                    current_content.push('`');
                    has_code_blocks = true;
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if in_heading {
                    current_heading_text.push(' ');
                } else {
                    current_content.push('\n');
                }
            }
            Event::Html(html) => {
                // Handle inline HTML if needed
                current_content.push_str(&html);
            }
            _ => {}
        }
    }

    // Add any remaining content as the last section
    if !current_content.trim().is_empty() {
        let heading_path = build_heading_path(&heading_stack);
        sections.push(ContentSection {
            heading_path,
            content: current_content.trim().to_string(),
            heading_level: heading_stack.last().map(|(level, _)| *level),
            has_code_blocks,
        });
    }

    // If no sections found, create a single section with all content
    if sections.is_empty() && !markdown.trim().is_empty() {
        sections.push(ContentSection {
            heading_path: "Main Content".to_string(),
            content: markdown.trim().to_string(),
            heading_level: None,
            has_code_blocks: markdown.contains("```") || markdown.contains('`'),
        });
    }

    Ok(sections)
}

/// Convert pulldown-cmark HeadingLevel to u8
fn heading_level_to_u8(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Extract page title from markdown (uses first heading or falls back to default)
fn extract_title_from_markdown(markdown: &str) -> String {
    let parser = Parser::new(markdown);
    let mut in_heading = false;
    let mut title = String::new();

    for event in parser {
        match event {
            Event::Start(Tag::Heading {
                level: HeadingLevel::H1,
                ..
            }) => {
                in_heading = true;
            }
            Event::End(TagEnd::Heading(HeadingLevel::H1)) => {
                if !title.trim().is_empty() {
                    return title.trim().to_string();
                }
                in_heading = false;
            }
            Event::Text(text) if in_heading => {
                title.push_str(&text);
            }
            _ => {}
        }
    }

    // If no H1 found, try any heading
    let parser = Parser::new(markdown);
    let mut in_any_heading = false;
    let mut any_title = String::new();

    for event in parser {
        match event {
            Event::Start(Tag::Heading { .. }) => {
                in_any_heading = true;
            }
            Event::End(TagEnd::Heading(_)) => {
                if !any_title.trim().is_empty() {
                    return any_title.trim().to_string();
                }
                in_any_heading = false;
            }
            Event::Text(text) if in_any_heading => {
                any_title.push_str(&text);
            }
            _ => {}
        }
    }

    "Untitled Document".to_string()
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

fn clean_content(document: Html) -> Html {
    // Create selectors for unwanted elements
    let unwanted_selector = Selector::parse(
        ".advertisement, .ads, .sidebar, .menu, .navigation, .anchor, a.src.rightside",
    )
    .expect("valid selector");

    // Create selector for main content areas
    let main_content_selector =
        Selector::parse("main, article, .content, .main-content, #content, #main")
            .expect("valid selector");

    // Create selector for body as fallback
    let body_selector = Selector::parse("body").expect("valid selector");

    // First, try to find main content area
    if let Some(main_element) = document.select(&main_content_selector).next() {
        // Clone the main element and create a new document
        let main_html = main_element.html();
        let mut cleaned_doc = Html::parse_fragment(&main_html);

        // Remove unwanted elements from the main content
        remove_unwanted_elements(&mut cleaned_doc, &unwanted_selector);

        return cleaned_doc;
    }

    // Fallback to body if no main content found
    if let Some(body_element) = document.select(&body_selector).next() {
        let body_html = body_element.html();
        let mut cleaned_doc = Html::parse_fragment(&body_html);

        // Remove unwanted elements from the body
        remove_unwanted_elements(&mut cleaned_doc, &unwanted_selector);

        return cleaned_doc;
    }

    // If neither main content nor body found, return the original document
    // after removing unwanted elements
    let mut cleaned_doc = document;
    remove_unwanted_elements(&mut cleaned_doc, &unwanted_selector);
    cleaned_doc
}

// Helper function to remove unwanted elements from an HTML document
fn remove_unwanted_elements(document: &mut Html, unwanted_selector: &scraper::Selector) {
    // Collect all unwanted element node IDs first to avoid borrowing issues
    let unwanted_node_ids: Vec<_> = document
        .select(unwanted_selector)
        .map(|element| element.id())
        .collect();

    // Remove each unwanted element
    for node_id in unwanted_node_ids {
        if let Some(mut node) = document.tree.get_mut(node_id) {
            node.detach();
        }
    }
}
