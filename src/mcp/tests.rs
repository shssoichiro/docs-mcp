//! MCP Protocol Implementation Tests
//!
//! Comprehensive unit tests for the MCP server implementation,
//! including protocol compliance, message handling, and error cases.

#[cfg(test)]
mod search_docs_tool_tests {
    use crate::mcp::tools::SearchDocsHandler;

    #[test]
    fn search_docs_tool_definition() {
        let tool = SearchDocsHandler::tool_definition();

        assert_eq!(tool.name, "search_docs");
        assert_eq!(
            tool.description,
            Some("Search indexed documentation".to_string())
        );

        // Verify required parameters
        let schema = tool.input_schema;
        let properties = schema["properties"].as_object().expect("has properties");

        assert!(properties.contains_key("query"));
        assert!(properties.contains_key("site_id"));
        assert!(properties.contains_key("sites_filter"));
        assert!(properties.contains_key("limit"));

        let required = schema["required"].as_array().expect("has required array");
        assert_eq!(required.len(), 1);
        assert_eq!(required[0], "query");
    }

    #[test]
    fn search_docs_parameter_validation() {
        let tool = SearchDocsHandler::tool_definition();
        let schema = tool.input_schema;

        // Verify query parameter
        let query_prop = &schema["properties"]["query"];
        assert_eq!(query_prop["type"], "string");
        assert_eq!(query_prop["description"], "Search query");

        // Verify site_id parameter
        let site_id_prop = &schema["properties"]["site_id"];
        assert_eq!(site_id_prop["type"], "integer");
        assert_eq!(
            site_id_prop["description"],
            "Optional: Search specific site by ID"
        );

        // Verify sites_filter parameter
        let sites_filter_prop = &schema["properties"]["sites_filter"];
        assert_eq!(sites_filter_prop["type"], "string");
        assert_eq!(
            sites_filter_prop["description"],
            "Optional: Regex pattern to filter sites (e.g., 'docs.rs')"
        );

        // Verify limit parameter
        let limit_prop = &schema["properties"]["limit"];
        assert_eq!(limit_prop["type"], "integer");
        assert_eq!(
            limit_prop["description"],
            "Maximum number of results (default: 10)"
        );
    }
}

#[cfg(test)]
mod list_sites_tool_tests {
    use crate::mcp::tools::ListSitesHandler;

    #[test]
    fn list_sites_tool_definition() {
        let tool = ListSitesHandler::tool_definition();

        assert_eq!(tool.name, "list_sites");
        assert_eq!(
            tool.description,
            Some("List available documentation sites".to_string())
        );

        // Should have no parameters
        let schema = tool.input_schema;
        let properties = schema["properties"].as_object().expect("has properties");
        assert!(properties.is_empty());
    }
}
