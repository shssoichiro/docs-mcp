#![expect(
    clippy::tests_outside_test_module,
    reason = "integration tests are only compiled in test mode"
)]

use anyhow::Result;
use docs_mcp::crawler::{CrawlerConfig, SiteCrawler, validate_url};
use docs_mcp::database::sqlite::{Database, NewSite, SiteQueries};
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
        .respond_with(ResponseTemplate::new(200).set_body_string(
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
        ))
        .mount(server)
        .await;

    // Mock getting started page
    Mock::given(method("GET"))
        .and(path("/docs/getting-started/"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
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
        ))
        .mount(server)
        .await;

    // Mock detailed installation page
    Mock::given(method("GET"))
        .and(path("/docs/getting-started/installation/"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
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
        ))
        .mount(server)
        .await;

    // Mock API page
    Mock::given(method("GET"))
        .and(path("/docs/api/"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
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
        ))
        .mount(server)
        .await;

    // Mock examples page
    Mock::given(method("GET"))
        .and(path("/docs/examples/"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
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
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"
            <!DOCTYPE html>
            <html>
            <head><title>Blocked Content</title></head>
            <body><h1>This should be blocked by robots.txt</h1></body>
            </html>
            "#,
        ))
        .mount(server)
        .await;
}

#[tokio::test]
async fn database_operations() -> Result<()> {
    use sqlx::Connection;
    use sqlx::sqlite::SqliteConnection;

    // Create a single connection to in-memory database
    let mut conn = SqliteConnection::connect("sqlite::memory:").await?;

    // Run migration on the connection
    sqlx::query(include_str!(
        "../src/database/sqlite/migrations/001_initial_schema.sql"
    ))
    .execute(&mut conn)
    .await?;

    // Test raw insertion first
    let now = chrono::Utc::now().naive_utc();
    let result = sqlx::query!(
        "INSERT INTO sites (base_url, name, version, status, created_date) VALUES (?, ?, ?, 'pending', ?)",
        "https://example.com/docs/",
        "Test Documentation",
        "1.0",
        now
    )
    .execute(&mut conn)
    .await?;

    let id = result.last_insert_rowid();

    // Test raw retrieval on the same connection
    let retrieved = sqlx::query!("SELECT * FROM sites WHERE id = ?", id)
        .fetch_optional(&mut conn)
        .await?;
    assert!(retrieved.is_some(), "Should retrieve the inserted site");

    Ok(())
}

#[tokio::test]
async fn basic_site_crawling() -> Result<()> {
    // Start mock server
    let server = MockServer::start().await;
    setup_mock_docs_site(&server).await;

    // Create test database and site
    let database = create_test_database().await?;
    let base_url = format!("{}/docs/", server.uri());

    let new_site = NewSite {
        base_url: base_url.clone(),
        name: "Test Documentation".to_string(),
        version: "1.0".to_string(),
    };

    let site = SiteQueries::create(database.pool(), new_site).await?;

    // Create crawler and crawl the site
    let config = CrawlerConfig {
        rate_limit_ms: 10, // Faster for tests
        max_retries: 1,    // Less retries for tests
        ..CrawlerConfig::default()
    };

    let mut crawler = SiteCrawler::new(database.pool().clone(), config);
    let stats = crawler.crawl_site(site.id, &base_url).await?;

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
async fn robots_txt_compliance() -> Result<()> {
    // Start mock server with restricted robots.txt
    let server = MockServer::start().await;
    setup_restricted_mock_site(&server).await;

    // Create test database and site
    let database = create_test_database().await?;
    let base_url = format!("{}/docs/", server.uri());

    let new_site = NewSite {
        base_url: base_url.clone(),
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

    let mut crawler = SiteCrawler::new(database.pool().clone(), config);
    let stats = crawler.crawl_site(site.id, &base_url).await?;

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

    let mut crawler = SiteCrawler::new(database.pool().clone(), config);
    let stats = crawler.crawl_site(site.id, &base_url).await?;

    // Verify error handling
    assert_eq!(
        stats.successful_crawls, 0,
        "No crawls should succeed with 404"
    );
    assert!(
        stats.failed_crawls > 0,
        "Should have failed crawls due to 404"
    );

    Ok(())
}

#[tokio::test]
async fn content_extraction() -> Result<()> {
    // Start mock server with a single complex page
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/docs/"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
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

    let mut crawler = SiteCrawler::new(database.pool().clone(), config);
    let stats = crawler.crawl_site(site.id, &base_url).await?;

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
