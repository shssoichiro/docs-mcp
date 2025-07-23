use super::clean_text as clean_text_impl;
use super::extract_metadata as extract_metadata_impl;
use super::*;

#[test]
fn extract_simple_content() {
    let html = r#"
            <html>
                <head><title>Test Page</title></head>
                <body>
                    <h1>Main Heading</h1>
                    <p>This is a paragraph of text.</p>
                    <h2>Sub Heading</h2>
                    <p>Another paragraph with more content.</p>
                </body>
            </html>
        "#;

    let config = ExtractionConfig::default();
    let result = extract_content(html, &config).expect("extract_content should succeed");

    assert_eq!(result.title, "Test Page");
    assert!(!result.sections.is_empty());
    assert!(!result.raw_text.is_empty());
}

#[test]
fn extract_with_code_blocks() {
    let html = r#"
            <html>
                <body>
                    <h1>Code Example</h1>
                    <p>Here's some code:</p>
                    <pre><code>fn main() {
    println!("Hello, world!");
}</code></pre>
                    <p>That was the code.</p>
                </body>
            </html>
        "#;

    let config = ExtractionConfig::default();
    let result = extract_content(html, &config).expect("extract_content should succeed");

    assert_eq!(result.title, "Code Example");

    // Should find code blocks
    let has_code = result.sections.iter().any(|s| s.has_code_blocks);
    assert!(has_code);
}

#[test]
fn heading_hierarchy() {
    let html = r#"
            <html>
                <body>
                    <h1>Chapter 1</h1>
                    <p>Chapter content</p>
                    <h2>Section A</h2>
                    <p>Section A content</p>
                    <h3>Subsection 1</h3>
                    <p>Subsection content</p>
                    <h2>Section B</h2>
                    <p>Section B content</p>
                </body>
            </html>
        "#;

    let config = ExtractionConfig::default();
    let result = extract_content(html, &config).expect("extract_content should succeed");

    // Check that heading paths are built correctly
    let paths: Vec<&str> = result
        .sections
        .iter()
        .map(|s| s.heading_path.as_str())
        .collect();

    assert!(paths.contains(&"Chapter 1"));
    assert!(paths.contains(&"Chapter 1 > Section A"));
    assert!(paths.contains(&"Chapter 1 > Section A > Subsection 1"));
    assert!(paths.contains(&"Chapter 1 > Section B"));
}

#[test]
fn clean_text() {
    let input = "  This   is    text with\n\n\nexcessive   whitespace  \n\n  ";
    let cleaned = clean_text_impl(input);
    assert_eq!(cleaned, "This is text with excessive whitespace");
}

#[test]
fn extract_metadata() {
    let html = r#"
            <html>
                <head>
                    <title>Test Page</title>
                    <meta name="description" content="A test page">
                    <meta property="og:title" content="Open Graph Title">
                    <meta name="keywords" content="test, page, example">
                </head>
                <body></body>
            </html>
        "#;

    let metadata = extract_metadata_impl(html).expect("extract_metadata should succeed");

    assert_eq!(metadata.get("title"), Some(&"Test Page".to_string()));
    assert_eq!(
        metadata.get("description"),
        Some(&"A test page".to_string())
    );
    assert_eq!(
        metadata.get("og:title"),
        Some(&"Open Graph Title".to_string())
    );
    assert_eq!(
        metadata.get("keywords"),
        Some(&"test, page, example".to_string())
    );
}

#[test]
fn malformed_html() {
    let html = r#"
            <html>
                <head><title>Broken Page</title>
                <body>
                    <h1>Unclosed heading
                    <p>Paragraph without closing tag
                    <div>Nested without proper closure
                        <span>More nesting
                </body>
            </html>
        "#;

    let config = ExtractionConfig::default();
    let content = extract_content(html, &config).expect("extract_content should succeed");

    // Should handle malformed HTML gracefully
    assert_eq!(content.title, "Broken Page");
    assert!(!content.raw_text.is_empty());
}
