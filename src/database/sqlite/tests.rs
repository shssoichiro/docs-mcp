use super::*;
use anyhow::Result;
use chrono::Utc;
use std::collections::HashSet;
use tempfile::TempDir;

async fn create_test_database() -> Result<(TempDir, Database)> {
    let temp_dir = TempDir::new()?;
    let database = Database::initialize_from_config_dir(temp_dir.path()).await?;
    Ok((temp_dir, database))
}

#[tokio::test]
async fn integration_schema_migration() -> Result<()> {
    let (_temp_dir, database) = create_test_database().await?;

    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    )
    .fetch_all(database.pool())
    .await?;

    let expected_tables: HashSet<&'static str> = [
        "sites",
        "crawl_queue",
        "indexed_chunks",
        "indexer_heartbeat",
    ]
    .into_iter()
    .collect();

    let actual_tables: HashSet<&str> = tables.iter().map(|t| t.as_str()).collect();
    assert_eq!(actual_tables, expected_tables);

    Ok(())
}

#[tokio::test]
async fn integration_foreign_key_constraints() -> Result<()> {
    let (_temp_dir, database) = create_test_database().await?;

    let new_site = NewSite {
        base_url: "https://example.com".to_string(),
        name: "Test Site".to_string(),
        version: "1.0".to_string(),
    };

    let site = SiteQueries::create(database.pool(), new_site).await?;

    let new_item = NewCrawlQueueItem {
        site_id: site.id,
        url: "https://example.com/page1".to_string(),
    };

    let queue_item = CrawlQueueQueries::add_url(database.pool(), new_item).await?;

    let new_chunk = NewIndexedChunk {
        site_id: site.id,
        url: "https://example.com/page1".to_string(),
        page_title: Some("Test Page".to_string()),
        heading_path: Some("Page Title > Section".to_string()),
        chunk_content: "This is a test chunk.".to_string(),
        chunk_index: 0,
        vector_id: "test-vector-id".to_string(),
    };

    let chunk = IndexedChunkQueries::create(database.pool(), new_chunk).await?;

    SiteQueries::delete(database.pool(), site.id).await?;

    let queue_item_after_delete =
        CrawlQueueQueries::get_by_id(database.pool(), queue_item.id).await?;
    assert!(queue_item_after_delete.is_none());

    let chunk_after_delete =
        IndexedChunkQueries::get_by_vector_id(database.pool(), &chunk.vector_id).await?;
    assert!(chunk_after_delete.is_none());

    Ok(())
}

#[tokio::test]
async fn integration_site_workflow() -> Result<()> {
    let (_temp_dir, database) = create_test_database().await?;

    let new_site = NewSite {
        base_url: "https://docs.example.com".to_string(),
        name: "Example Docs".to_string(),
        version: "2.0".to_string(),
    };

    let site = SiteQueries::create(database.pool(), new_site).await?;
    assert_eq!(site.status, SiteStatus::Pending);
    assert_eq!(site.progress_percent, 0);

    let urls = vec![
        "https://docs.example.com/intro".to_string(),
        "https://docs.example.com/guide".to_string(),
        "https://docs.example.com/api".to_string(),
    ];

    let inserted_count = CrawlQueueQueries::add_urls_batch(database.pool(), site.id, urls).await?;
    assert_eq!(inserted_count, 3);

    let update = SiteUpdate {
        status: Some(SiteStatus::Indexing),
        total_pages: Some(3),
        indexed_pages: Some(0),
        progress_percent: Some(0),
        last_heartbeat: Some(Utc::now().naive_utc()),
        error_message: None,
        indexed_date: None,
    };

    let updated_site = SiteQueries::update(database.pool(), site.id, update)
        .await?
        .expect("should update site queries successfully");
    assert_eq!(updated_site.status, SiteStatus::Indexing);
    assert_eq!(updated_site.total_pages, 3);

    for i in 0..3 {
        let pending_item = CrawlQueueQueries::get_next_pending(database.pool(), site.id, 3)
            .await?
            .expect("should get next pending succesfully");

        let processing_update = CrawlQueueUpdate {
            status: Some(CrawlStatus::Processing),
            retry_count: None,
            error_message: None,
        };
        CrawlQueueQueries::update_status(database.pool(), pending_item.id, processing_update)
            .await?;

        let new_chunk = NewIndexedChunk {
            site_id: site.id,
            url: pending_item.url.clone(),
            page_title: Some(format!("Page {}", i + 1)),
            heading_path: Some(format!("Page {} > Content", i + 1)),
            chunk_content: format!("Content for page {}", i + 1),
            chunk_index: 0,
            vector_id: format!("vector-{}", i + 1),
        };

        IndexedChunkQueries::create(database.pool(), new_chunk).await?;

        let completed_update = CrawlQueueUpdate {
            status: Some(CrawlStatus::Completed),
            retry_count: None,
            error_message: None,
        };
        CrawlQueueQueries::update_status(database.pool(), pending_item.id, completed_update)
            .await?;

        let site_progress_update = SiteUpdate {
            indexed_pages: Some(i + 1),
            progress_percent: Some(((i + 1) * 100) / 3),
            status: None,
            total_pages: None,
            error_message: None,
            last_heartbeat: Some(Utc::now().naive_utc()),
            indexed_date: None,
        };
        SiteQueries::update(database.pool(), site.id, site_progress_update).await?;
    }

    let completion_update = SiteUpdate {
        status: Some(SiteStatus::Completed),
        indexed_date: Some(Utc::now().naive_utc()),
        progress_percent: Some(100),
        total_pages: None,
        indexed_pages: None,
        error_message: None,
        last_heartbeat: None,
    };

    let final_site = SiteQueries::update(database.pool(), site.id, completion_update)
        .await?
        .expect("should update site queries successfully");
    assert_eq!(final_site.status, SiteStatus::Completed);
    assert_eq!(final_site.progress_percent, 100);
    assert_eq!(final_site.indexed_pages, 3);

    let statistics = SiteQueries::get_statistics(database.pool(), site.id)
        .await?
        .expect("should get statistics successfully");
    assert_eq!(statistics.total_chunks, 3);
    assert_eq!(statistics.pending_crawl_items, 0);
    assert_eq!(statistics.failed_crawl_items, 0);

    let completed_sites = SiteQueries::list_completed(database.pool()).await?;
    assert_eq!(completed_sites.len(), 1);
    assert_eq!(completed_sites[0].id, site.id);

    Ok(())
}

#[tokio::test]
async fn integration_batch_operations() -> Result<()> {
    let (_temp_dir, database) = create_test_database().await?;

    let new_site = NewSite {
        base_url: "https://batch.example.com".to_string(),
        name: "Batch Test".to_string(),
        version: "1.0".to_string(),
    };

    let site = SiteQueries::create(database.pool(), new_site).await?;

    let batch_urls: Vec<String> = (1..=10)
        .map(|i| format!("https://batch.example.com/page{}", i))
        .collect();

    let inserted_count =
        CrawlQueueQueries::add_urls_batch(database.pool(), site.id, batch_urls.clone()).await?;
    assert_eq!(inserted_count, 10);

    let duplicate_inserted =
        CrawlQueueQueries::add_urls_batch(database.pool(), site.id, batch_urls).await?;
    assert_eq!(duplicate_inserted, 0);

    let batch_chunks: Vec<NewIndexedChunk> = (1..=5)
        .map(|i| NewIndexedChunk {
            site_id: site.id,
            url: format!("https://batch.example.com/page{}", i),
            page_title: Some(format!("Batch Page {}", i)),
            heading_path: Some(format!("Batch Page {} > Content", i)),
            chunk_content: format!("Batch content for page {}", i),
            chunk_index: 0,
            vector_id: format!("batch-vector-{}", i),
        })
        .collect();

    let created_chunks = IndexedChunkQueries::create_batch(database.pool(), batch_chunks).await?;
    assert_eq!(created_chunks.len(), 5);

    let chunks_by_site = IndexedChunkQueries::list_by_site(database.pool(), site.id).await?;
    assert_eq!(chunks_by_site.len(), 5);

    let chunk_count = IndexedChunkQueries::count_by_site(database.pool(), site.id).await?;
    assert_eq!(chunk_count, 5);

    let deleted_chunk_count = IndexedChunkQueries::delete_by_site(database.pool(), site.id).await?;
    assert_eq!(deleted_chunk_count, 5);

    let final_chunk_count = IndexedChunkQueries::count_by_site(database.pool(), site.id).await?;
    assert_eq!(final_chunk_count, 0);

    Ok(())
}

#[tokio::test]
async fn integration_error_handling() -> Result<()> {
    let (_temp_dir, database) = create_test_database().await?;

    let invalid_site_result = SiteQueries::get_by_id(database.pool(), 999).await?;
    assert!(invalid_site_result.is_none());

    let invalid_chunk_result =
        IndexedChunkQueries::get_by_vector_id(database.pool(), "nonexistent").await?;
    assert!(invalid_chunk_result.is_none());

    let new_site = NewSite {
        base_url: "https://error.example.com".to_string(),
        name: "Error Test".to_string(),
        version: "1.0".to_string(),
    };

    let site = SiteQueries::create(database.pool(), new_site).await?;

    let error_update = SiteUpdate {
        status: Some(SiteStatus::Failed),
        error_message: Some("Test error message".to_string()),
        progress_percent: None,
        total_pages: None,
        indexed_pages: None,
        last_heartbeat: None,
        indexed_date: None,
    };

    let failed_site = SiteQueries::update(database.pool(), site.id, error_update)
        .await?
        .expect("should update site queries successfully");
    assert_eq!(failed_site.status, SiteStatus::Failed);
    assert_eq!(
        failed_site.error_message,
        Some("Test error message".to_string())
    );

    Ok(())
}

#[tokio::test]
async fn integration_transaction_rollback() -> Result<()> {
    let (_temp_dir, database) = create_test_database().await?;

    let new_site = NewSite {
        base_url: "https://transaction.example.com".to_string(),
        name: "Transaction Test".to_string(),
        version: "1.0".to_string(),
    };

    let site = SiteQueries::create(database.pool(), new_site).await?;

    let mut transaction = database.begin_transaction().await?;

    let new_chunk = NewIndexedChunk {
        site_id: site.id,
        url: "https://transaction.example.com/page1".to_string(),
        page_title: Some("Transaction Page".to_string()),
        heading_path: Some("Transaction Page > Content".to_string()),
        chunk_content: "Transaction content".to_string(),
        chunk_index: 0,
        vector_id: "transaction-vector".to_string(),
    };

    sqlx::query(
        r#"
        INSERT INTO indexed_chunks (site_id, url, page_title, heading_path, chunk_content, chunk_index, vector_id, indexed_date)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#
    )
    .bind(new_chunk.site_id)
    .bind(&new_chunk.url)
    .bind(&new_chunk.page_title)
    .bind(&new_chunk.heading_path)
    .bind(&new_chunk.chunk_content)
    .bind(new_chunk.chunk_index)
    .bind(&new_chunk.vector_id)
    .bind(Utc::now())
    .execute(&mut *transaction)
    .await?;

    transaction.rollback().await?;

    let chunk_after_rollback =
        IndexedChunkQueries::get_by_vector_id(database.pool(), "transaction-vector").await?;
    assert!(chunk_after_rollback.is_none());

    Ok(())
}

#[tokio::test]
async fn integration_concurrent_access() -> Result<()> {
    let (_temp_dir, database) = create_test_database().await?;

    let new_site = NewSite {
        base_url: "https://concurrent.example.com".to_string(),
        name: "Concurrent Test".to_string(),
        version: "1.0".to_string(),
    };

    let site = SiteQueries::create(database.pool(), new_site).await?;

    let mut handles = Vec::new();

    for i in 0..10 {
        let pool = database.pool().clone();
        let site_id = site.id;

        let handle = tokio::spawn(async move {
            let new_chunk = NewIndexedChunk {
                site_id,
                url: format!("https://concurrent.example.com/page{}", i),
                page_title: Some(format!("Concurrent Page {}", i)),
                heading_path: Some(format!("Concurrent Page {} > Content", i)),
                chunk_content: format!("Concurrent content {}", i),
                chunk_index: i,
                vector_id: format!("concurrent-vector-{}", i),
            };

            IndexedChunkQueries::create(&pool, new_chunk).await
        });

        handles.push(handle);
    }

    let mut successful_inserts = 0;
    for handle in handles {
        if handle
            .await
            .expect("handle should join successfully")
            .is_ok()
        {
            successful_inserts += 1;
        }
    }

    assert_eq!(successful_inserts, 10);

    let final_count = IndexedChunkQueries::count_by_site(database.pool(), site.id).await?;
    assert_eq!(final_count, 10);

    Ok(())
}
