use super::*;
use sqlx::sqlite::SqlitePoolOptions;
use tempfile::TempDir;

async fn create_test_pool() -> (TempDir, SqlitePool) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let db_path = temp_dir.path().join("test.db");

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&db_path)
                .create_if_missing(true)
                .foreign_keys(true),
        )
        .await
        .expect("Failed to create test pool");

    sqlx::query(include_str!("../migrations/001_initial_schema.sql"))
        .execute(&pool)
        .await
        .expect("Failed to run migrations");
    sqlx::query(include_str!("../migrations/002_add_index_url.sql"))
        .execute(&pool)
        .await
        .expect("Failed to run migrations");

    (temp_dir, pool)
}

#[tokio::test]
async fn site_crud_operations() {
    let (_temp_dir, pool) = create_test_pool().await;

    let new_site = NewSite {
        base_url: "https://example.com".to_string(),
        index_url: "https://example.com".to_string(),
        name: "Test Site".to_string(),
        version: "1.0".to_string(),
    };

    let created_site = SiteQueries::create(&pool, new_site)
        .await
        .expect("Failed to create site");

    assert_eq!(created_site.name, "Test Site");
    assert_eq!(created_site.status, SiteStatus::Pending);

    let retrieved_site = SiteQueries::get_by_id(&pool, created_site.id)
        .await
        .expect("Failed to get site")
        .expect("Site should exist");

    assert_eq!(retrieved_site.id, created_site.id);
    assert_eq!(retrieved_site.name, "Test Site");

    let update = SiteUpdate {
        status: Some(SiteStatus::Indexing),
        progress_percent: Some(50),
        total_pages: Some(100),
        indexed_pages: Some(50),
        error_message: None,
        last_heartbeat: Some(Utc::now().naive_utc()),
        indexed_date: None,
    };

    let updated_site = SiteQueries::update(&pool, created_site.id, update)
        .await
        .expect("Failed to update site")
        .expect("Site should exist");

    assert_eq!(updated_site.status, SiteStatus::Indexing);
    assert_eq!(updated_site.progress_percent, 50);

    let deleted = SiteQueries::delete(&pool, created_site.id)
        .await
        .expect("Failed to delete site");

    assert!(deleted);

    let not_found = SiteQueries::get_by_id(&pool, created_site.id)
        .await
        .expect("Query should succeed");

    assert!(not_found.is_none());
}

#[tokio::test]
async fn crawl_queue_operations() {
    let (_temp_dir, pool) = create_test_pool().await;

    let new_site = NewSite {
        base_url: "https://example.com".to_string(),
        index_url: "https://example.com".to_string(),
        name: "Test Site".to_string(),
        version: "1.0".to_string(),
    };

    let site = SiteQueries::create(&pool, new_site)
        .await
        .expect("Failed to create site");

    let new_item = NewCrawlQueueItem {
        site_id: site.id,
        url: "https://example.com/page1".to_string(),
    };

    let created_item = CrawlQueueQueries::add_url(&pool, new_item)
        .await
        .expect("Failed to add URL to queue");

    assert_eq!(created_item.site_id, site.id);
    assert_eq!(created_item.status, CrawlStatus::Pending);

    let next_item = CrawlQueueQueries::get_next_pending(&pool, site.id, 3)
        .await
        .expect("Failed to get next pending")
        .expect("Should have pending item");

    assert_eq!(next_item.id, created_item.id);

    let update = CrawlQueueUpdate {
        status: Some(CrawlStatus::Completed),
        retry_count: None,
        error_message: None,
    };

    let updated_item = CrawlQueueQueries::update_status(&pool, created_item.id, update)
        .await
        .expect("Failed to update status")
        .expect("Item should exist");

    assert_eq!(updated_item.status, CrawlStatus::Completed);
}

#[tokio::test]
async fn indexed_chunk_operations() {
    let (_temp_dir, pool) = create_test_pool().await;

    let new_site = NewSite {
        base_url: "https://example.com".to_string(),
        index_url: "https://example.com".to_string(),
        name: "Test Site".to_string(),
        version: "1.0".to_string(),
    };

    let site = SiteQueries::create(&pool, new_site)
        .await
        .expect("Failed to create site");

    let new_chunk = NewIndexedChunk {
        site_id: site.id,
        url: "https://example.com/page1".to_string(),
        page_title: Some("Test Page".to_string()),
        heading_path: Some("Page Title > Section".to_string()),
        chunk_content: "This is a test chunk of content.".to_string(),
        chunk_index: 0,
        vector_id: "test-vector-id".to_string(),
    };

    let created_chunk = IndexedChunkQueries::create(&pool, new_chunk)
        .await
        .expect("Failed to create chunk");

    assert_eq!(created_chunk.site_id, site.id);
    assert_eq!(created_chunk.vector_id, "test-vector-id");

    let retrieved_chunk = IndexedChunkQueries::get_by_vector_id(&pool, "test-vector-id")
        .await
        .expect("Failed to get chunk")
        .expect("Chunk should exist");

    assert_eq!(retrieved_chunk.id, created_chunk.id);
    assert_eq!(
        retrieved_chunk.chunk_content,
        "This is a test chunk of content."
    );

    let count = IndexedChunkQueries::count_by_site(&pool, site.id)
        .await
        .expect("Failed to count chunks");

    assert_eq!(count, 1);
}
