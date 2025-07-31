use super::*;
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
async fn browser_pool_creation_and_stats() {
    let config = BrowserConfig {
        max_browsers: 1,
        max_tabs_per_browser: 2,
        navigation_timeout_seconds: 10,
        headless: true,
        ..Default::default()
    };

    let pool = BrowserPool::new(config);
    let stats = pool.get_stats();

    assert_eq!(stats.total_browsers, 0);
    assert_eq!(stats.total_tabs, 0);
    assert_eq!(stats.max_browsers, 1);
    assert_eq!(stats.max_tabs_per_browser, 2);
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

#[test]
fn browser_pool_stats_default() {
    let stats = BrowserPoolStats::default();
    assert_eq!(stats.total_browsers, 0);
    assert_eq!(stats.total_tabs, 0);
    assert_eq!(stats.idle_browsers, 0);
    assert_eq!(stats.available_permits, 0);
}
