use std::time::Instant;

use anyhow::anyhow;
use url::Url;

use crate::crawler::{CrawlerConfig, HttpClient};

#[test]
fn validate_url() {
    // Valid URLs
    assert!(super::validate_url("https://example.com").is_ok());
    assert!(super::validate_url("http://docs.rs/regex/1.0/").is_ok());
    assert!(super::validate_url("https://docs.python.org/3/library/").is_ok());

    // Invalid URLs
    assert!(super::validate_url("ftp://example.com").is_err());
    assert!(super::validate_url("not-a-url").is_err());
    assert!(super::validate_url("").is_err());
    assert!(super::validate_url("https://").is_err());
}

#[test]
fn should_crawl_url() {
    let base = Url::parse("https://docs.rs/regex/1.10.6/regex/").expect("url should parse");

    // Should crawl - same path prefix
    assert!(super::should_crawl_url(
        &Url::parse("https://docs.rs/regex/1.10.6/regex/struct.Regex.html")
            .expect("url should parse"),
        &base
    ));

    // Should crawl - exact match
    assert!(super::should_crawl_url(&base, &base));

    // Should not crawl - different host
    assert!(!super::should_crawl_url(
        &Url::parse("https://doc.rust-lang.org/std/").expect("url should parse"),
        &base
    ));

    // Should not crawl - different path prefix
    assert!(!super::should_crawl_url(
        &Url::parse("https://docs.rs/other-crate/1.0/").expect("url should parse"),
        &base
    ));

    // Should not crawl - different scheme
    assert!(!super::should_crawl_url(
        &Url::parse("http://docs.rs/regex/1.10.6/regex/").expect("url should parse"),
        &base
    ));
}

#[test]
fn normalize_path_for_filtering() {
    // Already ends with slash
    assert_eq!(super::normalize_path_for_filtering("/docs/"), "/docs/");

    // Directory without trailing slash
    assert_eq!(super::normalize_path_for_filtering("/docs"), "/docs/");

    // Filename with extension
    assert_eq!(
        super::normalize_path_for_filtering("/docs/index.html"),
        "/docs/"
    );

    // Complex path with filename
    assert_eq!(
        super::normalize_path_for_filtering("/regex/1.10.6/regex/struct.Regex.html"),
        "/regex/1.10.6/regex/"
    );

    // Root path
    assert_eq!(super::normalize_path_for_filtering("/"), "/");

    // No leading slash
    assert_eq!(super::normalize_path_for_filtering("docs"), "docs/");
}

#[test]
fn is_retryable_error() {
    // Retryable errors
    assert!(super::is_retryable_error(&anyhow!("Connection timeout")));
    assert!(super::is_retryable_error(&anyhow!(
        "HTTP error 500: Internal Server Error"
    )));
    assert!(super::is_retryable_error(&anyhow!(
        "HTTP error 503: Service Unavailable"
    )));
    assert!(super::is_retryable_error(&anyhow!(
        "HTTP error 429: Too Many Requests"
    )));
    assert!(super::is_retryable_error(&anyhow!("Network unreachable")));

    // Non-retryable errors
    assert!(!super::is_retryable_error(&anyhow!(
        "HTTP error 404: Not Found"
    )));
    assert!(!super::is_retryable_error(&anyhow!(
        "HTTP error 401: Unauthorized"
    )));
    assert!(!super::is_retryable_error(&anyhow!("Invalid URL format")));
    assert!(!super::is_retryable_error(&anyhow!("Parse error")));
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

    let links = super::extract_links(html, &base_url).expect("extract_links should succeed");

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
    let links =
        super::extract_links(malformed_html, &base_url).expect("extract_links should succeed");

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
    assert!(super::validate_url("https://example.com:8080/docs").is_ok());
    assert!(super::validate_url("http://localhost:3000").is_ok());

    // URLs with authentication (should be valid but we might not want to crawl them)
    assert!(super::validate_url("https://user:pass@example.com").is_ok());

    // URLs with complex paths
    assert!(super::validate_url("https://docs.rs/regex/1.10.6/regex/struct.Regex.html").is_ok());

    // Invalid schemes
    assert!(super::validate_url("ftp://example.com").is_err());
    assert!(super::validate_url("file:///local/file.html").is_err());

    // Malformed URLs
    assert!(super::validate_url("https://").is_err());
    assert!(super::validate_url("not-a-url").is_err());
    assert!(super::validate_url("").is_err());
}

#[test]
fn complex_url_filtering_scenarios() {
    // Test complex URL filtering scenarios

    // Base URL with filename
    let base =
        Url::parse("https://docs.rs/regex/1.10.6/regex/index.html").expect("url should parse");

    // Should crawl pages in the same directory
    assert!(super::should_crawl_url(
        &Url::parse("https://docs.rs/regex/1.10.6/regex/struct.Regex.html")
            .expect("url should parse"),
        &base
    ));

    // Should crawl subdirectories
    assert!(super::should_crawl_url(
        &Url::parse("https://docs.rs/regex/1.10.6/regex/enum/Error.html")
            .expect("url should parse"),
        &base
    ));

    // Should not crawl parent directories
    assert!(!super::should_crawl_url(
        &Url::parse("https://docs.rs/regex/1.10.6/").expect("url should parse"),
        &base
    ));

    // Should not crawl different crates
    assert!(!super::should_crawl_url(
        &Url::parse("https://docs.rs/serde/1.0/serde/").expect("url should parse"),
        &base
    ));

    // Base URL without filename
    let base2 = Url::parse("https://docs.python.org/3/library/").expect("url should parse");

    // Should crawl subdirectories
    assert!(super::should_crawl_url(
        &Url::parse("https://docs.python.org/3/library/os.html").expect("url should parse"),
        &base2
    ));

    // Should crawl nested subdirectories
    assert!(super::should_crawl_url(
        &Url::parse("https://docs.python.org/3/library/concurrent/futures.html")
            .expect("url should parse"),
        &base2
    ));

    // Should not crawl different versions
    assert!(!super::should_crawl_url(
        &Url::parse("https://docs.python.org/2/library/os.html").expect("url should parse"),
        &base2
    ));
}

mod integration_tests {
    use crate::crawler::robots::fetch_robots_txt;

    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };

    #[tokio::test]
    async fn http_client_success() {
        // Setup mock server
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/test"))
            .respond_with(ResponseTemplate::new(200).set_body_string("Hello, World!"))
            .mount(&mock_server)
            .await;

        // Test HTTP client
        let config = CrawlerConfig {
            rate_limit_ms: 10, // Faster for testing
            ..Default::default()
        };
        let mut client = HttpClient::new(config);

        let url = format!("{}/test", mock_server.uri());
        let response = client.get(&url).await.expect("get call should succeed");

        assert_eq!(response, "Hello, World!");
    }

    #[tokio::test]
    async fn http_client_404_error() {
        // Setup mock server
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/not-found"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        // Test HTTP client
        let mut client = HttpClient::default();

        let url = format!("{}/not-found", mock_server.uri());
        let result = client.get(&url).await;

        let result_message = result.expect_err("result should be an error").to_string();
        assert!(
            result_message.contains("404"),
            "Did not find '404' in result string: {}",
            result_message
        );
    }

    #[tokio::test]
    async fn http_client_retry_on_500() {
        // Setup mock server that fails twice then succeeds
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/retry-test"))
            .respond_with(ResponseTemplate::new(500))
            .up_to_n_times(2)
            .mount(&mock_server)
            .await;

        Mock::given(method("GET"))
            .and(path("/retry-test"))
            .respond_with(ResponseTemplate::new(200).set_body_string("Success after retry"))
            .mount(&mock_server)
            .await;

        // Test HTTP client with fast retry
        let config = CrawlerConfig {
            max_retries: 3,
            retry_delay_seconds: 1, // Fast retry for testing
            rate_limit_ms: 10,
            ..Default::default()
        };
        let mut client = HttpClient::new(config);

        let url = format!("{}/retry-test", mock_server.uri());
        let response = client.get(&url).await.expect("get call should succeed");

        assert_eq!(response, "Success after retry");
    }

    #[tokio::test]
    async fn robots_txt_fetch_success() {
        // Setup mock server
        let mock_server = MockServer::start().await;

        let robots_content = r#"
            User-agent: *
            Disallow: /private/
            Allow: /public/
        "#;

        Mock::given(method("GET"))
            .and(path("/robots.txt"))
            .respond_with(ResponseTemplate::new(200).set_body_string(robots_content))
            .mount(&mock_server)
            .await;

        // Test robots.txt fetching
        let mut client = HttpClient::default();
        let base_url = Url::parse(&mock_server.uri()).expect("url should parse");

        let robots = fetch_robots_txt(&mut client, &base_url)
            .await
            .expect("fetch_robots_txt should succeed");

        // Test the parsed robots.txt
        let private_url = base_url
            .join("/private/secret.html")
            .expect("join should succeed");
        let public_url = base_url
            .join("/public/docs.html")
            .expect("join should succeed");

        assert!(!robots.is_allowed(&private_url, "docs-mcp"));
        assert!(robots.is_allowed(&public_url, "docs-mcp"));
    }

    #[tokio::test]
    async fn robots_txt_fetch_404() {
        // Setup mock server that returns 404 for robots.txt
        let mock_server = MockServer::start().await;

        Mock::given(method("GET"))
            .and(path("/robots.txt"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        // Test robots.txt fetching with 404
        let mut client = HttpClient::default();
        let base_url = Url::parse(&mock_server.uri()).expect("url should parse");

        let robots = fetch_robots_txt(&mut client, &base_url)
            .await
            .expect("fetch_robots_txt should succeed");

        // Should allow all URLs when no robots.txt
        let test_url = base_url.join("/anything").expect("join should succeed");
        assert!(robots.is_allowed(&test_url, "docs-mcp"));
    }

    #[tokio::test]
    async fn extract_links_integration() {
        // Setup mock server
        let mock_server = MockServer::start().await;

        let html_content = format!(
            r#"
            <html>
                <head><title>Test Page</title></head>
                <body>
                    <a href="/page1.html">Page 1</a>
                    <a href="/page2.html">Page 2</a>
                    <a href="{}/external.html">External Link</a>
                    <a href="mailto:test@example.com">Email</a>
                    <a href="\#anchor">Anchor</a>
                    <a href="javascript:void(0)">JS Link</a>
                </body>
            </html>
        "#,
            "https://external.com"
        );

        Mock::given(method("GET"))
            .and(path("/test-page"))
            .respond_with(ResponseTemplate::new(200).set_body_string(html_content))
            .mount(&mock_server)
            .await;

        // Fetch and extract links
        let mut client = HttpClient::default();
        let base_url = Url::parse(&format!("{}/", mock_server.uri())).expect("url should parse");
        let test_url = format!("{}/test-page", mock_server.uri());

        let html = client
            .get(&test_url)
            .await
            .expect("get call should succeed");
        let links =
            super::super::extract_links(&html, &base_url).expect("extract_links should succeed");

        // Should extract only valid internal links
        assert_eq!(links.len(), 2);
        assert!(links.contains(&base_url.join("/page1.html").expect("join should succeed")));
        assert!(links.contains(&base_url.join("/page2.html").expect("join should succeed")));
    }

    #[tokio::test]
    async fn crawl_workflow_with_robots_txt() {
        // Setup mock server
        let mock_server = MockServer::start().await;

        // Setup robots.txt
        let robots_content = r#"
            User-agent: docs-mcp
            Disallow: /admin/
            Allow: /docs/
        "#;

        Mock::given(method("GET"))
            .and(path("/robots.txt"))
            .respond_with(ResponseTemplate::new(200).set_body_string(robots_content))
            .mount(&mock_server)
            .await;

        // Setup main page with links
        let html_content = r#"
            <html>
                <body>
                    <a href="/docs/page1.html">Allowed Page</a>
                    <a href="/admin/secret.html">Admin Page</a>
                    <a href="/docs/page2.html">Another Allowed Page</a>
                </body>
            </html>
        "#;

        Mock::given(method("GET"))
            .and(path("/"))
            .respond_with(ResponseTemplate::new(200).set_body_string(html_content))
            .mount(&mock_server)
            .await;

        // Test the workflow
        let mut client = HttpClient::default();
        let base_url = Url::parse(&format!("{}/", mock_server.uri())).expect("url should parse");

        // 1. Fetch robots.txt
        let robots = fetch_robots_txt(&mut client, &base_url)
            .await
            .expect("fetch_robots_txt should succeed");

        // 2. Fetch main page
        let html = client
            .get(base_url.as_str())
            .await
            .expect("get call should succeed");
        let links =
            super::super::extract_links(&html, &base_url).expect("extract_links should succeed");

        // 3. Filter links based on robots.txt
        let allowed_links: Vec<_> = links
            .into_iter()
            .filter(|url| robots.is_allowed(url, "docs-mcp"))
            .collect();

        // Should only include docs pages, not admin
        assert_eq!(allowed_links.len(), 2);
        assert!(
            allowed_links
                .iter()
                .any(|url| url.path() == "/docs/page1.html")
        );
        assert!(
            allowed_links
                .iter()
                .any(|url| url.path() == "/docs/page2.html")
        );
        assert!(
            !allowed_links
                .iter()
                .any(|url| url.path() == "/admin/secret.html")
        );
    }
}
