#[cfg(test)]
mod tests;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
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
}
