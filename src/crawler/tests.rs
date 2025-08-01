use super::extract_links as extract_links_impl;
use super::is_retryable_error as is_retryable_error_impl;
use super::normalize_path_for_filtering as normalize_path_for_filtering_impl;
use super::should_crawl_url as should_crawl_url_impl;
use super::validate_url as validate_url_impl;
use super::*;

#[test]
fn validate_url() {
    // Valid URLs
    assert!(validate_url_impl("https://example.com").is_ok());
    assert!(validate_url_impl("http://docs.rs/regex/1.0/").is_ok());
    assert!(validate_url_impl("https://docs.python.org/3/library/").is_ok());

    // Invalid URLs
    assert!(validate_url_impl("ftp://example.com").is_err());
    assert!(validate_url_impl("not-a-url").is_err());
    assert!(validate_url_impl("").is_err());
    assert!(validate_url_impl("https://").is_err());
}

#[test]
fn should_crawl_url() {
    let base = Url::parse("https://docs.rs/regex/1.10.6/regex/").expect("url should parse");

    // Should crawl - same path prefix
    assert!(should_crawl_url_impl(
        &Url::parse("https://docs.rs/regex/1.10.6/regex/struct.Regex.html")
            .expect("url should parse"),
        &base
    ));

    // Should crawl - exact match
    assert!(should_crawl_url_impl(&base, &base));

    // Should not crawl - different host
    assert!(!should_crawl_url_impl(
        &Url::parse("https://doc.rust-lang.org/std/").expect("url should parse"),
        &base
    ));

    // Should not crawl - different path prefix
    assert!(!should_crawl_url_impl(
        &Url::parse("https://docs.rs/other-crate/1.0/").expect("url should parse"),
        &base
    ));

    // Should not crawl - different scheme
    assert!(!should_crawl_url_impl(
        &Url::parse("http://docs.rs/regex/1.10.6/regex/").expect("url should parse"),
        &base
    ));
}

#[test]
fn normalize_path_for_filtering() {
    // Already ends with slash
    assert_eq!(normalize_path_for_filtering_impl("/docs/"), "/docs/");

    // Directory without trailing slash
    assert_eq!(normalize_path_for_filtering_impl("/docs"), "/docs/");

    // Filename with extension
    assert_eq!(
        normalize_path_for_filtering_impl("/docs/index.html"),
        "/docs/"
    );

    // Complex path with filename
    assert_eq!(
        normalize_path_for_filtering_impl("/regex/1.10.6/regex/struct.Regex.html"),
        "/regex/1.10.6/regex/"
    );

    // Root path
    assert_eq!(normalize_path_for_filtering_impl("/"), "/");

    // No leading slash
    assert_eq!(normalize_path_for_filtering_impl("docs"), "docs/");
}

#[test]
fn is_retryable_error() {
    // Retryable errors
    assert!(is_retryable_error_impl(&anyhow!("Connection timeout")));
    assert!(is_retryable_error_impl(&anyhow!(
        "HTTP error 500: Internal Server Error"
    )));
    assert!(is_retryable_error_impl(&anyhow!(
        "HTTP error 503: Service Unavailable"
    )));
    assert!(is_retryable_error_impl(&anyhow!(
        "HTTP error 429: Too Many Requests"
    )));
    assert!(is_retryable_error_impl(&anyhow!("Network unreachable")));

    // Non-retryable errors
    assert!(!is_retryable_error_impl(&anyhow!(
        "HTTP error 404: Not Found"
    )));
    assert!(!is_retryable_error_impl(&anyhow!(
        "HTTP error 401: Unauthorized"
    )));
    assert!(!is_retryable_error_impl(&anyhow!("Invalid URL format")));
    assert!(!is_retryable_error_impl(&anyhow!("Parse error")));
}

#[test]
fn extract_links() {
    let base_url = Url::parse("https://docs.rs/regex/1.10.6/regex/").expect("url should parse");

    let html = r#"
        <html>
            <body>
                <a href="struct.Regex.html">Regex struct</a>
                <a href="../../../">Root</a>
                <a href="https://docs.rs/regex/1.10.6/regex/fn.escape.html">Escape function</a>
                <a href="https://doc.rust-lang.org/std/">Std docs</a>
                <a href="mailto:test@example.com">Email</a>
                <a href="javascript:void(0)">JS link</a>
                <a href="\#section">Anchor</a>
                <a>No href</a>
            </body>
        </html>
    "#;

    let links =
        extract_links_impl(html, &base_url, &base_url).expect("extract_links should succeed");

    // Should only include links that match the base URL path
    assert_eq!(links.len(), 2);
    assert!(
        links.contains(
            &Url::parse("https://docs.rs/regex/1.10.6/regex/struct.Regex.html")
                .expect("url should parse")
        )
    );
    assert!(links.contains(
        &Url::parse("https://docs.rs/regex/1.10.6/regex/fn.escape.html").expect("url should parse")
    ));
}

#[tokio::test]
async fn rate_limiting() {
    let config = CrawlerConfig {
        rate_limit_ms: 100,
        ..Default::default()
    };

    let mut client = HttpClient::new(config);

    let start = Instant::now();

    // First request should be immediate
    client.apply_rate_limit().await;
    let first_duration = start.elapsed();

    // Second request should wait
    client.apply_rate_limit().await;
    let second_duration = start.elapsed();

    // Should have waited at least 100ms between requests
    assert!(second_duration.as_millis() >= 100);
    assert!(first_duration.as_millis() < 50); // First should be immediate
}

#[test]
fn malformed_html_parsing() {
    // Test that we can handle malformed HTML gracefully
    let malformed_html = r#"
            <html>
                <body>
                    <a href="/valid-link.html">Valid Link
                    <a href="/another-link.html">Another Link</a>
                    <p>Some text without closing tag
                    <a href="/third-link.html">Third Link</a>
                </body>
            <!-- Missing closing html tag
        "#;

    let base_url = Url::parse("https://example.com/").expect("url should parse");
    let links = extract_links_impl(malformed_html, &base_url, &base_url)
        .expect("extract_links should succeed");

    // Should still extract valid links despite malformed HTML
    assert_eq!(links.len(), 3);
    assert!(
        links.contains(
            &Url::parse("https://example.com/valid-link.html").expect("url should parse")
        )
    );
    assert!(
        links.contains(
            &Url::parse("https://example.com/another-link.html").expect("url should parse")
        )
    );
    assert!(
        links.contains(
            &Url::parse("https://example.com/third-link.html").expect("url should parse")
        )
    );
}

#[test]
fn edge_case_url_validation() {
    // Test various edge cases for URL validation

    // URLs with different ports
    assert!(validate_url_impl("https://example.com:8080/docs").is_ok());
    assert!(validate_url_impl("http://localhost:3000").is_ok());

    // URLs with authentication (should be valid but we might not want to crawl them)
    assert!(validate_url_impl("https://user:pass@example.com").is_ok());

    // URLs with complex paths
    assert!(validate_url_impl("https://docs.rs/regex/1.10.6/regex/struct.Regex.html").is_ok());

    // Invalid schemes
    assert!(validate_url_impl("ftp://example.com").is_err());
    assert!(validate_url_impl("file:///local/file.html").is_err());

    // Malformed URLs
    assert!(validate_url_impl("https://").is_err());
    assert!(validate_url_impl("not-a-url").is_err());
    assert!(validate_url_impl("").is_err());
}

#[test]
fn complex_url_filtering_scenarios() {
    // Test complex URL filtering scenarios

    // Base URL with filename
    let base =
        Url::parse("https://docs.rs/regex/1.10.6/regex/index.html").expect("url should parse");

    // Should crawl pages in the same directory
    assert!(should_crawl_url_impl(
        &Url::parse("https://docs.rs/regex/1.10.6/regex/struct.Regex.html")
            .expect("url should parse"),
        &base
    ));

    // Should crawl subdirectories
    assert!(should_crawl_url_impl(
        &Url::parse("https://docs.rs/regex/1.10.6/regex/enum/Error.html")
            .expect("url should parse"),
        &base
    ));

    // Should not crawl parent directories
    assert!(!should_crawl_url_impl(
        &Url::parse("https://docs.rs/regex/1.10.6/").expect("url should parse"),
        &base
    ));

    // Should not crawl different crates
    assert!(!should_crawl_url_impl(
        &Url::parse("https://docs.rs/serde/1.0/serde/").expect("url should parse"),
        &base
    ));

    // Base URL without filename
    let base2 = Url::parse("https://docs.python.org/3/library/").expect("url should parse");

    // Should crawl subdirectories
    assert!(should_crawl_url_impl(
        &Url::parse("https://docs.python.org/3/library/os.html").expect("url should parse"),
        &base2
    ));

    // Should crawl nested subdirectories
    assert!(should_crawl_url_impl(
        &Url::parse("https://docs.python.org/3/library/concurrent/futures.html")
            .expect("url should parse"),
        &base2
    ));

    // Should not crawl different versions
    assert!(!should_crawl_url_impl(
        &Url::parse("https://docs.python.org/2/library/os.html").expect("url should parse"),
        &base2
    ));
}
