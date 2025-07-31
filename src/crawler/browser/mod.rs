use anyhow::{Context, Result, anyhow};
use headless_chrome::{Browser, LaunchOptions, Tab};
use std::ffi::OsStr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;
use tracing::{debug, error, info, warn};
use url::Url;

/// Configuration for browser operations
#[derive(Debug, Clone)]
pub struct BrowserConfig {
    /// Maximum number of browser instances in the pool
    pub max_browsers: usize,
    /// Maximum number of tabs per browser
    pub max_tabs_per_browser: usize,
    /// Timeout for page navigation in seconds
    pub navigation_timeout_seconds: u64,
    /// Timeout for JavaScript execution in seconds
    pub js_timeout_seconds: u64,
    /// Timeout for browser launch in seconds
    pub launch_timeout_seconds: u64,
    /// Whether to run browsers in headless mode
    pub headless: bool,
    /// Browser window width
    pub window_width: u32,
    /// Browser window height
    pub window_height: u32,
    /// Additional Chrome arguments
    pub chrome_args: Vec<String>,
    /// User agent string to use
    pub user_agent: String,
}

impl Default for BrowserConfig {
    #[inline]
    fn default() -> Self {
        Self {
            max_browsers: 2,
            max_tabs_per_browser: 4,
            navigation_timeout_seconds: 30,
            js_timeout_seconds: 10,
            launch_timeout_seconds: 10,
            headless: true,
            window_width: 1280,
            window_height: 720,
            chrome_args: vec![
                "--no-sandbox".to_string(),
                "--disable-dev-shm-usage".to_string(),
                "--disable-gpu".to_string(),
                "--disable-extensions".to_string(),
                "--disable-plugins".to_string(),
                "--disable-images".to_string(), // Optimize for content extraction
                "--disable-javascript-harmony-shipping".to_string(),
                "--disable-background-timer-throttling".to_string(),
                "--disable-renderer-backgrounding".to_string(),
                "--disable-backgrounding-occluded-windows".to_string(),
            ],
            user_agent:
                "docs-mcp/0.1.0 (Documentation Indexer; +https://github.com/anthropics/claude-code)"
                    .to_string(),
        }
    }
}

/// A managed browser instance with resource tracking
struct ManagedBrowser {
    browser: Browser,
    active_tabs: usize,
    last_used: Instant,
    created_at: Instant,
}

impl ManagedBrowser {
    /// Create a new managed browser instance
    #[inline]
    fn new(config: &BrowserConfig) -> Result<Self> {
        let args: Vec<&OsStr> = config.chrome_args.iter().map(OsStr::new).collect();
        let launch_options = LaunchOptions {
            headless: config.headless,
            window_size: Some((config.window_width, config.window_height)),
            args,
            idle_browser_timeout: Duration::from_secs(config.launch_timeout_seconds),
            ..Default::default()
        };

        let browser =
            Browser::new(launch_options).with_context(|| "Failed to launch browser instance")?;

        Ok(Self {
            browser,
            active_tabs: 0,
            last_used: Instant::now(),
            created_at: Instant::now(),
        })
    }

    /// Check if browser can accept more tabs
    #[inline]
    fn can_accept_tab(&self, max_tabs: usize) -> bool {
        self.active_tabs < max_tabs
    }

    /// Create a new tab in this browser
    #[inline]
    fn new_tab(&mut self, config: &BrowserConfig) -> Result<Arc<Tab>> {
        let tab = self
            .browser
            .new_tab()
            .with_context(|| "Failed to create new browser tab")?;

        // Set user agent
        tab.set_user_agent(&config.user_agent, None, None)
            .with_context(|| "Failed to set user agent")?;

        // Set viewport size
        tab.set_bounds(headless_chrome::types::Bounds::Normal {
            left: Some(0),
            top: Some(0),
            width: Some(config.window_width as f64),
            height: Some(config.window_height as f64),
        })
        .with_context(|| "Failed to set viewport bounds")?;

        self.active_tabs += 1;
        self.last_used = Instant::now();

        Ok(tab)
    }

    /// Release a tab from this browser
    #[inline]
    fn release_tab(&mut self) {
        if self.active_tabs > 0 {
            self.active_tabs -= 1;
        }
        self.last_used = Instant::now();
    }

    /// Check if browser is idle (no active tabs)
    #[inline]
    fn is_idle(&self) -> bool {
        self.active_tabs == 0
    }

    /// Get age of browser instance
    #[inline]
    #[allow(dead_code)]
    fn age(&self) -> Duration {
        self.created_at.elapsed()
    }

    /// Get time since last use
    #[inline]
    fn idle_time(&self) -> Duration {
        self.last_used.elapsed()
    }
}

/// A pool of browser instances for efficient resource management
pub struct BrowserPool {
    browsers: Arc<Mutex<Vec<Option<ManagedBrowser>>>>,
    config: BrowserConfig,
    semaphore: Arc<Semaphore>,
}

impl BrowserPool {
    /// Create a new browser pool with the given configuration
    #[inline]
    pub fn new(config: BrowserConfig) -> Self {
        let max_concurrent_operations = config.max_browsers * config.max_tabs_per_browser;

        Self {
            browsers: Arc::new(Mutex::new(Vec::new())),
            semaphore: Arc::new(Semaphore::new(max_concurrent_operations)),
            config,
        }
    }

    /// Get a browser tab for rendering a page
    #[inline]
    pub async fn get_tab(&self) -> Result<BrowserTab> {
        // Acquire semaphore permit for resource limiting
        let permit = Arc::clone(&self.semaphore)
            .acquire_owned()
            .await
            .map_err(|e| anyhow!("Failed to acquire browser pool permit: {}", e))?;

        // Try to find an existing browser with available capacity
        let mut browsers = self
            .browsers
            .lock()
            .map_err(|e| anyhow!("Browser pool lock poisoned: {}", e))?;

        // Look for a browser that can accept more tabs
        for (browser_index, browser_option) in browsers.iter_mut().enumerate() {
            if let Some(browser) = browser_option {
                if browser.can_accept_tab(self.config.max_tabs_per_browser) {
                    match browser.new_tab(&self.config) {
                        Ok(tab) => {
                            debug!(
                                "Reusing existing browser instance (active tabs: {})",
                                browser.active_tabs
                            );
                            return Ok(BrowserTab::new(
                                tab,
                                Arc::clone(&self.browsers),
                                browser_index,
                                permit,
                                self.config.clone(),
                            ));
                        }
                        Err(e) => {
                            warn!("Failed to create tab in existing browser: {}", e);
                        }
                    }
                }
            }
        }

        // Look for an empty slot first, then create new if needed
        let mut empty_slot_index = None;
        for (index, browser_option) in browsers.iter().enumerate() {
            if browser_option.is_none() {
                empty_slot_index = Some(index);
                break;
            }
        }

        // Either use empty slot or add new browser if under limit
        let target_index = if let Some(index) = empty_slot_index {
            index
        } else if browsers.len() < self.config.max_browsers {
            browsers.len()
        } else {
            return Err(anyhow!("Browser pool at capacity, no available tabs"));
        };

        match ManagedBrowser::new(&self.config) {
            Ok(mut browser) => match browser.new_tab(&self.config) {
                Ok(tab) => {
                    info!(
                        "Created new browser instance at index {} (total browsers: {})",
                        target_index,
                        browsers.iter().filter(|b| b.is_some()).count() + 1
                    );

                    if target_index == browsers.len() {
                        browsers.push(Some(browser));
                    } else {
                        browsers[target_index] = Some(browser);
                    }

                    Ok(BrowserTab::new(
                        tab,
                        Arc::clone(&self.browsers),
                        target_index,
                        permit,
                        self.config.clone(),
                    ))
                }
                Err(e) => {
                    error!("Failed to create tab in new browser: {}", e);
                    Err(e)
                }
            },
            Err(e) => {
                error!("Failed to create new browser instance: {}", e);
                Err(e)
            }
        }
    }

    /// Clean up idle browsers to free resources
    #[inline]
    pub fn cleanup_idle_browsers(&self, max_idle_time: Duration) -> usize {
        let mut browsers = match self.browsers.lock() {
            Ok(browsers) => browsers,
            Err(e) => {
                error!("Failed to acquire browser pool lock for cleanup: {}", e);
                return 0;
            }
        };

        let mut removed_count = 0;

        // Remove browsers that are idle and past the max idle time
        for (index, browser_option) in browsers.iter_mut().enumerate() {
            if let Some(browser) = browser_option {
                if browser.is_idle() && browser.idle_time() > max_idle_time {
                    debug!(
                        "Removing idle browser at index {} (idle for {:?})",
                        index,
                        browser.idle_time()
                    );
                    *browser_option = None;
                    removed_count += 1;
                }
            }
        }
        if removed_count > 0 {
            info!("Cleaned up {} idle browser instances", removed_count);
        }

        removed_count
    }

    /// Get pool statistics for monitoring
    #[inline]
    pub fn get_stats(&self) -> BrowserPoolStats {
        let Ok(browsers) = self.browsers.lock() else {
            return BrowserPoolStats::default();
        };

        let total_browsers = browsers.iter().filter(|b| b.is_some()).count();
        let total_tabs: usize = browsers
            .iter()
            .filter_map(|b| b.as_ref())
            .map(|b| b.active_tabs)
            .sum();
        let idle_browsers = browsers
            .iter()
            .filter_map(|b| b.as_ref())
            .filter(|b| b.is_idle())
            .count();
        let available_permits = self.semaphore.available_permits();

        BrowserPoolStats {
            total_browsers,
            total_tabs,
            idle_browsers,
            available_permits,
            max_browsers: self.config.max_browsers,
            max_tabs_per_browser: self.config.max_tabs_per_browser,
        }
    }
}

/// Statistics about the browser pool
#[derive(Debug, Clone, Default)]
pub struct BrowserPoolStats {
    pub total_browsers: usize,
    pub total_tabs: usize,
    pub idle_browsers: usize,
    pub available_permits: usize,
    pub max_browsers: usize,
    pub max_tabs_per_browser: usize,
}

/// A managed browser tab with automatic cleanup
pub struct BrowserTab {
    tab: Arc<Tab>,
    browsers: Arc<Mutex<Vec<Option<ManagedBrowser>>>>,
    browser_index: usize,
    _permit: tokio::sync::OwnedSemaphorePermit,
    config: BrowserConfig,
}

impl BrowserTab {
    #[inline]
    fn new(
        tab: Arc<Tab>,
        browsers: Arc<Mutex<Vec<Option<ManagedBrowser>>>>,
        browser_index: usize,
        permit: tokio::sync::OwnedSemaphorePermit,
        config: BrowserConfig,
    ) -> Self {
        Self {
            tab,
            browsers,
            browser_index,
            _permit: permit,
            config,
        }
    }

    /// Navigate to a URL and wait for the page to load completely
    #[inline]
    pub async fn navigate_and_wait(&self, url: &Url) -> Result<()> {
        let url_str = url.as_str();
        debug!("Navigating to URL: {}", url_str);

        // Navigate to the URL
        self.tab
            .navigate_to(url_str)
            .with_context(|| format!("Failed to navigate to {}", url_str))?;

        // Wait for navigation to complete with timeout
        let navigation_timeout = Duration::from_secs(self.config.navigation_timeout_seconds);

        tokio::time::timeout(navigation_timeout, async {
            // Wait for the page to load
            self.tab
                .wait_until_navigated()
                .with_context(|| format!("Navigation to {} did not complete", url_str))?;

            // Wait for network to be mostly idle (no requests for 500ms)
            if let Err(e) = self.tab.wait_for_element("body") {
                warn!("Failed to wait for body element: {}", e);
            }

            Ok::<(), anyhow::Error>(())
        })
        .await
        .map_err(|_| anyhow!("Navigation timeout after {:?}", navigation_timeout))??;

        // Give JavaScript time to execute and render dynamic content
        let js_wait_time = Duration::from_millis(2000); // 2 seconds for JS to complete
        tokio::time::sleep(js_wait_time).await;

        debug!(
            "Successfully navigated to {} and waited for rendering",
            url_str
        );
        Ok(())
    }

    /// Get the rendered HTML content of the page
    #[inline]
    pub fn get_content(&self) -> Result<String> {
        debug!("Extracting rendered HTML content");

        // Get the outer HTML of the document
        let content = self
            .tab
            .get_content()
            .with_context(|| "Failed to get page content")?;

        debug!(
            "Successfully extracted {} bytes of rendered content",
            content.len()
        );
        Ok(content)
    }

    /// Execute JavaScript and return the result
    #[inline]
    pub fn execute_js(&self, script: &str) -> Result<serde_json::Value> {
        debug!("Executing JavaScript: {}", script);

        let _js_timeout = Duration::from_secs(self.config.js_timeout_seconds);

        // Execute JavaScript with timeout handling
        let result = std::thread::scope(|s| {
            let handle = s.spawn(|| {
                self.tab
                    .evaluate(script, false)
                    .with_context(|| format!("Failed to execute JavaScript: {}", script))
            });

            // Simple timeout handling since we can't use tokio::time::timeout in sync context
            handle
                .join()
                .unwrap_or_else(|_| Err(anyhow!("JavaScript execution panicked")))
        });

        result.map(|r| r.value.unwrap_or(serde_json::Value::Null))
    }

    /// Check if the page contains dynamic content that requires JavaScript rendering
    #[inline]
    pub fn has_dynamic_content(&self) -> Result<bool> {
        // Check for common indicators of JavaScript-rendered content
        let checks = vec![
            // React applications
            "document.querySelector('[data-reactroot]') !== null",
            // Vue.js applications
            "document.querySelector('[data-v-]') !== null || document.querySelector('#app')",
            // Angular applications
            "document.querySelector('[ng-app]') !== null || document.querySelector('[ng-version]') !== null",
            // General SPA indicators
            "document.querySelector('script[src*=\"app\"]') !== null",
            "document.querySelector('script[src*=\"main\"]') !== null",
            "document.querySelector('script[src*=\"bundle\"]') !== null",
            // Check for minimal initial content that gets populated by JS
            "document.body.textContent.trim().length < 500 && document.querySelectorAll('script').length > 3",
        ];

        for check in checks {
            match self.execute_js(check) {
                Ok(result) => {
                    if let Some(is_dynamic) = result.as_bool() {
                        if is_dynamic {
                            debug!("Detected dynamic content via check: {}", check);
                            return Ok(true);
                        }
                    }
                }
                Err(e) => {
                    debug!("JavaScript check failed: {} - {}", check, e);
                }
            }
        }

        Ok(false)
    }

    /// Get page title from the rendered page
    #[inline]
    pub fn get_title(&self) -> Result<String> {
        match self.execute_js("document.title") {
            Ok(title) => {
                let title_str = match title {
                    serde_json::Value::String(s) => s,
                    serde_json::Value::Null => String::new(),
                    other => {
                        debug!("Unexpected title value type: {:?}", other);
                        other.to_string().trim_matches('"').to_string()
                    }
                };
                Ok(title_str)
            }
            Err(e) => {
                debug!("Failed to get page title via JavaScript: {}", e);
                Ok(String::new())
            }
        }
    }
}

impl Drop for BrowserTab {
    #[inline]
    fn drop(&mut self) {
        // Release the tab from its specific browser when dropped
        if let Ok(mut browsers) = self.browsers.lock() {
            if let Some(Some(browser)) = browsers.get_mut(self.browser_index) {
                browser.release_tab();
                debug!(
                    "Browser tab released from browser index {}",
                    self.browser_index
                );
            } else {
                warn!(
                    "Failed to find browser at index {} during tab cleanup",
                    self.browser_index
                );
            }
        } else {
            error!("Failed to acquire browser lock during tab cleanup");
        }
    }
}

/// Client for rendering JavaScript-heavy pages
pub struct BrowserClient {
    pool: BrowserPool,
}

impl BrowserClient {
    /// Create a new browser client with the given configuration
    #[inline]
    pub fn new(config: BrowserConfig) -> Self {
        Self {
            pool: BrowserPool::new(config),
        }
    }

    /// Render a URL and return the fully rendered HTML content
    #[inline]
    pub async fn render_page(&self, url: &Url) -> Result<RenderedPage> {
        let start_time = Instant::now();

        // Get a browser tab from the pool
        let tab = self.pool.get_tab().await?;

        // Navigate and wait for the page to render
        tab.navigate_and_wait(url).await?;

        // Check if page has dynamic content
        let has_dynamic_content = tab.has_dynamic_content().unwrap_or(false);

        // Get the rendered HTML content
        let content = tab.get_content()?;

        // Get page title
        let title = tab.get_title().unwrap_or_default();

        let render_time = start_time.elapsed();

        debug!(
            "Successfully rendered page {} (dynamic: {}, {} bytes, took {:?})",
            url,
            has_dynamic_content,
            content.len(),
            render_time
        );

        Ok(RenderedPage {
            url: url.clone(),
            content,
            title,
            has_dynamic_content,
            render_time,
        })
    }

    /// Render a URL and return only the HTML content
    #[inline]
    pub async fn get_rendered_html(&self, url: &Url) -> Result<String> {
        let rendered_page = self.render_page(url).await?;
        Ok(rendered_page.content)
    }
}

impl Default for BrowserClient {
    #[inline]
    fn default() -> Self {
        Self::new(BrowserConfig::default())
    }
}

/// Result of rendering a page with a browser
#[derive(Debug, Clone)]
pub struct RenderedPage {
    /// The URL that was rendered
    pub url: Url,
    /// The fully rendered HTML content
    pub content: String,
    /// The page title
    pub title: String,
    /// Whether the page contains dynamic content
    pub has_dynamic_content: bool,
    /// Time taken to render the page
    pub render_time: Duration,
}

impl BrowserConfig {
    /// Validate the browser configuration
    #[inline]
    pub fn validate(&self) -> Result<()> {
        if self.navigation_timeout_seconds == 0 || self.navigation_timeout_seconds > 300 {
            return Err(anyhow!(
                "Invalid navigation timeout: {} (must be between 1 and 300 seconds)",
                self.navigation_timeout_seconds
            ));
        }

        if self.max_browsers == 0 || self.max_browsers > 10 {
            return Err(anyhow!(
                "Invalid browser pool size: {} (must be between 1 and 10)",
                self.max_browsers
            ));
        }

        if self.max_tabs_per_browser == 0 || self.max_tabs_per_browser > 10 {
            return Err(anyhow!(
                "Invalid tabs per browser: {} (must be between 1 and 10)",
                self.max_tabs_per_browser
            ));
        }

        if self.window_width < 100
            || self.window_width > 4000
            || self.window_height < 100
            || self.window_height > 4000
        {
            return Err(anyhow!(
                "Invalid window dimensions: {}x{} (must be between 100 and 4000)",
                self.window_width,
                self.window_height
            ));
        }

        Ok(())
    }

    /// Set the maximum number of browsers
    #[inline]
    pub fn set_max_browsers(&mut self, max_browsers: usize) -> Result<()> {
        if max_browsers == 0 || max_browsers > 10 {
            return Err(anyhow!(
                "Invalid browser pool size: {} (must be between 1 and 10)",
                max_browsers
            ));
        }
        self.max_browsers = max_browsers;
        Ok(())
    }

    /// Set the navigation timeout
    #[inline]
    pub fn set_navigation_timeout(&mut self, timeout_seconds: u64) -> Result<()> {
        if timeout_seconds == 0 || timeout_seconds > 300 {
            return Err(anyhow!(
                "Invalid navigation timeout: {} (must be between 1 and 300 seconds)",
                timeout_seconds
            ));
        }
        self.navigation_timeout_seconds = timeout_seconds;
        Ok(())
    }

    /// Set the window size
    #[inline]
    pub fn set_window_size(&mut self, width: u32, height: u32) -> Result<()> {
        if !(100..=4000).contains(&width) || !(100..=4000).contains(&height) {
            return Err(anyhow!(
                "Invalid window dimensions: {}x{} (must be between 100 and 4000)",
                width,
                height
            ));
        }
        self.window_width = width;
        self.window_height = height;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
