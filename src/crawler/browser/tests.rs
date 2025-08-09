use std::sync::atomic::{AtomicBool, Ordering};

use super::*;
use tempfile::NamedTempFile;
use tokio::time::Duration;

#[tokio::test]
async fn browser_config_validation() {
    let mut config = BrowserConfig::default();

    // Valid configuration should pass
    config.validate().expect("Default config should be valid");

    // Test invalid timeout
    config.navigation_timeout_seconds = 0;
    assert!(config.validate().is_err());
    config.navigation_timeout_seconds = 400;
    assert!(config.validate().is_err());
    config.navigation_timeout_seconds = 30; // Reset to valid

    // Test invalid browser pool size
    config.max_browsers = 0;
    assert!(config.validate().is_err());
    config.max_browsers = 20;
    assert!(config.validate().is_err());
    config.max_browsers = 2; // Reset to valid

    // Test invalid window dimensions
    config.window_width = 50;
    assert!(config.validate().is_err());
    config.window_height = 5000;
    assert!(config.validate().is_err());
}

#[tokio::test]
async fn browser_pool_cleanup() {
    let config = BrowserConfig {
        headless: true,
        max_browsers: 2,
        max_tabs_per_browser: 1,
        ..Default::default()
    };

    let pool = BrowserPool::new(config);

    // No browsers to clean up initially
    let cleaned = pool.cleanup_idle_browsers(Duration::from_secs(1));
    assert_eq!(cleaned, 0);
}

#[tokio::test]
async fn browser_pool_resource_limits() {
    let config = BrowserConfig {
        headless: true,
        max_browsers: 1,
        max_tabs_per_browser: 1,
        navigation_timeout_seconds: 2,
        ..Default::default()
    };

    let pool = BrowserPool::new(config);

    // The pool should limit concurrent operations
    let semaphore_permits = pool.semaphore.available_permits();
    assert_eq!(semaphore_permits, 1); // max_browsers * max_tabs_per_browser
}

#[tokio::test]
async fn rendered_page_structure() {
    let url = Url::parse("https://example.com").expect("is valid url");
    let rendered_page = RenderedPage {
        url: url.clone(),
        content: "<html><body>Test</body></html>".to_string(),
        title: "Test Page".to_string(),
        has_dynamic_content: false,
        render_time: Duration::from_millis(500),
    };

    assert_eq!(rendered_page.url, url);
    assert_eq!(rendered_page.title, "Test Page");
    assert!(!rendered_page.has_dynamic_content);
    assert_eq!(rendered_page.render_time, Duration::from_millis(500));
}

#[test]
fn browser_config_setters() {
    let mut config = BrowserConfig::default();

    // Note: enable_js_rendering is not available in this BrowserConfig,
    // it's in the config module's BrowserConfig

    config.set_max_browsers(3).expect("can set config property");
    assert_eq!(config.max_browsers, 3);

    assert!(config.set_max_browsers(0).is_err());
    assert!(config.set_max_browsers(20).is_err());

    config
        .set_navigation_timeout(60)
        .expect("can set config property");
    assert_eq!(config.navigation_timeout_seconds, 60);

    assert!(config.set_navigation_timeout(0).is_err());
    assert!(config.set_navigation_timeout(400).is_err());

    config
        .set_window_size(1920, 1080)
        .expect("can set config property");
    assert_eq!(config.window_width, 1920);
    assert_eq!(config.window_height, 1080);

    assert!(config.set_window_size(50, 50).is_err());
    assert!(config.set_window_size(5000, 5000).is_err());
}

mod integration_tests {
    use super::*;

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
                eprintln!("Render time: {:?}", rendered_page.render_time);
            }
            Err(e) => {
                // Skip test if Chrome is not available
                if e.to_string().contains("Chrome") || e.to_string().contains("browser") {
                    eprintln!("Skipping test - Chrome not available: {}", e);
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
                    eprintln!("Skipping test - Chrome not available: {}", e);
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
    }
}
