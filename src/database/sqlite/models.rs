use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Type};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct Site {
    pub id: i64,
    pub base_url: String,
    pub name: String,
    pub version: String,
    pub indexed_date: Option<NaiveDateTime>,
    pub status: SiteStatus,
    pub progress_percent: i64,
    pub total_pages: i64,
    pub indexed_pages: i64,
    pub error_message: Option<String>,
    pub created_date: NaiveDateTime,
    pub last_heartbeat: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum SiteStatus {
    Pending,
    Indexing,
    Completed,
    Failed,
}

impl std::fmt::Display for SiteStatus {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            SiteStatus::Pending => write!(f, "Pending"),
            SiteStatus::Indexing => write!(f, "Indexing"),
            SiteStatus::Completed => write!(f, "Completed"),
            SiteStatus::Failed => write!(f, "Failed"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewSite {
    pub base_url: String,
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteUpdate {
    pub status: Option<SiteStatus>,
    pub progress_percent: Option<i64>,
    pub total_pages: Option<i64>,
    pub indexed_pages: Option<i64>,
    pub error_message: Option<String>,
    pub last_heartbeat: Option<NaiveDateTime>,
    pub indexed_date: Option<NaiveDateTime>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct CrawlQueueItem {
    pub id: i64,
    pub site_id: i64,
    pub url: String,
    pub status: CrawlStatus,
    pub retry_count: i64,
    pub error_message: Option<String>,
    pub created_date: NaiveDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum CrawlStatus {
    Pending,
    Processing,
    Completed,
    Failed,
}

impl std::fmt::Display for CrawlStatus {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            CrawlStatus::Pending => write!(f, "Pending"),
            CrawlStatus::Processing => write!(f, "Processing"),
            CrawlStatus::Completed => write!(f, "Completed"),
            CrawlStatus::Failed => write!(f, "Failed"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewCrawlQueueItem {
    pub site_id: i64,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrawlQueueUpdate {
    pub status: Option<CrawlStatus>,
    pub retry_count: Option<i64>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, FromRow)]
pub struct IndexedChunk {
    pub id: i64,
    pub site_id: i64,
    pub url: String,
    pub page_title: Option<String>,
    pub heading_path: Option<String>,
    pub chunk_content: String,
    pub chunk_index: i64,
    pub vector_id: String,
    pub indexed_date: NaiveDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewIndexedChunk {
    pub site_id: i64,
    pub url: String,
    pub page_title: Option<String>,
    pub heading_path: Option<String>,
    pub chunk_content: String,
    pub chunk_index: i64,
    pub vector_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SiteStatistics {
    pub site: Site,
    pub total_chunks: i64,
    pub pending_crawl_items: i64,
    pub failed_crawl_items: i64,
}

impl Site {
    #[inline]
    pub fn is_completed(&self) -> bool {
        self.status == SiteStatus::Completed
    }

    #[inline]
    pub fn is_indexing(&self) -> bool {
        self.status == SiteStatus::Indexing
    }

    #[inline]
    pub fn is_failed(&self) -> bool {
        self.status == SiteStatus::Failed
    }

    #[inline]
    pub fn progress_percentage(&self) -> f64 {
        if self.total_pages == 0 {
            0.0
        } else {
            (self.indexed_pages as f64 / self.total_pages as f64) * 100.0
        }
    }
}

impl CrawlQueueItem {
    #[inline]
    pub fn can_retry(&self) -> bool {
        self.status == CrawlStatus::Failed && self.retry_count < 3
    }

    #[inline]
    pub fn should_process(&self) -> bool {
        self.status == CrawlStatus::Pending
            || (self.status == CrawlStatus::Failed && self.can_retry())
    }
}

#[cfg(test)]
mod tests {
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

        assert_eq!(site.progress_percentage(), 50.0);
        assert!(site.is_indexing());
        assert!(!site.is_completed());
        assert!(!site.is_failed());
    }

    #[test]
    fn crawl_queue_retry_logic() {
        let item = CrawlQueueItem {
            id: 1,
            site_id: 1,
            url: "https://example.com/page".to_string(),
            status: CrawlStatus::Failed,
            retry_count: 2,
            error_message: Some("Network error".to_string()),
            created_date: Utc::now().naive_utc(),
        };

        assert!(item.can_retry());
        assert!(item.should_process());

        let item_max_retries = CrawlQueueItem {
            retry_count: 3,
            ..item
        };

        assert!(!item_max_retries.can_retry());
        assert!(!item_max_retries.should_process());
    }
}
