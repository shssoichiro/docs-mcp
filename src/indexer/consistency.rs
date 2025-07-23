// Database consistency validation module
// Ensures data integrity between SQLite and LanceDB

use anyhow::Result;
use std::collections::{HashMap, HashSet};
use tracing::{debug, error, info, warn};

use crate::DocsError;
use crate::database::lancedb::VectorStore;
use crate::database::sqlite::Database;

/// Consistency check results between SQLite and LanceDB
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsistencyReport {
    /// Number of chunks in SQLite
    pub sqlite_chunks: usize,
    /// Number of embeddings in LanceDB
    pub lancedb_embeddings: usize,
    /// Vector IDs that exist in SQLite but not in LanceDB
    pub missing_in_lancedb: Vec<String>,
    /// Vector IDs that exist in LanceDB but not in SQLite
    pub orphaned_in_lancedb: Vec<String>,
    /// Sites with consistency issues
    pub inconsistent_sites: Vec<SiteConsistencyIssue>,
    /// Overall consistency status
    pub is_consistent: bool,
}

/// Consistency issue for a specific site
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SiteConsistencyIssue {
    pub site_id: i64,
    pub site_name: String,
    pub sqlite_chunks: usize,
    pub lancedb_embeddings: usize,
    pub missing_in_lancedb: Vec<String>,
    pub orphaned_in_lancedb: Vec<String>,
}

/// Performs consistency validation between SQLite and LanceDB
pub struct ConsistencyValidator<'a> {
    database: &'a Database,
    vector_store: &'a mut VectorStore,
}

impl<'a> ConsistencyValidator<'a> {
    /// Create a new consistency validator
    #[inline]
    pub fn new(database: &'a Database, vector_store: &'a mut VectorStore) -> Self {
        Self {
            database,
            vector_store,
        }
    }

    /// Perform a full consistency check between SQLite and LanceDB
    #[inline]
    pub async fn validate_consistency(&mut self) -> Result<ConsistencyReport> {
        info!("Starting cross-database consistency validation");

        // Get all indexed chunks from SQLite
        let sqlite_chunks = self.get_all_sqlite_chunks().await?;
        debug!("Found {} chunks in SQLite", sqlite_chunks.len());

        // Get all embeddings from LanceDB
        let lancedb_embeddings = self.get_all_lancedb_vector_ids().await?;
        debug!("Found {} embeddings in LanceDB", lancedb_embeddings.len());

        // Find missing and orphaned records
        let sqlite_vector_ids: HashSet<String> = sqlite_chunks
            .iter()
            .map(|(vector_id, _)| vector_id.clone())
            .collect();

        let lancedb_vector_ids: HashSet<String> = lancedb_embeddings.iter().cloned().collect();

        let missing_in_lancedb: Vec<String> = sqlite_vector_ids
            .difference(&lancedb_vector_ids)
            .cloned()
            .collect();

        let orphaned_in_lancedb: Vec<String> = lancedb_vector_ids
            .difference(&sqlite_vector_ids)
            .cloned()
            .collect();

        // Check consistency per site
        let inconsistent_sites =
            self.check_site_consistency(&sqlite_chunks, &lancedb_embeddings)?;

        let is_consistent = missing_in_lancedb.is_empty()
            && orphaned_in_lancedb.is_empty()
            && inconsistent_sites.is_empty();

        let report = ConsistencyReport {
            sqlite_chunks: sqlite_chunks.len(),
            lancedb_embeddings: lancedb_embeddings.len(),
            missing_in_lancedb,
            orphaned_in_lancedb,
            inconsistent_sites,
            is_consistent,
        };

        if report.is_consistent {
            info!("Database consistency validation passed");
        } else {
            warn!("Database consistency validation found issues");
            self.log_consistency_issues(&report);
        }

        Ok(report)
    }

    /// Clean up orphaned embeddings in LanceDB that don't have corresponding SQLite records
    #[inline]
    pub async fn cleanup_orphaned_embeddings(&mut self, vector_ids: &[String]) -> Result<usize> {
        if vector_ids.is_empty() {
            return Ok(0);
        }

        info!(
            "Cleaning up {} orphaned embeddings from LanceDB",
            vector_ids.len()
        );

        let mut cleaned_count = 0;
        for vector_id in vector_ids {
            match self.vector_store.delete_embedding(vector_id).await {
                Ok(true) => {
                    cleaned_count += 1;
                    debug!("Cleaned up orphaned embedding: {}", vector_id);
                }
                Ok(false) => {
                    warn!("Orphaned embedding not found for deletion: {}", vector_id);
                }
                Err(e) => {
                    error!("Failed to delete orphaned embedding {}: {}", vector_id, e);
                }
            }
        }

        info!(
            "Successfully cleaned up {} orphaned embeddings",
            cleaned_count
        );
        Ok(cleaned_count)
    }

    /// Regenerate missing embeddings for SQLite chunks that don't have LanceDB entries
    #[inline]
    pub async fn regenerate_missing_embeddings(&mut self, vector_ids: &[String]) -> Result<usize> {
        if vector_ids.is_empty() {
            return Ok(0);
        }

        info!("Regenerating {} missing embeddings", vector_ids.len());

        // Get the chunks that need regeneration
        let mut regenerated_count = 0;

        for vector_id in vector_ids {
            match self.regenerate_single_embedding(vector_id).await {
                Ok(()) => {
                    regenerated_count += 1;
                    debug!("Regenerated embedding for vector_id: {}", vector_id);
                }
                Err(e) => {
                    error!("Failed to regenerate embedding for {}: {}", vector_id, e);
                }
            }
        }

        info!("Successfully regenerated {} embeddings", regenerated_count);
        Ok(regenerated_count)
    }

    /// Get all indexed chunks from SQLite with their vector IDs and site information
    async fn get_all_sqlite_chunks(&self) -> Result<Vec<(String, (i64, String))>> {
        let sites = self
            .database
            .get_sites_by_status(crate::database::sqlite::SiteStatus::Completed)
            .await?;
        let mut all_chunks = Vec::new();

        for site in sites {
            let chunks = self.database.get_chunks_for_site(site.id).await?;
            for chunk in chunks {
                all_chunks.push((chunk.vector_id, (site.id, chunk.url)));
            }
        }

        Ok(all_chunks)
    }

    /// Get all vector IDs from LanceDB
    async fn get_all_lancedb_vector_ids(&mut self) -> Result<Vec<String>> {
        self.vector_store
            .list_all_vector_ids()
            .await
            .map_err(|e| anyhow::anyhow!(e))
    }

    /// Check consistency for each site individually
    fn check_site_consistency(
        &self,
        sqlite_chunks: &[(String, (i64, String))],
        lancedb_embeddings: &[String],
    ) -> Result<Vec<SiteConsistencyIssue>> {
        let mut site_chunks: HashMap<i64, (String, Vec<String>)> = HashMap::new();

        // Group chunks by site
        for (vector_id, (site_id, _url)) in sqlite_chunks {
            let entry = site_chunks
                .entry(*site_id)
                .or_insert_with(|| (format!("Site {}", site_id), Vec::new()));
            entry.1.push(vector_id.clone());
        }

        let mut inconsistent_sites = Vec::new();
        let lancedb_set: HashSet<String> = lancedb_embeddings.iter().cloned().collect();

        for (site_id, (site_name, site_vector_ids)) in site_chunks {
            let site_vector_set: HashSet<String> = site_vector_ids.iter().cloned().collect();

            let missing_in_lancedb: Vec<String> =
                site_vector_set.difference(&lancedb_set).cloned().collect();

            // Find embeddings in LanceDB that belong to this site but are orphaned
            // This is a simplified approach - in a real implementation, we'd need
            // site_id information from LanceDB metadata
            let orphaned_in_lancedb = Vec::new(); // Placeholder

            if !missing_in_lancedb.is_empty() || !orphaned_in_lancedb.is_empty() {
                inconsistent_sites.push(SiteConsistencyIssue {
                    site_id,
                    site_name,
                    sqlite_chunks: site_vector_ids.len(),
                    lancedb_embeddings: site_vector_ids.len() - missing_in_lancedb.len(),
                    missing_in_lancedb,
                    orphaned_in_lancedb,
                });
            }
        }

        Ok(inconsistent_sites)
    }

    /// Regenerate a single embedding for a vector ID
    async fn regenerate_single_embedding(&self, vector_id: &str) -> Result<()> {
        // Get the chunk from SQLite
        let _chunk = self.get_chunk_by_vector_id(vector_id).await?;

        // Re-extract content and generate embedding
        // This would require implementing the full pipeline again
        // For now, return an error indicating this needs implementation
        Err(DocsError::Database("Embedding regeneration not yet implemented".to_string()).into())
    }

    /// Get a chunk from SQLite by vector ID
    async fn get_chunk_by_vector_id(
        &self,
        vector_id: &str,
    ) -> Result<crate::database::sqlite::IndexedChunk> {
        let chunk = self.database.get_chunk_by_vector_id(vector_id).await?;
        chunk.ok_or_else(|| {
            DocsError::Database(format!("Chunk with vector_id {} not found", vector_id)).into()
        })
    }

    /// Log consistency issues for debugging
    fn log_consistency_issues(&self, report: &ConsistencyReport) {
        if !report.missing_in_lancedb.is_empty() {
            warn!(
                "Found {} chunks in SQLite missing from LanceDB",
                report.missing_in_lancedb.len()
            );
        }

        if !report.orphaned_in_lancedb.is_empty() {
            warn!(
                "Found {} orphaned embeddings in LanceDB",
                report.orphaned_in_lancedb.len()
            );
        }

        for site_issue in &report.inconsistent_sites {
            warn!(
                "Site {} ({}) has consistency issues: {} SQLite chunks, {} LanceDB embeddings",
                site_issue.site_name,
                site_issue.site_id,
                site_issue.sqlite_chunks,
                site_issue.lancedb_embeddings
            );
        }
    }
}

/// Utility functions for consistency validation
impl ConsistencyReport {
    /// Get a human-readable summary of the consistency report
    #[inline]
    pub fn summary(&self) -> String {
        if self.is_consistent {
            format!(
                "Database is consistent: {} chunks in SQLite, {} embeddings in LanceDB",
                self.sqlite_chunks, self.lancedb_embeddings
            )
        } else {
            format!(
                "Database inconsistencies found: {} missing in LanceDB, {} orphaned in LanceDB, {} sites with issues",
                self.missing_in_lancedb.len(),
                self.orphaned_in_lancedb.len(),
                self.inconsistent_sites.len()
            )
        }
    }

    /// Get the total number of consistency issues
    #[inline]
    pub fn total_issues(&self) -> usize {
        self.missing_in_lancedb.len() + self.orphaned_in_lancedb.len()
    }
}

#[cfg(test)]
mod tests {
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
        assert!(!report.is_consistent);
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
        assert!(report.is_consistent);
        assert!(report.summary().contains("Database is consistent"));
    }

    #[test]
    fn site_consistency_issue() {
        let issue = SiteConsistencyIssue {
            site_id: 1,
            site_name: "Test Site".to_string(),
            sqlite_chunks: 50,
            lancedb_embeddings: 48,
            missing_in_lancedb: vec!["vec1".to_string(), "vec2".to_string()],
            orphaned_in_lancedb: vec![],
        };

        assert_eq!(issue.missing_in_lancedb.len(), 2);
        assert_eq!(issue.orphaned_in_lancedb.len(), 0);
    }
}
