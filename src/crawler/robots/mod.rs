#[cfg(test)]
mod tests;

use anyhow::{Context, Result};
use std::collections::HashMap;
use tracing::{debug, warn};
use url::Url;

/// Represents a robots.txt file and its rules
#[derive(Debug, Clone)]
pub struct RobotsTxt {
    /// Rules for different user agents
    rules: HashMap<String, UserAgentRules>,
    /// Default rules for all user agents (*)
    default_rules: UserAgentRules,
}

/// Rules for a specific user agent
#[derive(Debug, Clone, Default)]
struct UserAgentRules {
    /// List of disallowed path patterns
    disallowed: Vec<String>,
    /// List of allowed path patterns (takes precedence over disallowed)
    allowed: Vec<String>,
}

impl RobotsTxt {
    /// Parse robots.txt content
    #[inline]
    pub fn parse(content: &str) -> Self {
        let mut rules: HashMap<String, UserAgentRules> = HashMap::new();
        let mut default_rules = UserAgentRules::default();
        let mut current_user_agents = Vec::new();

        for line in content.lines() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse directive
            if let Some((directive, value)) = parse_directive(line) {
                match directive.to_lowercase().as_str() {
                    "user-agent" => {
                        current_user_agents.clear();
                        current_user_agents.push(value.to_lowercase());
                    }
                    "disallow" => {
                        if current_user_agents.is_empty() {
                            warn!("Disallow directive without User-agent: {}", line);
                            continue;
                        }

                        for user_agent in &current_user_agents {
                            if user_agent == "*" {
                                default_rules.disallowed.push(value.to_string());
                            } else {
                                rules
                                    .entry(user_agent.clone())
                                    .or_default()
                                    .disallowed
                                    .push(value.to_string());
                            }
                        }
                    }
                    "allow" => {
                        if current_user_agents.is_empty() {
                            warn!("Allow directive without User-agent: {}", line);
                            continue;
                        }

                        for user_agent in &current_user_agents {
                            if user_agent == "*" {
                                default_rules.allowed.push(value.to_string());
                            } else {
                                rules
                                    .entry(user_agent.clone())
                                    .or_default()
                                    .allowed
                                    .push(value.to_string());
                            }
                        }
                    }
                    "crawl-delay" | "sitemap" => {
                        // We ignore these for now but could implement them later
                        debug!("Ignoring robots.txt directive: {}: {}", directive, value);
                    }
                    _ => {
                        debug!("Unknown robots.txt directive: {}: {}", directive, value);
                    }
                }
            }
        }

        Self {
            rules,
            default_rules,
        }
    }

    /// Check if a URL is allowed to be crawled by the given user agent
    #[inline]
    pub fn is_allowed(&self, url: &Url, user_agent: &str) -> bool {
        let path = url.path();
        let user_agent_lower = user_agent.to_lowercase();

        // First check specific user agent rules
        if let Some(agent_rules) = self.rules.get(&user_agent_lower) {
            // Check if explicitly allowed (takes precedence)
            for allow_pattern in &agent_rules.allowed {
                if path_matches_pattern(path, allow_pattern) {
                    debug!("URL {} allowed by specific pattern: {}", url, allow_pattern);
                    return true;
                }
            }

            // Check if explicitly disallowed
            for disallow_pattern in &agent_rules.disallowed {
                if path_matches_pattern(path, disallow_pattern) {
                    debug!(
                        "URL {} disallowed by specific pattern: {}",
                        url, disallow_pattern
                    );
                    return false;
                }
            }
        }

        // If no specific rules matched, check default rules
        // Check if explicitly allowed by default rules
        for allow_pattern in &self.default_rules.allowed {
            if path_matches_pattern(path, allow_pattern) {
                debug!("URL {} allowed by default pattern: {}", url, allow_pattern);
                return true;
            }
        }

        // Check if explicitly disallowed by default rules
        for disallow_pattern in &self.default_rules.disallowed {
            if path_matches_pattern(path, disallow_pattern) {
                debug!(
                    "URL {} disallowed by default pattern: {}",
                    url, disallow_pattern
                );
                return false;
            }
        }

        // Default is to allow
        true
    }

    /// Get the robots.txt URL for a given base URL
    #[inline]
    pub fn robots_url(base_url: &Url) -> Result<Url> {
        let mut robots_url = base_url.clone();
        robots_url.set_path("/robots.txt");
        robots_url.set_query(None);
        robots_url.set_fragment(None);
        Ok(robots_url)
    }
}

/// Parse a robots.txt directive line
fn parse_directive(line: &str) -> Option<(&str, &str)> {
    #[expect(
        clippy::string_slice,
        reason = "we know the slice points are on char boundaries"
    )]
    line.find(':').map(|colon_pos| {
        let directive = line[..colon_pos].trim();
        let mut value = line[colon_pos + 1..].trim();

        // Handle inline comments
        if let Some(comment_pos) = value.find('#') {
            value = value[..comment_pos].trim();
        }

        (directive, value)
    })
}

/// Check if a path matches a robots.txt pattern
fn path_matches_pattern(path: &str, pattern: &str) -> bool {
    // Empty pattern means root
    if pattern.is_empty() || pattern == "/" {
        return true;
    }

    // Simple prefix matching
    // robots.txt patterns can use wildcards, but we'll implement basic matching first
    pattern.strip_suffix('*').map_or_else(
        || path.starts_with(pattern),
        |prefix| path.starts_with(prefix),
    )
}

/// Fetch and parse robots.txt for a given URL
#[inline]
pub async fn fetch_robots_txt(
    http_client: &mut crate::crawler::HttpClient,
    base_url: &Url,
) -> Result<RobotsTxt> {
    let robots_url = RobotsTxt::robots_url(base_url)?;

    debug!("Fetching robots.txt from: {}", robots_url);

    match http_client.get(robots_url.as_str()).await {
        Ok(content) => {
            debug!("Successfully fetched robots.txt ({} bytes)", content.len());
            Ok(RobotsTxt::parse(&content))
        }
        Err(e) => {
            let error_str = e.to_string().to_lowercase();

            // 404 is expected and means no robots.txt (allow all)
            if error_str.contains("404") || error_str.contains("not found") {
                debug!("No robots.txt found (404), allowing all URLs");
                Ok(RobotsTxt::parse(""))
            } else {
                Err(e).with_context(|| format!("Failed to fetch robots.txt from {}", robots_url))
            }
        }
    }
}
