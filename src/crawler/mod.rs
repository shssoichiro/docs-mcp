pub mod browser;
pub mod extractor;
pub mod robots;

#[cfg(test)]
mod tests;

use anyhow::{Context, Result, anyhow, bail};
use indicatif::{ProgressBar, ProgressStyle};
use scraper::{Html, Selector};
use std::borrow::Cow;
use std::collections::HashSet;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};
use ureq::Agent;
use url::Url;

use self::browser::{BrowserClient, BrowserConfig};
use self::extractor::{ExtractionConfig, extract_content};
use self::robots::{RobotsTxt, fetch_robots_txt};
use crate::database::sqlite::{
    CrawlQueueQueries, CrawlQueueUpdate, CrawlStatus, DbPool, NewCrawlQueueItem, SiteQueries,
    SiteStatus, SiteUpdate,
};

/// Configuration for the web crawler
#[derive(Debug, Clone)]
pub struct CrawlerConfig {
    /// User agent string to use for requests
    pub user_agent: String,
    /// Timeout for HTTP requests in seconds
    pub timeout_seconds: u64,
    /// Rate limit delay between requests in milliseconds
    pub rate_limit_ms: u64,
    /// Maximum number of retry attempts for retryable errors
    pub max_retries: u32,
    /// Delay between retry attempts in seconds
    pub retry_delay_seconds: u64,
    /// Whether to enable JavaScript rendering for dynamic content
    pub enable_js_rendering: bool,
    /// Configuration for browser-based rendering
    pub browser_config: BrowserConfig,
}

impl Default for CrawlerConfig {
    #[inline]
    fn default() -> Self {
        Self {
            user_agent: "docs-mcp/0.1.0 (Documentation Indexer)".to_string(),
            timeout_seconds: 30,
            rate_limit_ms: 250,
            max_retries: 3,
            retry_delay_seconds: 30,
            enable_js_rendering: true,
            browser_config: BrowserConfig::default(),
        }
    }
}

/// HTTP client wrapper with rate limiting and retry logic
#[derive(Debug)]
pub struct HttpClient {
    agent: Agent,
    config: CrawlerConfig,
    last_request_time: Option<Instant>,
}

impl HttpClient {
    /// Create a new HTTP client with the given configuration
    #[inline]
    pub fn new(config: CrawlerConfig) -> Self {
        let agent = Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(config.timeout_seconds)))
            .user_agent(&config.user_agent)
            .build()
            .into();

        Self {
            agent,
            config,
            last_request_time: None,
        }
    }

    /// Perform an HTTP GET request with rate limiting and retry logic
    #[inline]
    pub async fn get(&mut self, url: &str) -> Result<String> {
        self.apply_rate_limit().await;

        let mut last_error = None;

        for attempt in 0..=self.config.max_retries {
            if attempt > 0 {
                debug!("Retrying request to {} (attempt {})", url, attempt + 1);
                tokio::time::sleep(Duration::from_secs(self.config.retry_delay_seconds)).await;
            }

            match self.try_get(url) {
                Ok(response) => {
                    debug!("Successfully fetched {} (attempt {})", url, attempt + 1);
                    return Ok(response);
                }
                Err(e) if is_retryable_error(&e) && attempt < self.config.max_retries => {
                    warn!("Retryable error for {}: {}", url, e);
                    last_error = Some(e);
                }
                Err(e) => {
                    error!("Non-retryable error for {}: {}", url, e);
                    return Err(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow!("All retry attempts failed")))
    }

    /// Apply rate limiting by sleeping if necessary
    async fn apply_rate_limit(&mut self) {
        if let Some(last_time) = self.last_request_time {
            let elapsed = last_time.elapsed();
            let rate_limit_duration = Duration::from_millis(self.config.rate_limit_ms);

            if elapsed < rate_limit_duration {
                let sleep_duration = rate_limit_duration - elapsed;
                debug!("Rate limiting: sleeping for {:?}", sleep_duration);
                sleep(sleep_duration).await;
            }
        }

        self.last_request_time = Some(Instant::now());
    }

    /// Attempt a single HTTP GET request without retry logic
    fn try_get(&self, url: &str) -> Result<String> {
        debug!("Making HTTP GET request to: {}", url);

        match self.agent.get(url).call() {
            Ok(mut response) => {
                let text = response
                    .body_mut()
                    .read_to_string()
                    .with_context(|| format!("Failed to read response body from {}", url))?;
                debug!("Successfully read {} bytes from {}", text.len(), url);
                Ok(text)
            }
            Err(ureq::Error::StatusCode(code)) => {
                let status = code;
                debug!("HTTP request failed with status {}: {}", status, url);
                Err(anyhow!("HTTP error {}", status))
            }
            Err(e) => {
                debug!("HTTP request failed with transport error: {}", e);
                Err(anyhow::Error::from(e))
                    .with_context(|| format!("Failed to make HTTP request to {}", url))
            }
        }
    }
}

impl Default for HttpClient {
    /// Create a new HTTP client with default configuration
    #[inline]
    fn default() -> Self {
        Self::new(CrawlerConfig::default())
    }
}

/// Check if an error is retryable (network timeouts, 5xx errors)
fn is_retryable_error(error: &anyhow::Error) -> bool {
    let error_str = error.to_string().to_lowercase();

    // Network timeouts and connection errors
    if error_str.contains("timeout")
        || error_str.contains("connection")
        || error_str.contains("network")
    {
        return true;
    }

    // HTTP 5xx server errors are retryable
    if error_str.contains("http error 5") {
        return true;
    }

    // HTTP 429 (rate limiting) is retryable
    if error_str.contains("http error 429") {
        return true;
    }

    false
}

/// Validate and normalize a URL
#[inline]
pub fn validate_url(url_str: &str) -> Result<Url> {
    let url = Url::parse(url_str).with_context(|| format!("Invalid URL format: {}", url_str))?;

    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(anyhow!("URL must use HTTP or HTTPS scheme: {}", url_str));
    }

    if url.host_str().is_none() {
        return Err(anyhow!("URL must have a valid host: {}", url_str));
    }

    Ok(url)
}

/// Check if a URL should be crawled based on base URL filtering rules
#[inline]
pub fn should_crawl_url(url: &Url, base_url: &Url) -> bool {
    // Must be same scheme and host
    if url.scheme() != base_url.scheme() || url.host() != base_url.host() {
        return false;
    }

    // Must start with the base URL path (excluding trailing filename)
    let base_path = normalize_path_for_filtering(base_url.path());
    let url_path = url.path();

    url_path.starts_with(base_path.as_ref())
}

/// Normalize a URL path for filtering by removing trailing filename if present
fn normalize_path_for_filtering(path: &str) -> Cow<'_, str> {
    if path.ends_with('/') {
        Cow::Borrowed(path)
    } else {
        // Check if the last segment looks like a filename (contains a dot)
        path.rfind('/').map_or_else(
            || Cow::Owned(format!("{}/", path)),
            #[expect(clippy::string_slice, reason = "we know the split point is one byte")]
            |last_slash| {
                let last_segment = &path[last_slash + 1..];
                if last_segment.contains('.') && !last_segment.ends_with('/') {
                    // Looks like a filename, use the directory path
                    Cow::Borrowed(&path[..=last_slash])
                } else {
                    // Not a filename, add trailing slash
                    Cow::Owned(format!("{}/", path))
                }
            },
        )
    }
}

/// Extract all links from HTML content using proper HTML parsing
#[inline]
pub fn extract_links(html: &str, source_url: &Url, base_url: &Url) -> Result<Vec<Url>> {
    let document = Html::parse_document(html);
    let link_selector = Selector::parse("a[href]")
        .map_err(|e| anyhow!("Failed to create CSS selector: {:?}", e))?;

    let mut links = Vec::new();

    for element in document.select(&link_selector) {
        if let Some(href) = element.value().attr("href") {
            // Skip non-HTTP(S) links
            if href.starts_with("mailto:")
                || href.starts_with("javascript:")
                || href.starts_with("#")
                || href.starts_with("\\#")
            {
                continue;
            }

            match source_url.join(href) {
                Ok(absolute_url) => {
                    if should_crawl_url(&absolute_url, base_url) {
                        links.push(absolute_url);
                    }
                }
                Err(e) => {
                    debug!(
                        "Failed to resolve URL '{}' relative to '{}': {}",
                        href, base_url, e
                    );
                }
            }
        }
    }

    // Remove duplicates
    links.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    links.dedup();

    info!("Extracted {} valid links from {}", links.len(), source_url);
    Ok(links)
}

/// Main crawler that coordinates the crawling process
pub struct SiteCrawler {
    http_client: HttpClient,
    browser_client: Option<BrowserClient>,
    db_pool: DbPool,
    config: CrawlerConfig,
    extraction_config: ExtractionConfig,
}

/// Result of crawling a single page
#[derive(Debug, Clone)]
pub struct CrawlResult {
    /// The URL that was crawled
    pub url: Url,
    /// The extracted content from the page
    pub content: extractor::ExtractedContent,
    /// Links found on this page
    pub links: Vec<Url>,
    /// Whether this page was successfully processed
    pub success: bool,
    /// Error message if crawling failed
    pub error_message: Option<String>,
}

/// Statistics about a crawl session
#[derive(Debug, Clone, Copy)]
pub struct CrawlStats {
    /// Total URLs discovered
    pub total_urls: usize,
    /// URLs successfully crawled
    pub successful_crawls: usize,
    /// URLs that failed to crawl
    pub failed_crawls: usize,
    /// URLs skipped due to robots.txt
    pub robots_blocked: usize,
    /// Duration of crawl session
    pub duration: Duration,
}

impl CrawlStats {
    #[inline]
    pub fn total_crawled(&self) -> usize {
        self.successful_crawls + self.failed_crawls + self.robots_blocked
    }
}

impl SiteCrawler {
    /// Create a new site crawler
    #[inline]
    pub fn new(db_pool: DbPool, config: CrawlerConfig) -> Self {
        let http_client = HttpClient::new(config.clone());
        let extraction_config = ExtractionConfig::default();

        // Initialize browser client if JavaScript rendering is enabled
        let browser_client = if config.enable_js_rendering {
            info!("JavaScript rendering enabled with browser client");
            Some(BrowserClient::new(config.browser_config.clone()))
        } else {
            debug!("JavaScript rendering disabled");
            None
        };

        Self {
            http_client,
            browser_client,
            db_pool,
            config,
            extraction_config,
        }
    }

    /// Crawl a documentation site from the given base URL
    #[inline]
    pub async fn crawl_site(
        &mut self,
        site_id: i64,
        index_url: &str,
        base_url: &str,
    ) -> Result<CrawlStats> {
        // TODO: Implement per-site multiple instance checking

        let start_time = Instant::now();
        let index_url = validate_url(index_url)?;
        let base_url = validate_url(base_url)?;

        info!("Starting crawl for site {} at {}", site_id, index_url);

        // Update site status to indexing
        self.update_site_status(site_id, SiteStatus::Indexing, None)
            .await?;

        // Fetch robots.txt
        let robots_txt = match fetch_robots_txt(&mut self.http_client, &base_url).await {
            Ok(robots) => {
                debug!("Successfully loaded robots.txt for {}", base_url);
                robots
            }
            Err(e) => {
                warn!("Failed to fetch robots.txt for {}: {}", base_url, e);
                RobotsTxt::parse("") // Allow all if robots.txt is unavailable
            }
        };

        // Initialize crawl queue with base URL
        self.init_crawl_queue(site_id, &index_url).await?;

        // Track discovered URLs to avoid duplicates
        let mut discovered_urls = HashSet::new();
        discovered_urls.insert(index_url.as_str().to_string());

        let mut stats = CrawlStats {
            total_urls: 1,
            successful_crawls: 0,
            failed_crawls: 0,
            robots_blocked: 0,
            duration: Duration::default(),
        };

        let bar = if console::user_attended_stderr() {
            ProgressBar::new_spinner().with_style(
                ProgressStyle::with_template("{spinner} [{pos}/{len}] Crawling {msg}")
                    .expect("style template is valid"),
            )
        } else {
            ProgressBar::hidden()
        };
        bar.set_position(0);
        bar.set_length(1);

        // Main crawling loop - breadth-first approach
        loop {
            // Get next URL from queue
            let Some(queue_item) = self.get_next_queue_item(site_id).await? else {
                info!("No more URLs in queue, crawl complete");
                break;
            };

            // Mark item as processing
            self.update_queue_item_status(queue_item.id, CrawlStatus::Processing, None)
                .await?;

            let url = match validate_url(&queue_item.url) {
                Ok(url) => url,
                Err(e) => {
                    error!("Invalid URL in queue: {}: {}", queue_item.url, e);

                    // Set retry count to max to prevent retrying invalid URLs
                    let update = CrawlQueueUpdate {
                        status: Some(CrawlStatus::Failed),
                        retry_count: Some(self.config.max_retries.into()),
                        error_message: Some(format!("Invalid URL: {}", e)),
                    };
                    CrawlQueueQueries::update(&self.db_pool, queue_item.id, update).await?;

                    stats.failed_crawls += 1;
                    bar.set_position(stats.total_crawled() as u64);
                    continue;
                }
            };

            // Check robots.txt
            if !robots_txt.is_allowed(&url, &self.config.user_agent) {
                info!("URL blocked by robots.txt: {}", url);

                // Set retry count to max to prevent it from being retried
                let update = CrawlQueueUpdate {
                    status: Some(CrawlStatus::Failed),
                    retry_count: Some(self.config.max_retries.into()),
                    error_message: Some("Blocked by robots.txt".to_string()),
                };
                CrawlQueueQueries::update(&self.db_pool, queue_item.id, update).await?;

                stats.robots_blocked += 1;
                bar.set_position(stats.total_crawled() as u64);
                continue;
            }

            // Crawl the page
            bar.set_message(url.to_string());
            match self.crawl_page(&url, &base_url).await {
                Ok(crawl_result) => {
                    if crawl_result.success {
                        info!("Successfully crawled: {}", url);
                        stats.successful_crawls += 1;
                        bar.set_position(stats.total_crawled() as u64);

                        // Mark queue item as completed
                        self.update_queue_item_status(queue_item.id, CrawlStatus::Completed, None)
                            .await?;

                        // Add newly discovered URLs to the queue
                        for mut link in crawl_result.links.iter().cloned() {
                            link.set_fragment(None);
                            let link_str = link.as_str();
                            if !discovered_urls.contains(link_str) {
                                discovered_urls.insert(link_str.to_string());

                                // Add to database queue
                                match self.add_url_to_queue(site_id, link_str).await {
                                    Ok(_) => {
                                        debug!("Added URL to queue: {}", link_str);
                                        stats.total_urls += 1;
                                        bar.set_length(stats.total_urls as u64);
                                    }
                                    Err(e) => {
                                        // URL might already exist - that's okay
                                        debug!(
                                            "Could not add URL to queue (likely duplicate): {}",
                                            e
                                        );
                                    }
                                }
                            }
                        }

                        // Update site progress
                        self.update_site_progress(site_id).await?;
                    } else {
                        let error_msg = crawl_result.error_message.clone().unwrap_or_default();
                        error!("Failed to crawl: {} - {}", url, error_msg);
                        stats.failed_crawls += 1;
                        bar.set_position(stats.total_crawled() as u64);

                        // Mark queue item as failed
                        self.update_queue_item_status(
                            queue_item.id,
                            CrawlStatus::Failed,
                            crawl_result.error_message,
                        )
                        .await?;
                    }
                }
                Err(e) => {
                    error!("Error crawling {}: {}", url, e);
                    stats.failed_crawls += 1;
                    bar.set_position(stats.total_crawled() as u64);

                    // Mark queue item as failed
                    self.update_queue_item_status(
                        queue_item.id,
                        CrawlStatus::Failed,
                        Some(e.to_string()),
                    )
                    .await?;
                }
            }

            // Update progress periodically
            if stats.total_crawled() % 10 == 0 {
                self.update_site_progress(site_id).await?;
            }
        }

        stats.duration = start_time.elapsed();
        bar.finish_and_clear();

        // Final site status update
        if stats.failed_crawls == 0 {
            self.update_site_status(site_id, SiteStatus::Indexing, None)
                .await?;
        } else if stats.successful_crawls == 0 {
            let error_message = format!("All {} crawl attempts failed", stats.failed_crawls);
            self.update_site_status(site_id, SiteStatus::Failed, Some(error_message.clone()))
                .await?;
            bail!("{}", error_message);
        } else {
            self.update_site_status(
                site_id,
                SiteStatus::Indexing,
                Some(format!(
                    "{} pages succeeded, {} failed",
                    stats.successful_crawls, stats.failed_crawls
                )),
            )
            .await?;
        }

        info!(
            "Crawl completed for site {}: {} successful, {} failed, {} blocked by robots.txt, took {:?}",
            site_id,
            stats.successful_crawls,
            stats.failed_crawls,
            stats.robots_blocked,
            stats.duration
        );

        Ok(stats)
    }

    /// Crawl a single page and extract content
    async fn crawl_page(&mut self, url: &Url, base_url: &Url) -> Result<CrawlResult> {
        debug!("Crawling page: {}", url);

        // Try JavaScript rendering first if available, fallback to HTTP client
        let html = match self.try_browser_rendering(url).await {
            Ok(html) => {
                debug!("Successfully rendered page with JavaScript: {}", url);
                html
            }
            Err(e) => {
                debug!(
                    "Browser rendering failed for {}, falling back to HTTP: {}",
                    url, e
                );

                // Fallback to HTTP client
                match self.http_client.get(url.as_str()).await {
                    Ok(html) => html,
                    Err(e) => {
                        return Ok(CrawlResult {
                            url: url.clone(),
                            content: extractor::ExtractedContent {
                                title: String::new(),
                                sections: Vec::new(),
                                raw_text: String::new(),
                            },
                            links: Vec::new(),
                            success: false,
                            error_message: Some(e.to_string()),
                        });
                    }
                }
            }
        };

        // Extract content
        let content = match extract_content(&html, &self.extraction_config) {
            Ok(content) => content,
            Err(e) => {
                return Ok(CrawlResult {
                    url: url.clone(),
                    content: extractor::ExtractedContent {
                        title: String::new(),
                        sections: Vec::new(),
                        raw_text: String::new(),
                    },
                    links: Vec::new(),
                    success: false,
                    error_message: Some(format!("Content extraction failed: {}", e)),
                });
            }
        };

        // Extract links
        let links = match extract_links(&html, url, base_url) {
            Ok(links) => links,
            Err(e) => {
                warn!("Failed to extract links from {}: {}", url, e);
                Vec::new() // Continue without links if extraction fails
            }
        };

        debug!(
            "Successfully processed page {}: {} sections, {} links",
            url,
            content.sections.len(),
            links.len()
        );

        Ok(CrawlResult {
            url: url.clone(),
            content,
            links,
            success: true,
            error_message: None,
        })
    }

    /// Initialize the crawl queue with the base URL
    async fn init_crawl_queue(&self, site_id: i64, index_url: &Url) -> Result<()> {
        let new_item = NewCrawlQueueItem {
            site_id,
            url: index_url.as_str().to_string(),
        };

        CrawlQueueQueries::create(&self.db_pool, new_item).await?;
        info!("Initialized crawl queue with base URL: {}", index_url);
        Ok(())
    }

    /// Get the next queue item to process
    async fn get_next_queue_item(
        &self,
        site_id: i64,
    ) -> Result<Option<crate::database::sqlite::CrawlQueueItem>> {
        CrawlQueueQueries::get_next_pending(&self.db_pool, site_id, self.config.max_retries).await
    }

    /// Add a new URL to the crawl queue
    async fn add_url_to_queue(&self, site_id: i64, url: &str) -> Result<()> {
        let new_item = NewCrawlQueueItem {
            site_id,
            url: url.to_string(),
        };

        CrawlQueueQueries::create(&self.db_pool, new_item).await?;
        Ok(())
    }

    /// Update the status of a queue item
    async fn update_queue_item_status(
        &self,
        item_id: i64,
        status: CrawlStatus,
        error_message: Option<String>,
    ) -> Result<()> {
        let update = CrawlQueueUpdate {
            status: Some(status),
            retry_count: None,
            error_message,
        };

        CrawlQueueQueries::update(&self.db_pool, item_id, update).await?;
        if status == CrawlStatus::Failed {
            CrawlQueueQueries::increment_retry_count(&self.db_pool, item_id).await?;
        }
        Ok(())
    }

    /// Update site status
    async fn update_site_status(
        &self,
        site_id: i64,
        status: SiteStatus,
        error_message: Option<String>,
    ) -> Result<()> {
        let update = SiteUpdate {
            status: Some(status),
            error_message,
            last_heartbeat: Some(chrono::Utc::now().naive_utc()),
            ..Default::default()
        };

        SiteQueries::update(&self.db_pool, site_id, update).await?;
        Ok(())
    }

    /// Update site progress based on queue completion
    async fn update_site_progress(&self, site_id: i64) -> Result<()> {
        let stats = CrawlQueueQueries::get_stats(&self.db_pool, site_id).await?;

        let total_pages = stats.total as i64;
        let indexed_pages = stats.completed as i64;
        let progress_percent = if total_pages > 0 {
            ((indexed_pages as f64 / total_pages as f64) * 100.0) as i64
        } else {
            0
        };

        let update = SiteUpdate {
            total_pages: Some(total_pages),
            indexed_pages: Some(indexed_pages),
            progress_percent: Some(progress_percent),
            last_heartbeat: Some(chrono::Utc::now().naive_utc()),
            ..Default::default()
        };

        SiteQueries::update(&self.db_pool, site_id, update).await?;
        Ok(())
    }

    /// Try to render a page using browser JavaScript rendering
    async fn try_browser_rendering(&self, url: &Url) -> Result<String> {
        if let Some(ref browser_client) = self.browser_client {
            browser_client.get_rendered_html(url).await
        } else {
            Err(anyhow!("Browser client not available"))
        }
    }
}
