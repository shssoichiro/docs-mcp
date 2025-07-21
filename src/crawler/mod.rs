pub mod robots;

#[cfg(test)]
mod tests;

use anyhow::{Context, Result, anyhow};
use scraper::{Html, Selector};
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tracing::{debug, error, info, warn};
use ureq::Agent;
use url::Url;

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

    url_path.starts_with(&base_path)
}

/// Normalize a URL path for filtering by removing trailing filename if present
fn normalize_path_for_filtering(path: &str) -> String {
    if path.ends_with('/') {
        path.to_string()
    } else {
        // Check if the last segment looks like a filename (contains a dot)
        path.rfind('/').map_or_else(
            || format!("{}/", path),
            #[expect(clippy::string_slice, reason = "we know the split point is one byte")]
            |last_slash| {
                let last_segment = &path[last_slash + 1..];
                if last_segment.contains('.') && !last_segment.ends_with('/') {
                    // Looks like a filename, use the directory path
                    path[..=last_slash].to_string()
                } else {
                    // Not a filename, add trailing slash
                    format!("{}/", path)
                }
            },
        )
    }
}

/// Extract all links from HTML content using proper HTML parsing
#[inline]
pub fn extract_links(html: &str, base_url: &Url) -> Result<Vec<Url>> {
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

            match base_url.join(href) {
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

    info!("Extracted {} valid links from {}", links.len(), base_url);
    Ok(links)
}
