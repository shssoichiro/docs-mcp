use chrono::Utc;

use super::*;

#[test]
fn site_status_serialization() {
    assert_eq!(SiteStatus::Pending.to_string(), "Pending");
    assert_eq!(SiteStatus::Indexing.to_string(), "Indexing");
    assert_eq!(SiteStatus::Completed.to_string(), "Completed");
    assert_eq!(SiteStatus::Failed.to_string(), "Failed");
}

#[test]
fn crawl_status_serialization() {
    assert_eq!(CrawlStatus::Pending.to_string(), "Pending");
    assert_eq!(CrawlStatus::Processing.to_string(), "Processing");
    assert_eq!(CrawlStatus::Completed.to_string(), "Completed");
    assert_eq!(CrawlStatus::Failed.to_string(), "Failed");
}

#[test]
fn site_progress_calculation() {
    let site = Site {
        id: 1,
        base_url: "https://example.com".to_string(),
        name: "Test Site".to_string(),
        version: "1.0".to_string(),
        indexed_date: None,
        status: SiteStatus::Indexing,
        progress_percent: 50,
        total_pages: 100,
        indexed_pages: 50,
        error_message: None,
        created_date: Utc::now().naive_utc(),
        last_heartbeat: None,
    };

    assert!(site.is_indexing());
    assert!(!site.is_completed());
    assert!(!site.is_failed());
}
