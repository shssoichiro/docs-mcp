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
use std::fmt::Write;
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
            description: Some("Search through indexed documentation using semantic similarity. Returns relevant documentation chunks with their metadata.".to_string()),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "The search query to find relevant documentation"
                    },
                    "site_filter": {
                        "type": "string",
                        "description": "Optional site name to filter results to a specific documentation site",
                        "nullable": true
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return (default: 10, max: 50)",
                        "minimum": 1,
                        "maximum": 50,
                        "default": 10
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

        let site_filter = args.get("site_filter").and_then(|v| v.as_str());

        let limit = args
            .get("limit")
            .and_then(|v| v.as_i64())
            .unwrap_or(10)
            .clamp(1, 50) as usize;

        debug!(
            "Searching docs: query='{}', site_filter={:?}, limit={}",
            query, site_filter, limit
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

        // Convert site filter to the format expected by search_similar
        let site_filter_str = if let Some(site_name) = site_filter {
            match self.sqlite_db.get_site_by_name(site_name).await {
                Ok(Some(site)) => Some(site.id.to_string()),
                Ok(None) => {
                    return Ok(CallToolResult {
                        content: vec![ToolContent::Text {
                            text: format!(
                                "Site '{}' not found. Use list_sites tool to see available sites.",
                                site_name
                            ),
                        }],
                        is_error: Some(true),
                    });
                }
                Err(e) => {
                    error!("Error looking up site '{}': {}", site_name, e);
                    return Ok(CallToolResult {
                        content: vec![ToolContent::Text {
                            text: format!("Error looking up site: {}", e),
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
            .search_similar(&query_embedding, limit, site_filter_str.as_deref())
            .await
        {
            Ok(results) => {
                if results.is_empty() {
                    return Ok(CallToolResult {
                        content: vec![ToolContent::Text {
                            text: "No relevant documentation found for your query.".to_string(),
                        }],
                        is_error: Some(false),
                    });
                }

                // Format results as text
                let mut response_text = format!(
                    "Found {} relevant documentation chunk(s):\n\n",
                    results.len()
                );

                for (i, result) in results.iter().enumerate() {
                    writeln!(
                        &mut response_text,
                        "{}. **{}** (Score: {:.3})",
                        i + 1,
                        result
                            .chunk_metadata
                            .heading_path
                            .as_deref()
                            .unwrap_or("Unknown"),
                        result.similarity_score
                    )
                    .expect("write to string should not fail");

                    writeln!(
                        &mut response_text,
                        "   URL: {}",
                        result.chunk_metadata.page_url
                    )
                    .expect("write to string should not fail");
                    writeln!(
                        &mut response_text,
                        "   Site: {}",
                        result.chunk_metadata.page_title
                    )
                    .expect("write to string should not fail");

                    writeln!(
                        &mut response_text,
                        "   Content: {}\n",
                        result
                            .chunk_metadata
                            .content
                            .chars()
                            .take(200)
                            .collect::<String>()
                    )
                    .expect("write to string should not fail");

                    if result.chunk_metadata.content.len() > 200 {
                        response_text.push_str("   ...\n\n");
                    }
                }

                Ok(CallToolResult {
                    content: vec![ToolContent::Text {
                        text: response_text,
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
            description: Some(
                "List all available indexed documentation sites with their status and statistics."
                    .to_string(),
            ),
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
                if sites.is_empty() {
                    return Ok(CallToolResult {
                        content: vec![ToolContent::Text {
                            text: "No documentation sites have been indexed yet.".to_string(),
                        }],
                        is_error: Some(false),
                    });
                }

                let mut response_text =
                    format!("Found {} indexed documentation site(s):\n\n", sites.len());

                for (i, site) in sites.iter().enumerate() {
                    writeln!(&mut response_text, "{}. **{}**", i + 1, site.name)
                        .expect("write to string should not fail");

                    writeln!(&mut response_text, "   URL: {}", site.base_url)
                        .expect("write to string should not fail");
                    writeln!(&mut response_text, "   Status: {:?}", site.status)
                        .expect("write to string should not fail");

                    writeln!(
                        &mut response_text,
                        "   Pages Crawled: {}",
                        site.indexed_pages
                    )
                    .expect("write to string should not fail");
                    writeln!(&mut response_text, "   Total Pages: {}", site.total_pages)
                        .expect("write to string should not fail");
                    writeln!(
                        &mut response_text,
                        "   Progress: {:.1}%",
                        site.progress_percent as f64
                    )
                    .expect("write to string should not fail");

                    writeln!(
                        &mut response_text,
                        "   Added: {}",
                        site.created_date.format("%Y-%m-%d %H:%M:%S UTC")
                    )
                    .expect("write to string should not fail");

                    if let Some(last_heartbeat) = site.last_heartbeat {
                        writeln!(
                            &mut response_text,
                            "   Last Activity: {}",
                            last_heartbeat.format("%Y-%m-%d %H:%M:%S UTC")
                        )
                        .expect("write to string should not fail");
                    }

                    writeln!(&mut response_text).expect("write to string should not fail");
                }

                Ok(CallToolResult {
                    content: vec![ToolContent::Text {
                        text: response_text,
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
