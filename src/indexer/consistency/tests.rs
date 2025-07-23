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
