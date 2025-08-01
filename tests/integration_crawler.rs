#![expect(
    clippy::tests_outside_test_module,
    reason = "integration tests are only compiled in test mode"
)]

use anyhow::Result;
use docs_mcp::config::Config;
use docs_mcp::crawler::robots::fetch_robots_txt;
use docs_mcp::crawler::{CrawlerConfig, HttpClient, SiteCrawler, extract_links, validate_url};
use docs_mcp::database::sqlite::{Database, NewSite, SiteQueries};
use serial_test::serial;
use tempfile::TempDir;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Test helper to create a test database with shared cache
async fn create_test_database() -> Result<Database> {
    // Use shared cache for in-memory database to work with connection pools
    let database = Database::new("file::memory:?cache=shared").await?;

    // Manually run migrations to ensure tables exist
    database.run_migrations().await?;

    // Verify the migration worked by checking if we can query the sites table
    let _count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sites")
        .fetch_one(database.pool())
        .await?;

    Ok(database)
}

/// Test helper to create a mock documentation site
async fn setup_mock_docs_site(server: &MockServer) {
    // Mock the base page
    Mock::given(method("GET"))
        .and(path("/docs/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <title>Test Documentation</title>
            </head>
            <body>
                <h1>Welcome to Test Docs</h1>
                <nav>
                    <ul>
                        <li><a href="/docs/getting-started/">Getting Started</a></li>
                        <li><a href="/docs/api/">API Reference</a></li>
                        <li><a href="/docs/examples/">Examples</a></li>
                        <li><a href="https://external.com/">External Link</a></li>
                    </ul>
                </nav>
                <main>
                    <h2>Overview</h2>
                    <p>This is the main documentation page with useful content.</p>
                    <pre><code>
                    function example() {
                        return "Hello World";
                    }
                    </code></pre>
                </main>
            </body>
            </html>
            "#,
            "text/html",
        ))
        .mount(server)
        .await;

    // Mock getting started page
    Mock::given(method("GET"))
        .and(path("/docs/getting-started/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <title>Getting Started - Test Documentation</title>
            </head>
            <body>
                <h1>Getting Started</h1>
                <h2>Installation</h2>
                <p>To install the software, run the following command:</p>
                <pre><code>npm install test-package</code></pre>
                
                <h2>Configuration</h2>
                <p>Configure your application by creating a config file.</p>
                <a href="/docs/getting-started/installation/">Detailed Installation</a>
            </body>
            </html>
            "#,
            "text/html",
        ))
        .mount(server)
        .await;

    // Mock detailed installation page
    Mock::given(method("GET"))
        .and(path("/docs/getting-started/installation/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <title>Installation Guide - Test Documentation</title>
            </head>
            <body>
                <h1>Detailed Installation Guide</h1>
                <h2>Prerequisites</h2>
                <p>Before installing, ensure you have:</p>
                <ul>
                    <li>Node.js 18+</li>
                    <li>npm or yarn</li>
                </ul>
                
                <h2>Step-by-step Installation</h2>
                <ol>
                    <li>Download the package</li>
                    <li>Run the installer</li>
                    <li>Configure your environment</li>
                </ol>
            </body>
            </html>
            "#,
            "text/html",
        ))
        .mount(server)
        .await;

    // Mock API page
    Mock::given(method("GET"))
        .and(path("/docs/api/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <title>API Reference - Test Documentation</title>
            </head>
            <body>
                <h1>API Reference</h1>
                <h2>Authentication</h2>
                <p>All API requests require authentication using an API key.</p>
                
                <h2>Endpoints</h2>
                <h3>GET /users</h3>
                <p>Retrieve a list of users.</p>
                
                <h3>POST /users</h3>
                <p>Create a new user.</p>
                <code>
                {
                    "name": "John Doe",
                    "email": "john@example.com"
                }
                </code>
            </body>
            </html>
            "#,
            "text/html",
        ))
        .mount(server)
        .await;

    // Mock examples page
    Mock::given(method("GET"))
        .and(path("/docs/examples/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <title>Examples - Test Documentation</title>
            </head>
            <body>
                <h1>Code Examples</h1>
                <h2>Basic Usage</h2>
                <p>Here's a basic example of how to use the API:</p>
                <pre><code>
                const client = new ApiClient({
                    apiKey: 'your-api-key'
                });

                const users = await client.getUsers();
                console.log(users);
                </code></pre>
                
                <h2>Advanced Usage</h2>
                <p>For more complex scenarios:</p>
                <pre><code>
                const client = new ApiClient({
                    apiKey: 'your-api-key',
                    baseUrl: 'https://api.example.com'
                });

                const response = await client.post('/users', {
                    name: 'Jane Smith',
                    email: 'jane@example.com'
                });
                </code></pre>
            </body>
            </html>
            "#,
            "text/html",
        ))
        .mount(server)
        .await;

    // Mock robots.txt
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"
            User-agent: *
            Allow: /docs/
            Disallow: /admin/
            Disallow: /private/
            "#,
        ))
        .mount(server)
        .await;
}

/// Test helper to setup a site with restricted robots.txt
async fn setup_restricted_mock_site(server: &MockServer) {
    // Mock robots.txt that blocks our user agent
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"
            User-agent: docs-mcp/0.1.0 (Documentation Indexer)
            Disallow: /

            User-agent: *
            Allow: /
            "#,
        ))
        .mount(server)
        .await;

    // Mock a page that should be blocked
    Mock::given(method("GET"))
        .and(path("/docs/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"
            <!DOCTYPE html>
            <html>
            <head><title>Blocked Content</title></head>
            <body><h1>This should be blocked by robots.txt</h1></body>
            </html>
            "#,
            "text/html",
        ))
        .mount(server)
        .await;
}

#[tokio::test]
#[serial]
async fn basic_site_crawling() -> Result<()> {
    // Start mock server
    let server = MockServer::start().await;
    setup_mock_docs_site(&server).await;

    // Create test database and site
    let database = create_test_database().await?;
    let base_url = format!("{}/docs/", server.uri());

    let new_site = NewSite {
        base_url: base_url.clone(),
        index_url: base_url.clone(),
        name: "Test Documentation".to_string(),
        version: "1.0".to_string(),
    };

    let site = SiteQueries::create(database.pool(), new_site).await?;

    // Create crawler and crawl the site
    let config = CrawlerConfig {
        rate_limit_ms: 10, // Faster for tests
        max_retries: 1,    // Less retries for tests
        enable_js_rendering: false,
        ..CrawlerConfig::default()
    };

    let config_path = TempDir::new()?;
    let mut crawler = SiteCrawler::new(
        database.pool().clone(),
        config,
        Config::load(config_path.path())?,
    );
    let stats = crawler.crawl_site(site.id, &base_url, &base_url).await?;

    // Verify crawl statistics
    assert!(stats.total_urls > 0, "Should have discovered URLs");
    assert!(stats.successful_crawls > 0, "Should have successful crawls");
    assert_eq!(
        stats.robots_blocked, 0,
        "No URLs should be blocked by robots.txt"
    );

    // Verify that multiple pages were crawled
    assert!(
        stats.total_urls >= 4,
        "Should discover at least 4 URLs (base + 3 linked pages)"
    );
    assert!(
        stats.successful_crawls >= 4,
        "Should successfully crawl at least 4 pages"
    );

    Ok(())
}

#[tokio::test]
async fn url_validation() -> Result<()> {
    // Test valid URLs
    let valid_urls = [
        "https://example.com/docs/",
        "http://localhost:3000/api/",
        "https://docs.example.com/v1/",
    ];

    for url in &valid_urls {
        let result = validate_url(url);
        assert!(result.is_ok(), "URL {} should be valid", url);
    }

    // Test invalid URLs
    let invalid_urls = [
        "not-a-url",
        "ftp://example.com/docs/",
        "//example.com/docs/",
        "https://",
    ];

    for url in &invalid_urls {
        let result = validate_url(url);
        assert!(result.is_err(), "URL {} should be invalid", url);
    }

    Ok(())
}

#[tokio::test]
#[serial]
async fn robots_txt_compliance() -> Result<()> {
    // Start mock server with restricted robots.txt
    let server = MockServer::start().await;
    setup_restricted_mock_site(&server).await;

    // Create test database and site
    let database = create_test_database().await?;
    let base_url = format!("{}/docs/", server.uri());

    let new_site = NewSite {
        base_url: base_url.clone(),
        index_url: base_url.clone(),
        name: "Restricted Test Site".to_string(),
        version: "1.0".to_string(),
    };

    let site = SiteQueries::create(database.pool(), new_site).await?;

    // Create crawler with our user agent
    let config = CrawlerConfig {
        user_agent: "docs-mcp/0.1.0 (Documentation Indexer)".to_string(),
        rate_limit_ms: 10,
        max_retries: 1,
        ..CrawlerConfig::default()
    };

    let config_path = TempDir::new()?;
    let mut crawler = SiteCrawler::new(
        database.pool().clone(),
        config,
        Config::load(config_path.path())?,
    );
    let stats = crawler.crawl_site(site.id, &base_url, &base_url).await?;

    // Verify that the URL was blocked by robots.txt
    assert_eq!(
        stats.successful_crawls, 0,
        "No crawls should succeed due to robots.txt"
    );
    assert!(
        stats.robots_blocked > 0,
        "URLs should be blocked by robots.txt"
    );

    Ok(())
}

#[tokio::test]
#[serial]
async fn error_handling() -> Result<()> {
    // Start mock server
    let server = MockServer::start().await;

    // Mock a page that returns 404
    Mock::given(method("GET"))
        .and(path("/docs/"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;

    // Mock robots.txt to allow crawling
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string("User-agent: *\nAllow: /"))
        .mount(&server)
        .await;

    // Create test database and site
    let database = create_test_database().await?;
    let base_url = format!("{}/docs/", server.uri());

    let new_site = NewSite {
        base_url: base_url.clone(),
        index_url: base_url.clone(),
        name: "Error Test Site".to_string(),
        version: "1.0".to_string(),
    };

    let site = SiteQueries::create(database.pool(), new_site).await?;

    // Create crawler
    let config = CrawlerConfig {
        rate_limit_ms: 10,
        max_retries: 1,
        retry_delay_seconds: 1, // Faster retry for tests
        ..CrawlerConfig::default()
    };

    let config_path = TempDir::new()?;
    let mut crawler = SiteCrawler::new(
        database.pool().clone(),
        config,
        Config::load(config_path.path())?,
    );
    let crawl_error = crawler
        .crawl_site(site.id, &base_url, &base_url)
        .await
        .expect_err("should return an error");

    // Verify error handling
    assert_eq!(crawl_error.to_string(), "All 1 crawl attempts failed");

    Ok(())
}

#[tokio::test]
#[serial]
async fn content_extraction() -> Result<()> {
    // Start mock server with a single complex page
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/docs/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            r#"
            <!DOCTYPE html>
            <html>
            <head>
                <title>Complex Documentation Page</title>
            </head>
            <body>
                <nav>Navigation content (should be filtered)</nav>
                <main>
                    <h1>Main Title</h1>
                    <h2>Section 1</h2>
                    <p>Content for section 1 with important information.</p>
                    
                    <h2>Section 2</h2>
                    <p>Content for section 2.</p>
                    <h3>Subsection 2.1</h3>
                    <p>Subsection content with more details.</p>
                    
                    <h2>Code Examples</h2>
                    <pre><code>
                    const example = {
                        name: "test",
                        value: 42
                    };
                    </code></pre>
                </main>
                <footer>Footer content (should be filtered)</footer>
            </body>
            </html>
            "#,
            "text/html",
        ))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string("User-agent: *\nAllow: /"))
        .mount(&server)
        .await;

    // Create test database and site
    let database = create_test_database().await?;
    let base_url = format!("{}/docs/", server.uri());

    let new_site = NewSite {
        base_url: base_url.clone(),
        index_url: base_url.clone(),
        name: "Content Test Site".to_string(),
        version: "1.0".to_string(),
    };

    let site = SiteQueries::create(database.pool(), new_site).await?;

    // Create crawler
    let config = CrawlerConfig {
        rate_limit_ms: 10,
        max_retries: 1,
        ..CrawlerConfig::default()
    };

    let config_path = TempDir::new()?;
    let mut crawler = SiteCrawler::new(
        database.pool().clone(),
        config,
        Config::load(config_path.path())?,
    );
    let stats = crawler.crawl_site(site.id, &base_url, &base_url).await?;

    // Verify content was processed
    assert_eq!(
        stats.successful_crawls, 1,
        "Should successfully crawl 1 page"
    );
    assert_eq!(stats.failed_crawls, 0, "Should have no failed crawls");

    // Note: In a full implementation, we would check the extracted content
    // was properly stored in indexed_chunks table with proper heading paths
    Ok(())
}

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
        .respond_with(ResponseTemplate::new(200).set_body_raw(html_content, "text/html"))
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
    let links = extract_links(&html, &base_url, &base_url).expect("extract_links should succeed");

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
        .respond_with(ResponseTemplate::new(200).set_body_raw(html_content, "text/html"))
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
    let links = extract_links(&html, &base_url, &base_url).expect("extract_links should succeed");

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
