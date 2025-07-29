#![expect(
    clippy::tests_outside_test_module,
    reason = "integration tests are only compiled in test mode"
)]

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use docs_mcp::crawler::browser::{BrowserClient, BrowserConfig};
use tempfile::NamedTempFile;
use url::Url;

// Integration test that requires Chrome to be available
#[tokio::test]
async fn browser_page_rendering() {
    let config = BrowserConfig {
        headless: true,
        max_browsers: 1,
        max_tabs_per_browser: 1,
        navigation_timeout_seconds: 10,
        js_timeout_seconds: 5,
        ..Default::default()
    };

    let client = BrowserClient::new(config);

    // Create a simple HTML file to test with
    let html_content = r#"
    <!DOCTYPE html>
    <html>
    <head>
        <title>Test Page</title>
    </head>
    <body>
        <h1>Hello World</h1>
        <p>This is a test page</p>
        <script>
            document.body.innerHTML += '<p>JavaScript works!</p>';
        </script>
    </body>
    </html>
    "#;

    let temp_file = NamedTempFile::with_suffix(".html").expect("Failed to create temp file");
    std::fs::write(temp_file.path(), html_content).expect("Failed to write HTML");

    let file_url = format!("file://{}", temp_file.path().to_string_lossy());
    let url = Url::parse(&file_url).expect("Failed to parse file URL");

    match client.render_page(&url).await {
        Ok(rendered_page) => {
            assert_eq!(rendered_page.url, url);
            assert!(!rendered_page.content.is_empty());
            assert_eq!(rendered_page.title, "Test Page");
            assert!(rendered_page.content.contains("Hello World"));
            assert!(rendered_page.content.contains("JavaScript works!"));
            println!("Render time: {:?}", rendered_page.render_time);
        }
        Err(e) => {
            // Skip test if Chrome is not available
            if e.to_string().contains("Chrome") || e.to_string().contains("browser") {
                println!("Skipping test - Chrome not available: {}", e);
                return;
            }
            panic!("Unexpected error: {}", e);
        }
    }
}

#[tokio::test]
async fn browser_javascript_detection() {
    let config = BrowserConfig {
        headless: true,
        max_browsers: 1,
        max_tabs_per_browser: 1,
        navigation_timeout_seconds: 10,
        ..Default::default()
    };

    let client = BrowserClient::new(config);

    // Create an HTML file with React-like content
    let html_content = r#"
    <!DOCTYPE html>
    <html>
    <head>
        <title>React App</title>
    </head>
    <body>
        <div id="app" data-reactroot="">
            <p>Loading...</p>
        </div>
        <script src="https://unpkg.com/react@18/umd/react.development.js"></script>
        <script>
            // Simulate React app
            setTimeout(() => {
                document.getElementById('app').innerHTML = '<h1>React App Loaded</h1>';
            }, 100);
        </script>
    </body>
    </html>
    "#;

    let temp_file = NamedTempFile::with_suffix(".html").expect("Failed to create temp file");
    std::fs::write(temp_file.path(), html_content).expect("Failed to write HTML");

    let file_url = format!("file://{}", temp_file.path().to_string_lossy());
    let url = Url::parse(&file_url).expect("Failed to parse file URL");

    match client.render_page(&url).await {
        Ok(rendered_page) => {
            assert!(rendered_page.has_dynamic_content);
            assert!(rendered_page.content.contains("data-reactroot"));
        }
        Err(e) => {
            if e.to_string().contains("Chrome") || e.to_string().contains("browser") {
                println!("Skipping test - Chrome not available: {}", e);
                return;
            }
            panic!("Unexpected error: {}", e);
        }
    }
}

#[tokio::test]
async fn browser_concurrent_requests() {
    let config = BrowserConfig {
        headless: true,
        max_browsers: 2,
        max_tabs_per_browser: 2,
        navigation_timeout_seconds: 10,
        ..Default::default()
    };

    let client = Arc::new(BrowserClient::new(config));

    // Create multiple HTML files
    let html_content = r#"
    <!DOCTYPE html>
    <html>
    <head><title>Test {{INDEX}}</title></head>
    <body>
        <h1>Page {{INDEX}}</h1>
        <script>
            document.body.innerHTML += '<p>Script executed on page {{INDEX}}</p>';
        </script>
    </body>
    </html>
    "#;

    let mut tasks = Vec::new();
    let success_count = Arc::new(AtomicBool::new(true));

    for i in 0..4 {
        let client_clone = Arc::clone(&client);
        let success_clone = Arc::clone(&success_count);
        let content = html_content.replace("{{INDEX}}", &i.to_string());

        let task = tokio::spawn(async move {
            let temp_file = NamedTempFile::new().expect("Failed to create temp file");
            std::fs::write(temp_file.path(), content).expect("Failed to write HTML");

            let file_url = format!("file://{}", temp_file.path().to_string_lossy());
            let url = Url::parse(&file_url).expect("Failed to parse file URL");

            match client_clone.render_page(&url).await {
                Ok(rendered_page) => {
                    assert!(rendered_page.content.contains(&format!("Page {}", i)));
                    assert!(
                        rendered_page
                            .content
                            .contains(&format!("Script executed on page {}", i))
                    );
                }
                Err(e) => {
                    if !e.to_string().contains("Chrome") && !e.to_string().contains("browser") {
                        success_clone.store(false, Ordering::Relaxed);
                        panic!("Unexpected error in concurrent test: {}", e);
                    }
                }
            }
        });

        tasks.push(task);
    }

    // Wait for all tasks to complete
    for task in tasks {
        task.await.expect("Task panicked");
    }

    if success_count.load(Ordering::Relaxed) {
        let stats = client.get_pool_stats();
        println!("Final pool stats: {:?}", stats);
    }
}
