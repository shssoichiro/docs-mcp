use super::*;

#[test]
fn parse_empty_robots_txt() {
    let robots = RobotsTxt::parse("");
    let url = Url::parse("https://example.com/test").expect("url should parse");

    assert!(robots.is_allowed(&url, "docs-mcp"));
    assert!(robots.is_allowed(&url, "*"));
}

#[test]
fn parse_simple_robots_txt() {
    let content = r#"
            User-agent: *
            Disallow: /private/
            Disallow: /admin/
            Allow: /public/
        "#;

    let robots = RobotsTxt::parse(content);

    // Should disallow private paths
    assert!(!robots.is_allowed(
        &Url::parse("https://example.com/private/secret.html").expect("url should parse"),
        "docs-mcp"
    ));
    assert!(!robots.is_allowed(
        &Url::parse("https://example.com/admin/panel").expect("url should parse"),
        "docs-mcp"
    ));

    // Should allow public paths
    assert!(robots.is_allowed(
        &Url::parse("https://example.com/public/docs.html").expect("url should parse"),
        "docs-mcp"
    ));

    // Should allow other paths
    assert!(robots.is_allowed(
        &Url::parse("https://example.com/docs/").expect("url should parse"),
        "docs-mcp"
    ));
}

#[test]
fn parse_specific_user_agent() {
    let content = r#"
            User-agent: badbot
            Disallow: /
            
            User-agent: docs-mcp
            Disallow: /private/
            Allow: /private/allowed/
            
            User-agent: *
            Disallow: /admin/
        "#;

    let robots = RobotsTxt::parse(content);

    // Bad bot should be disallowed everywhere
    assert!(!robots.is_allowed(
        &Url::parse("https://example.com/").expect("url should parse"),
        "badbot"
    ));
    assert!(!robots.is_allowed(
        &Url::parse("https://example.com/docs/").expect("url should parse"),
        "badbot"
    ));

    // docs-mcp should have specific rules
    assert!(!robots.is_allowed(
        &Url::parse("https://example.com/private/secret.html").expect("url should parse"),
        "docs-mcp"
    ));
    assert!(robots.is_allowed(
        &Url::parse("https://example.com/private/allowed/file.html").expect("url should parse"),
        "docs-mcp"
    ));
    assert!(!robots.is_allowed(
        &Url::parse("https://example.com/admin/panel").expect("url should parse"),
        "docs-mcp"
    ));

    // Other user agents should use default rules
    assert!(!robots.is_allowed(
        &Url::parse("https://example.com/admin/panel").expect("url should parse"),
        "other-bot"
    ));
    assert!(robots.is_allowed(
        &Url::parse("https://example.com/docs/").expect("url should parse"),
        "other-bot"
    ));
}

#[test]
fn parse_with_comments() {
    let content = r#"
            # This is a comment
            User-agent: *
            # Another comment
            Disallow: /test/  # Inline comment
            
            # More comments
            Allow: /test/public/
        "#;

    let robots = RobotsTxt::parse(content);

    assert!(!robots.is_allowed(
        &Url::parse("https://example.com/test/private.html").expect("url should parse"),
        "docs-mcp"
    ));
    assert!(robots.is_allowed(
        &Url::parse("https://example.com/test/public/file.html").expect("url should parse"),
        "docs-mcp"
    ));
}

#[test]
fn path_matching() {
    // Exact path matching
    assert!(path_matches_pattern("/test/", "/test/"));
    assert!(path_matches_pattern("/test/file.html", "/test/"));
    assert!(!path_matches_pattern("/other/", "/test/"));

    // Wildcard matching
    assert!(path_matches_pattern("/test/anything", "/test/*"));
    assert!(path_matches_pattern("/test/", "/test/*"));
    assert!(!path_matches_pattern("/other/", "/test/*"));

    // Root pattern
    assert!(path_matches_pattern("/anything", "/"));
    assert!(path_matches_pattern("/", "/"));

    // Empty pattern
    assert!(path_matches_pattern("/anything", ""));
}

#[test]
fn robots_url_generation() {
    let base = Url::parse("https://example.com/docs/api/").expect("url should parse");
    let robots_url = RobotsTxt::robots_url(&base).expect("robots_url should succeed");

    assert_eq!(robots_url.as_str(), "https://example.com/robots.txt");
}

#[test]
fn robots_url_with_query_and_fragment() {
    let base = Url::parse("https://example.com/docs?version=1#section").expect("url should parse");
    let robots_url = RobotsTxt::robots_url(&base).expect("robots_url should succeed");

    assert_eq!(robots_url.as_str(), "https://example.com/robots.txt");
}

#[test]
fn case_insensitive_user_agent() {
    let content = r#"
            User-agent: DOCS-MCP
            Disallow: /private/
        "#;

    let robots = RobotsTxt::parse(content);

    // Should work with different cases
    assert!(!robots.is_allowed(
        &Url::parse("https://example.com/private/secret.html").expect("url should parse"),
        "docs-mcp"
    ));
    assert!(!robots.is_allowed(
        &Url::parse("https://example.com/private/secret.html").expect("url should parse"),
        "DOCS-MCP"
    ));
    assert!(!robots.is_allowed(
        &Url::parse("https://example.com/private/secret.html").expect("url should parse"),
        "Docs-Mcp"
    ));
}
