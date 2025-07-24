use super::*;

#[test]
fn consistency_report_creation() {
    let report = ConsistencyReport {
        sqlite_chunks: 100,
        lancedb_embeddings: 95,
        missing_in_lancedb: vec!["vec1".to_string(), "vec2".to_string()],
        orphaned_in_lancedb: vec![],
        inconsistent_sites: vec![],
        is_consistent: false,
    };

    assert_eq!(report.total_issues(), 2);
    assert!(report.summary().contains("inconsistencies found"));
}

#[test]
fn consistent_report() {
    let report = ConsistencyReport {
        sqlite_chunks: 100,
        lancedb_embeddings: 100,
        missing_in_lancedb: vec![],
        orphaned_in_lancedb: vec![],
        inconsistent_sites: vec![],
        is_consistent: true,
    };

    assert_eq!(report.total_issues(), 0);
    assert!(report.summary().contains("Database is consistent"));
}

#[test]
fn consistency_report_summary_formats_correctly() {
    // Test inconsistent report summary
    let inconsistent_report = ConsistencyReport {
        sqlite_chunks: 150,
        lancedb_embeddings: 140,
        missing_in_lancedb: vec!["vec1".to_string(), "vec2".to_string(), "vec3".to_string()],
        orphaned_in_lancedb: vec!["orphan1".to_string()],
        inconsistent_sites: vec![SiteConsistencyIssue {
            site_id: 1,
            site_name: "Test Site".to_string(),
            sqlite_chunks: 10,
            lancedb_embeddings: 8,
            missing_in_lancedb: vec!["vec1".to_string()],
            orphaned_in_lancedb: vec![],
        }],
        is_consistent: false,
    };

    let summary = inconsistent_report.summary();
    assert!(summary.contains("3 missing in LanceDB"));
    assert!(summary.contains("1 orphaned in LanceDB"));
    assert!(summary.contains("1 sites with issues"));

    // Test consistent report summary
    let consistent_report = ConsistencyReport {
        sqlite_chunks: 100,
        lancedb_embeddings: 100,
        missing_in_lancedb: vec![],
        orphaned_in_lancedb: vec![],
        inconsistent_sites: vec![],
        is_consistent: true,
    };

    let summary = consistent_report.summary();
    assert!(summary.contains("Database is consistent"));
    assert!(summary.contains("100 chunks in SQLite"));
    assert!(summary.contains("100 embeddings in LanceDB"));
}

#[test]
fn consistency_report_total_issues_calculation() {
    let report = ConsistencyReport {
        sqlite_chunks: 100,
        lancedb_embeddings: 95,
        missing_in_lancedb: vec!["vec1".to_string(), "vec2".to_string(), "vec3".to_string()],
        orphaned_in_lancedb: vec!["orphan1".to_string(), "orphan2".to_string()],
        inconsistent_sites: vec![],
        is_consistent: false,
    };

    assert_eq!(report.total_issues(), 5); // 3 missing + 2 orphaned
}

#[test]
fn site_consistency_issue_creation() {
    let issue = SiteConsistencyIssue {
        site_id: 42,
        site_name: "Test Documentation".to_string(),
        sqlite_chunks: 25,
        lancedb_embeddings: 20,
        missing_in_lancedb: vec!["missing1".to_string(), "missing2".to_string()],
        orphaned_in_lancedb: vec!["orphan1".to_string()],
    };

    assert_eq!(issue.site_id, 42);
    assert_eq!(issue.site_name, "Test Documentation");
    assert_eq!(issue.sqlite_chunks, 25);
    assert_eq!(issue.lancedb_embeddings, 20);
    assert_eq!(issue.missing_in_lancedb.len(), 2);
    assert_eq!(issue.orphaned_in_lancedb.len(), 1);
}
