//! MCP Tools Implementation
//!
//! This module provides the tool registration and discovery system,
//! along with concrete tool implementations for documentation search.

use crate::database::lancedb::VectorStore;
use crate::database::sqlite::Database as SqliteDB;
use crate::embeddings::ollama::OllamaClient;
use crate::mcp::protocol::*;
use crate::mcp::server::ToolHandler;
use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, error};

/// Documentation search tool handler
pub struct SearchDocsHandler {
    sqlite_db: Arc<SqliteDB>,
    vector_store: Arc<VectorStore>,
    ollama_client: Arc<OllamaClient>,
}

/// List sites tool handler
pub struct ListSitesHandler {
    sqlite_db: Arc<SqliteDB>,
}

impl SearchDocsHandler {
    /// Create a new search docs handler
    #[inline]
    pub fn new(
        sqlite_db: Arc<SqliteDB>,
        vector_store: Arc<VectorStore>,
        ollama_client: Arc<OllamaClient>,
    ) -> Self {
        Self {
            sqlite_db,
            vector_store,
            ollama_client,
        }
    }

    /// Create the search_docs tool definition
    #[inline]
    pub fn tool_definition() -> Tool {
        Tool {
            name: "search_docs".to_string(),
            description: Some("Search indexed documentation".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Search query"
                    },
                    "site_id": {
                        "type": "integer",
                        "description": "Optional: Search specific site by ID"
                    },
                    "sites_filter": {
                        "type": "string",
                        "description": "Optional: Regex pattern to filter sites (e.g., 'docs.rs')"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results (default: 10)"
                    }
                },
                "required": ["query"],
                "additionalProperties": false
            }),
        }
    }
}

#[async_trait]
impl ToolHandler for SearchDocsHandler {
    #[inline]
    async fn handle(&self, params: CallToolParams) -> Result<CallToolResult> {
        let args = params.arguments.unwrap_or_default();

        // Extract and validate parameters
        let query = args
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing required parameter: query"))?;

        let site_id = args.get("site_id").and_then(|v| v.as_i64());
        let sites_filter = args.get("sites_filter").and_then(|v| v.as_str());

        let limit = args
            .get("limit")
            .and_then(|v| v.as_i64())
            .unwrap_or(10)
            .max(1) as usize;

        debug!(
            "Searching docs: query='{}', site_id={:?}, sites_filter={:?}, limit={}",
            query, site_id, sites_filter, limit
        );

        // Generate embedding for the query text
        let query_embedding = match self.ollama_client.generate_embedding(query) {
            Ok(result) => result.embedding,
            Err(e) => {
                error!("Failed to generate embedding for query: {}", e);
                return Ok(CallToolResult {
                    content: vec![ToolContent::Text {
                        text: format!("Failed to generate embedding for query: {}", e),
                    }],
                    is_error: Some(true),
                });
            }
        };

        // Handle site filtering - convert site_id or sites_filter to format expected by search_similar
        let site_filter_id = if let Some(id) = site_id {
            Some(id.to_string())
        } else if let Some(filter_pattern) = sites_filter {
            // For sites_filter, we need to get all sites and filter by regex pattern
            // For simplicity, we'll match against site names
            match self.sqlite_db.list_sites().await {
                Ok(sites) => {
                    // Simple pattern matching - if pattern is contained in site name or URL
                    let matching_sites: Vec<_> = sites
                        .into_iter()
                        .filter(|site| {
                            site.name.contains(filter_pattern)
                                || site.base_url.contains(filter_pattern)
                        })
                        .collect();

                    if matching_sites.is_empty() {
                        return Ok(CallToolResult {
                            content: vec![ToolContent::Text {
                                text: format!(
                                    "No sites found matching pattern '{}'. Use list_sites tool to see available sites.",
                                    filter_pattern
                                ),
                            }],
                            is_error: Some(true),
                        });
                    }

                    // For now, just use the first matching site ID
                    // TODO: Enhance vector search to support multiple site IDs
                    Some(matching_sites[0].id.to_string())
                }
                Err(e) => {
                    error!("Error listing sites for filter '{}': {}", filter_pattern, e);
                    return Ok(CallToolResult {
                        content: vec![ToolContent::Text {
                            text: format!("Error listing sites: {}", e),
                        }],
                        is_error: Some(true),
                    });
                }
            }
        } else {
            None
        };

        // Perform the search
        match self
            .vector_store
            .search_similar(&query_embedding, limit, site_filter_id.as_deref())
            .await
        {
            Ok(results) => {
                if results.is_empty() {
                    let empty_response = json!({
                        "results": []
                    });
                    return Ok(CallToolResult {
                        content: vec![ToolContent::Text {
                            text: serde_json::to_string_pretty(&empty_response)?,
                        }],
                        is_error: Some(false),
                    });
                }

                // Get site information for results
                let mut formatted_results = Vec::new();

                for result in results {
                    // Get site details from SQLite
                    let site_info = match self
                        .sqlite_db
                        .get_site_by_id(result.chunk_metadata.site_id.parse::<i64>().unwrap_or(0))
                        .await
                    {
                        Ok(Some(site)) => (site.name, site.version),
                        Ok(None) => ("Unknown Site".to_string(), "unknown".to_string()),
                        Err(_) => ("Unknown Site".to_string(), "unknown".to_string()),
                    };

                    let result_obj = json!({
                        "content": result.chunk_metadata.content,
                        "url": result.chunk_metadata.page_url,
                        "page_title": result.chunk_metadata.page_title,
                        "heading_path": result.chunk_metadata.heading_path.unwrap_or_else(|| "N/A".to_string()),
                        "site_name": site_info.0,
                        "site_version": site_info.1,
                        "relevance_score": result.similarity_score
                    });

                    formatted_results.push(result_obj);
                }

                let response = json!({
                    "results": formatted_results
                });

                Ok(CallToolResult {
                    content: vec![ToolContent::Text {
                        text: serde_json::to_string_pretty(&response)?,
                    }],
                    is_error: Some(false),
                })
            }
            Err(e) => {
                error!("Error performing search: {}", e);
                Ok(CallToolResult {
                    content: vec![ToolContent::Text {
                        text: format!("Search error: {}", e),
                    }],
                    is_error: Some(true),
                })
            }
        }
    }
}

impl ListSitesHandler {
    /// Create a new list sites handler
    #[inline]
    pub fn new(sqlite_db: Arc<SqliteDB>) -> Self {
        Self { sqlite_db }
    }

    /// Create the list_sites tool definition
    #[inline]
    pub fn tool_definition() -> Tool {
        Tool {
            name: "list_sites".to_string(),
            description: Some("List available documentation sites".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        }
    }
}

#[async_trait]
impl ToolHandler for ListSitesHandler {
    #[inline]
    async fn handle(&self, _params: CallToolParams) -> Result<CallToolResult> {
        debug!("Listing documentation sites");

        match self.sqlite_db.list_sites().await {
            Ok(sites) => {
                // Filter to only show completed sites to MCP clients (as per SPEC.md)
                let completed_sites: Vec<_> = sites
                    .into_iter()
                    .filter(|site| {
                        matches!(site.status, crate::database::sqlite::SiteStatus::Completed)
                    })
                    .collect();

                let mut site_list = Vec::new();

                for site in completed_sites {
                    let site_obj = json!({
                        "id": site.id,
                        "name": site.name,
                        "version": site.version,
                        "url": site.base_url,
                        "status": "completed",
                        "indexed_date": site.indexed_date.map(|d| d.format("%Y-%m-%dT%H:%M:%SZ").to_string()),
                        "page_count": site.indexed_pages
                    });
                    site_list.push(site_obj);
                }

                let response = json!({
                    "sites": site_list
                });

                Ok(CallToolResult {
                    content: vec![ToolContent::Text {
                        text: serde_json::to_string_pretty(&response)?,
                    }],
                    is_error: Some(false),
                })
            }
            Err(e) => {
                error!("Error listing sites: {}", e);
                Ok(CallToolResult {
                    content: vec![ToolContent::Text {
                        text: format!("Error listing sites: {}", e),
                    }],
                    is_error: Some(true),
                })
            }
        }
    }
}

/// Tool registry for managing tool registration
pub struct ToolRegistry {
    tools: HashMap<String, Tool>,
}

impl ToolRegistry {
    /// Create a new tool registry
    #[inline]
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool
    #[inline]
    pub fn register(&mut self, tool: Tool) {
        self.tools.insert(tool.name.clone(), tool);
    }

    /// Get all registered tools
    #[inline]
    pub fn list_tools(&self) -> Vec<Tool> {
        self.tools.values().cloned().collect()
    }

    /// Get a specific tool by name
    #[inline]
    pub fn get_tool(&self, name: &str) -> Option<&Tool> {
        self.tools.get(name)
    }

    /// Create the default tool registry with documentation tools
    #[inline]
    pub fn create_default() -> Self {
        let mut registry = Self::new();

        // Register default tools
        registry.register(SearchDocsHandler::tool_definition());
        registry.register(ListSitesHandler::tool_definition());

        registry
    }
}

impl Default for ToolRegistry {
    #[inline]
    fn default() -> Self {
        Self::create_default()
    }
}
